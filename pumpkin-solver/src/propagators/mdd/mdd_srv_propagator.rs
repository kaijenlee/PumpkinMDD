use crate::basic_types::Inconsistency::Conflict;
use crate::basic_types::{HashMap, HashSet};
use crate::engine::opaque_domain_event::OpaqueDomainEvent;
use crate::engine::propagation::contexts::PropagationContextWithTrailedValues;
use crate::engine::propagation::{
    EnqueueDecision, LocalId, PropagationContext, PropagationContextMut, Propagator, ReadDomains,
};
use crate::engine::{DomainEvents, EmptyDomain};
use crate::predicate;
use crate::predicates::{Predicate, PropositionalConjunction};
use crate::propagators::mdd::common::{EdgeStatus, EdgeWatchFlag, MddComponentType, NodeStatus};
use crate::variables::IntegerVariable;
use fnv::{FnvBuildHasher, FnvHashMap, FnvHashSet};
use mdd_compile::mdd::{MddEdge, MddGraph, MddNode};
use std::collections::hash_set::Iter;
use std::collections::VecDeque;

/// ['MddSRVPropagator'] is a propagator that uses provided multi-valued decision diagram (MDD) to propagate
/// the constraint represented by the MDD (see ['mdd_compile::mdd']) and supports extended resolutions with state reaching variables (SRV).
///
/// The propagator uses incremental propagation and explanations algorithms extended from \[1\] to support extended resolutions with state reaching variables.
///
/// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
#[derive(Debug)]
pub struct MddSRVPropagator<Var: std::fmt::Debug + Clone + std::hash::Hash + Eq + 'static> {
    mdd: MddGraph<Var>,

    /// The current state of the MDD, represented by domains of the variables that it involves
    ///
    /// The value at index `i` is the domain for `x[i]`
    current_model_domains: Vec<HashSet<i32>>,
    /// The domain changes that have been made since the last propagation, to be accounted for
    /// the next propagation.
    model_domain_changes: Vec<(Var, i32)>,
    /// The trail of edges that have been killed during the propagation
    trail: Vec<MddComponentType>,
    /// For each (var, val) pair (both srvs and model variables) that were removed due to inference, the limit keeps track the trail position
    /// before the propagation where the (var, val) was removed.
    ///
    /// This is used to restore the state of the MDD when backtracking occurs.
    limit: HashMap<(Var, i32), i32>,
    /// The status of each edge in the MDD at the current state.
    edge_status: HashMap<MddEdge, EdgeStatus>,
    /// Tracks each edge and their watch flags at the current state of the MDD.
    watched: HashMap<MddEdge, HashSet<EdgeWatchFlag>>,
    /// Maps between each (var, val) pair and the edges that are related to it.
    var_value_to_edges: HashMap<(Var, i32), HashSet<MddEdge>>,
    /// Tracks between each (var, val) pair and the edge that it is watching at the current state of the MDD.
    var_value_to_watched_edge: HashMap<(Var, i32), MddEdge>,
    /// Tracks between each node and the incoming edge it is watching at the current state of the MDD.
    node_to_watched_in_edge: HashMap<MddNode, MddEdge>,
    /// Tracks between each node and the outgoing edge it is watching at the current state of the MDD.
    node_to_watched_out_edge: HashMap<MddNode, MddEdge>,
    /// Maps between each node and its incoming edges.
    node_to_in_edges: HashMap<MddNode, HashSet<MddEdge>>,
    /// Maps between each node and its outgoing edges.
    node_to_out_edges: HashMap<MddNode, HashSet<MddEdge>>,
    /// Maps between each variable and its index in the MDD layers.
    model_var_to_index: HashMap<Var, usize>,
    /// The domain changes for the state reaching variables (SRV) that have been made since the last propagation, to be accounted for
    /// the next propagation.
    srv_domain_changes: Vec<(Var, i32)>,
    /// The current state of the MDD, w.r.t to the state reaching variables (SRV).
    current_srv_domain: HashMap<Var, HashSet<i32>>,
    /// The status of each node in the MDD at the current state.
    node_status: HashMap<MddNode, NodeStatus>,
    /// Maps between each state reaching variable (SRV) and the MDD node that represents it by the index in the domain of the SRV.
    srv_to_mdd_node: HashMap<Var, HashMap<usize, MddNode>>,
}

impl<Var: std::fmt::Debug + Clone + std::hash::Hash + Eq + 'static> MddSRVPropagator<Var>
where
    Var: IntegerVariable,
{
    pub(crate) fn new(mdd: MddGraph<Var>) -> Self {
        Self {
            mdd,
            current_model_domains: Vec::new(),
            model_domain_changes: Vec::new(),
            trail: Vec::new(),
            limit: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            edge_status: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            watched: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            var_value_to_edges: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            var_value_to_watched_edge: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            node_to_watched_in_edge: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            node_to_watched_out_edge: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            node_to_in_edges: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            node_to_out_edges: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            model_var_to_index: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            srv_domain_changes: Vec::new(),
            current_srv_domain: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            node_status: FnvHashMap::with_hasher(FnvBuildHasher::default()),
            srv_to_mdd_node: FnvHashMap::with_hasher(FnvBuildHasher::default()),
        }
    }
    
    pub fn get_all_srv(&self) -> HashSet<Var> {
        self.mdd.srv_layers.iter().cloned().collect()
    }

    /// The downward pass of the MDD incremental propagation algorithm extended from \[1\].
    /// For each node *potentially* killed from above, check if it is actually killed/dead by finding the support edge to replace the watcher edge for the node.
    /// Also, process children of the node to check if they are also killed from below.
    ///
    /// Returns a tupel set of (var, val) pairs for model variables and srvs that should potentially be propagated due to the corresponding watched edge being killed.
    ///
    /// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn downward_pass(
        &mut self,
        kfa: HashSet<MddNode>,
    ) -> (HashSet<(Var, i32)>, HashSet<(Var, i32)>) {
        // TODO may be use a set with ordering guarantees
        let mut pinf_model = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut pinf_srv = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut node_queue = VecDeque::from_iter(kfa.iter());

        while !node_queue.is_empty() {
            let node = node_queue.pop_front().unwrap();

            // Find support to replace watcher edger for the node
            for edge in self.node_to_in_edges.get(node).unwrap() {
                if *self.edge_status.get(edge).unwrap() == EdgeStatus::Alive {
                    let _ = self
                        .watched
                        .get_mut(self.node_to_watched_in_edge.get(node).unwrap())
                        .unwrap()
                        .remove(&EdgeWatchFlag::End);
                    let _ = self
                        .watched
                        .get_mut(edge)
                        .unwrap()
                        .insert(EdgeWatchFlag::End);
                    let _ = self.node_to_watched_in_edge.insert(*node, *edge);
                    break;
                }
            }

            // No support found meaning it is dead, i.e., all of its incoming edges are killed
            // If the node is dead, kill all of its outgoing edges
            if self.is_node_dead(self.node_to_in_edges.get(node).unwrap().iter())
                && node.layer < self.mdd.layers.len()
            {
                // TODO check this for handling removal of value from srv domain
                let _ = self.node_status.insert(*node, NodeStatus::Above);
                let _ =
                    pinf_srv.insert((self.mdd.srv_layers[self.layer_index_to_srv_index(node.layer)].clone(), node.index as i32));
                let _ = self.trail.push(MddComponentType::Node(*node));
                if self.node_to_out_edges.get(node).is_none() {
                    continue;
                }
                for edge in self.node_to_out_edges.get(node).unwrap() {
                    if self.edge_status.get(edge).unwrap() != &EdgeStatus::Alive {
                        continue;
                    }
                    let _ = self.edge_status.insert(*edge, EdgeStatus::Above);
                    let _ = self.trail.push(MddComponentType::Edge(*edge));
                    // Queue node that watching the current edge as its incoming edge to process it
                    if self
                        .watched
                        .get(edge)
                        .unwrap()
                        .contains(&EdgeWatchFlag::End)
                    {
                        let _ = node_queue.push_back(&edge.to);
                    }

                    // If a (var, val) watches the edge, add it to be processed by collect fn
                    if self
                        .watched
                        .get(edge)
                        .unwrap()
                        .contains(&EdgeWatchFlag::Value)
                    {
                        let _ = pinf_model
                            .insert((self.mdd.layers[edge.from.layer].clone(), edge.value));
                    }
                }
            }
        }
        (pinf_model, pinf_srv)
    }

    /// The upward pass of the MDD incremental propagation algorithm extended from \[1\].
    /// For each node *potentially* killed from below, check if it is actually killed/dead by finding the support edge to replace the watcher edge for the node.
    /// Also, process parent of the node to check if they are also killed from below.
    ///
    /// Returns a tuple set of (var, val) pairs for model variables and srvs that should potentially be propagated due to the corresponding watched edge being killed.
    ///
    ///[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn upward_pass(&mut self, kfb: HashSet<MddNode>) -> (HashSet<(Var, i32)>, HashSet<(Var, i32)>) {
        let mut pinf_model = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut pinf_srv = FnvHashSet::with_hasher(FnvBuildHasher::default());

        let mut node_queue = VecDeque::from_iter(kfb.iter());

        while !node_queue.is_empty() {
            let node = node_queue.pop_front().unwrap();

            // Find support to replace watcher edger for the node
            for edge in self.node_to_out_edges.get(node).unwrap() {
                if *self.edge_status.get(edge).unwrap() == EdgeStatus::Alive {
                    let _ = self
                        .watched
                        .get_mut(self.node_to_watched_out_edge.get(node).unwrap())
                        .unwrap()
                        .remove(&EdgeWatchFlag::Begin);
                    let _ = self
                        .watched
                        .get_mut(edge)
                        .unwrap()
                        .insert(EdgeWatchFlag::Begin);
                    let _ = self.node_to_watched_out_edge.insert(*node, *edge);
                    break;
                }
            }

            // No support found meaning it is dead, i.e., all of its outgoing edges are killed
            // If the node is dead, kill all of its incoming edges
            if self.is_node_dead(self.node_to_out_edges.get(node).unwrap().iter()) && node.layer > 0
            {
                // TODO check this for handling removal of value from srv domain
                let _ = self.node_status.insert(*node, NodeStatus::Below);
                let _ =
                    pinf_srv.insert((self.mdd.srv_layers[self.layer_index_to_srv_index(node.layer)].clone(), node.index as i32));
                let _ = self.trail.push(MddComponentType::Node(*node));
                if self.node_to_in_edges.get(node).is_none() {
                    continue;
                }
                for edge in self.node_to_in_edges.get(node).unwrap() {
                    if self.edge_status.get(edge).unwrap() != &EdgeStatus::Alive {
                        continue;
                    }
                    let _ = self.edge_status.insert(*edge, EdgeStatus::Below);
                    let _ = self.trail.push(MddComponentType::Edge(*edge));
                    // Queue node that watching the current edge as its incoming edge to process it
                    if self
                        .watched
                        .get(edge)
                        .unwrap()
                        .contains(&EdgeWatchFlag::End)
                    {
                        let _ = node_queue.push_back(&edge.from);
                    }

                    // If a (var, val) watches the edge, add it to be processed by collect fn
                    if self
                        .watched
                        .get(edge)
                        .unwrap()
                        .contains(&EdgeWatchFlag::Value)
                    {
                        let _ = pinf_model
                            .insert((self.mdd.layers[edge.from.layer].clone(), edge.value));
                    }
                }
            }
        }
        (pinf_model, pinf_srv)
    }

    /// Check if a node is dead by checking if all of its given (incoming or outgoing) edges are dead.
    fn is_node_dead(&self, edges: Iter<MddEdge>) -> bool {
        for edge in edges {
            if *self.edge_status.get(&edge).unwrap() == EdgeStatus::Alive {
                return false;
            }
        }
        true
    }

    /// Given two sets of (var, val) pairs for model variables and srvs that should be potentially propagated.
    ///
    /// For model variables, it is identified oftheir corresponding watched edge being killed from above or below, proceeded by
    /// checking if all the edges corresponding to the (var, val) pairs are dead via the watching scheme of \[1\].
    ///
    /// For state reaching variables (SRVs), it is identified whether they are killed from above or below, to determine the reasons for their value removals.
    ///
    /// Propagates the removal of (var, val) that are determined to be "dead".
    ///
    ///[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn collect_and_propagate(
        &mut self,
        pinf_model: HashSet<(Var, i32)>,
        pinf_srv: HashSet<(Var, i32)>,
        count: i32,
        mut context: PropagationContextMut,
    ) -> Result<(), EmptyDomain> {
        let mut inf_model: HashSet<(Var, i32)> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut inf_srv: HashSet<(Var, i32)> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        // // TODO optimize further memoization so that it is memoized for the entirety of the solver and capable of restoring upon backtrack
        // let mut killed_above_memo: HashMap<MddNode, bool> =
        //     FnvHashMap::with_hasher(FnvBuildHasher::default());
        // let mut killed_below_memo: HashMap<MddNode, bool> =
        //     FnvHashMap::with_hasher(FnvBuildHasher::default());

        for (var, val) in pinf_model {
            let mut edge = self
                .var_value_to_watched_edge
                .get(&(var.clone(), val))
                .unwrap();
            // Find new support edge for the value to watch
            for e in self.var_value_to_edges.get(&(var.clone(), val)).unwrap() {
                if self.edge_status.get(e).unwrap() == &EdgeStatus::Alive {
                    let _ = self
                        .watched
                        .get_mut(edge)
                        .unwrap()
                        .remove(&EdgeWatchFlag::Value);
                    let _ = self
                        .watched
                        .get_mut(e)
                        .unwrap()
                        .insert(EdgeWatchFlag::Value);
                    let _ = self
                        .var_value_to_watched_edge
                        .insert((var.clone(), val), *e);
                    edge = e;
                    break;
                }
            }
            // No support found meaning the (var, val) can be propagated
            if self.edge_status.get(edge).unwrap() != &EdgeStatus::Alive {
                let _ = inf_model.insert((var.clone(), val));
                let _ = self.limit.insert((var.clone(), val), count);
            }
        }

        for (var, val) in pinf_srv {
            let _ = inf_srv.insert((var.clone(), val));
            let _ = self.limit.insert((var.clone(), val), count);
        }

        // Propagate model variables' value removals
        for (var, val) in inf_model {
            let reason = self.explain_model_value_removal(var.clone(), val, context.as_readonly());
            context.remove(&var, val, reason)?;
        }
        for (var, val) in inf_srv {
            let reason = self.explain_srv_value_removal(var.clone(), val, context.as_readonly());
            context.remove(&var, val, reason)?;
        }
        for (i, x_i) in self.mdd.layers.iter().enumerate() {
            // Reset the current model domains to the original domains
            self.current_model_domains[i] = context.iterate_domain(x_i).collect::<HashSet<i32>>();
        }
        for y_i in self.mdd.srv_layers.iter() {
            // Reset the current srv domains to the original domains
            let _ = self.current_srv_domain.insert(
                y_i.clone(),
                context
                    .iterate_domain(&y_i.clone())
                    .collect::<HashSet<i32>>(),
            );
        }
        Ok(())
    }

    /// Function to restore the state of the MDD when backtracking occurs.
    fn restore_to(&mut self, var: Var, val: i32) {
        let limit = self.limit.get(&(var, val)).unwrap();
        while self.trail.len() > *limit as usize {
            let component = self.trail.pop().unwrap();
            match component {
                MddComponentType::Edge(edge) => {
                    let _ = self.edge_status.insert(edge, EdgeStatus::Alive);
                }
                MddComponentType::Node(node) => {
                    // shouldn't have the need to restore edges statuses because they will all be covered by the same limit/trail mechanism. but check this
                    let _ = self.node_status.insert(node, NodeStatus::Alive);
                    let _ = self
                        .current_srv_domain
                        .get_mut(&self.mdd.srv_layers[self.layer_index_to_srv_index(node.layer)])
                        .unwrap()
                        .insert(node.index as i32);
                }
            }
        }
    }

    /// Performs BFS to check if there is a path from the source to the sink in the MDD following
    /// edges that are alive. Used to check if the MDD is valid during initialization.
    fn check_path_from_source_to_sink(
        &self,
        _context: &mut crate::engine::propagation::PropagatorInitialisationContext,
    ) -> Result<(), PropositionalConjunction> {
        let mut node_queue = VecDeque::new();
        node_queue.push_back(MddNode::source());
        while !node_queue.is_empty() {
            let node = node_queue.pop_front().unwrap();
            if node == self.mdd.sink {
                return Ok(());
            }
            if let None = self.node_to_out_edges.get(&node) {
                continue;
            }
            for edge in self.node_to_out_edges.get(&node).unwrap() {
                if *self.edge_status.get(edge).unwrap() == EdgeStatus::Alive {
                    node_queue.push_back(edge.to);
                }
            }
        }
        // Explain with domain
        Err(PropositionalConjunction::new(self.mdd.layers.iter().fold(
            Vec::new(),
            |mut acc, x| {
                acc.extend(vec![
                    predicate![x >= _context.lower_bound(x)],
                    predicate![x <= _context.upper_bound(x)],
                ]);
                acc
            },
        )))
    }

    /// The incremental explanation algorithm for model variables extended from \[1\] to also consider SRVs killed by domain change that may be a part of the reason of the propagation.
    /// Produces a sufficiently small reason without exploring the whole MDD graph.
    ///
    /// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn explain_model_value_removal(
        &self,
        var: Var,
        val: i32,
        _context: PropagationContext,
    ) -> PropositionalConjunction {
        let mut kfa: HashSet<MddEdge> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut kfb: HashSet<MddEdge> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        for edge in self.var_value_to_edges.get(&(var.clone(), val)).unwrap() {
            if *self.edge_status.get(edge).unwrap() == EdgeStatus::Above {
                let _ = kfa.insert(*edge);
            } else if *self.edge_status.get(edge).unwrap() == EdgeStatus::Below {
                let _ = kfb.insert(*edge);
            }
        }
        let mut predicates = Vec::new();
        predicates.extend(self.explain_down(kfb));
        predicates.extend(self.explain_up(kfa));
        PropositionalConjunction::new(predicates)
    }

    /// The incremental explanation algorithm for state reaching variables (SRV) inspired by the approach for model variables in \[1\].
    ///
    /// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn explain_srv_value_removal(
        &mut self,
        var: Var,
        val: i32,
        _context: PropagationContext,
    ) -> PropositionalConjunction {
        let mdd_node = self
            .srv_to_mdd_node
            .get(&var)
            .expect("srv variable should have a corresponding MDD node")
            .get(&(val as usize))
            .expect("srv variable should have a corresponding MDD node at the given index");
        if *self.node_status.get(mdd_node).unwrap() == NodeStatus::Above {
            PropositionalConjunction::new(
                self.explain_up(self.node_to_in_edges.get(mdd_node).unwrap().clone()),
            )
        } else {
            PropositionalConjunction::new(
                self.explain_down(self.node_to_out_edges.get(mdd_node).unwrap().clone()),
            )
        }
    }

    /// Produces explanations for edges related to the propagated (var, val) removal that are killed from bellow.
    fn explain_down(&self, kfb: HashSet<MddEdge>) -> Vec<Predicate> {
        let mut reason_predicates = Vec::new();
        let mut reason: HashSet<(Var, i32)> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut current_kfb = kfb.clone();
        while !current_kfb.is_empty() {
            let mut pending: HashSet<MddEdge> = FnvHashSet::with_hasher(FnvBuildHasher::default());
            for edge in current_kfb.iter() {
                if *self.edge_status.get(edge).unwrap() == EdgeStatus::Dom
                    && *self.node_status.get(&edge.to).unwrap() != NodeStatus::Below
                {
                    let model_var = self.mdd.layers[edge.from.layer].clone();
                    reason_predicates.push(predicate![model_var != edge.value]);
                    let _ = reason.insert((model_var, edge.value));
                } else if *self.node_status.get(&edge.to).unwrap() == NodeStatus::Dom {
                    let srv_var = self.mdd.srv_layers[self.layer_index_to_srv_index(edge.to.layer)].clone();
                    reason_predicates.push(predicate![srv_var != edge.to.index as i32]);
                    let _ = reason.insert((srv_var, edge.to.index as i32));
                } else {
                    let _ = pending.insert(*edge);
                }
            }

            let mut next_kfb = FnvHashSet::with_hasher(FnvBuildHasher::default());
            for edge in pending.iter() {
                if !reason.contains(&(self.mdd.layers[edge.from.layer].clone(), edge.value)) {
                    if let Some(out_edges) = self.node_to_out_edges.get(&edge.to) {
                        next_kfb.extend(out_edges);
                    }
                }
            }
            current_kfb = next_kfb;
        }
        reason_predicates
    }

    // /// Checks if a node is killed from below or above
    // ///
    // /// Results are memoized for each propagation call
    // fn killed_below(
    //     &mut self,
    //     node: MddNode,
    //     killed_below_memo: &mut HashMap<MddNode, bool>,
    // ) -> bool {
    //     if let Some(result) = killed_below_memo.get(&node) {
    //         return *result;
    //     }
    //     if let Some(out_edges) = self.node_to_out_edges.get(&node) {
    //         for edge in out_edges {
    //             if *self.edge_status.get(edge).unwrap() == EdgeStatus::Alive
    //                 || *self.edge_status.get(edge).unwrap() == EdgeStatus::Above
    //             {
    //                 let _ = killed_below_memo.insert(node, false);
    //                 return false;
    //             }
    //         }
    //     }
    //     let _ = killed_below_memo.insert(node, true);
    //     true
    // }

    /// Produces explanations for edges related to the propagated (var, val) removal that are killed from above.
    fn explain_up(&self, kfa: HashSet<MddEdge>) -> Vec<Predicate> {
        let mut reason_predicates = Vec::new();
        let mut reason: HashSet<(Var, i32)> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut current_kfa = kfa.clone();
        while !current_kfa.is_empty() {
            let mut pending: HashSet<MddEdge> = FnvHashSet::with_hasher(FnvBuildHasher::default());
            for edge in current_kfa.iter() {
                if *self.edge_status.get(edge).unwrap() == EdgeStatus::Dom
                    && *self.node_status.get(&edge.from).unwrap() != NodeStatus::Above
                {
                    let model_var = self.mdd.layers[edge.from.layer].clone();
                    reason_predicates.push(predicate![model_var != edge.value]);
                    let _ = reason.insert((model_var, edge.value));
                } else if *self.node_status.get(&edge.to).unwrap() == NodeStatus::Dom {
                    let srv_var = self.mdd.srv_layers[self.layer_index_to_srv_index(edge.to.layer)].clone();
                    reason_predicates.push(predicate![srv_var != edge.to.index as i32]);
                    let _ = reason.insert((srv_var, edge.to.index as i32));
                } else {
                    let _ = pending.insert(*edge);
                }
            }

            let mut next_kfa = FnvHashSet::with_hasher(FnvBuildHasher::default());
            for edge in pending.iter() {
                if !reason.contains(&(self.mdd.layers[edge.from.layer].clone(), edge.value)) {
                    if let Some(in_edges) = self.node_to_in_edges.get(&edge.from) {
                        next_kfa.extend(in_edges);
                    }
                }
            }
            current_kfa = next_kfa;
        }
        reason_predicates
    }

    // /// Checks if a node is killed from above
    // ///
    // /// Results are memoized for each propagation call
    // fn killed_above(
    //     &mut self,
    //     node: MddNode,
    //     killed_above_memo: &mut HashMap<MddNode, bool>,
    // ) -> bool {
    //     if let Some(result) = killed_above_memo.get(&node) {
    //         return *result;
    //     }
    //     if let Some(in_edges) = self.node_to_in_edges.get(&node) {
    //         for edge in in_edges {
    //             if *self.edge_status.get(edge).unwrap() == EdgeStatus::Alive
    //                 || *self.edge_status.get(edge).unwrap() == EdgeStatus::Below
    //             {
    //                 let _ = killed_above_memo.insert(node, false);
    //                 return false;
    //             }
    //         }
    //     }
    //     let _ = killed_above_memo.insert(node, true);
    //     true
    // }

    /// Helper function to encourage VSIDS to branch on nodes with the most edges, via SRV
    pub(crate) fn get_node_with_most_active_edges(
        &self,
    ) -> Option<Predicate> {

        let mut max_active_edges = 0;
        let best_node = self.srv_to_mdd_node.iter().fold(None, |current_best, (_, nodes)| {
            let mut best_node: Option<MddNode> = current_best;
            for (_, node) in nodes.iter() {
                let active_in_edges = self.node_to_in_edges.get(node).unwrap().iter().filter(|e| self.edge_status.get(e).unwrap() == &EdgeStatus::Alive).count();
                let active_out_edges = self.node_to_out_edges.get(node).unwrap().iter().filter(|e| self.edge_status.get(e).unwrap() == &EdgeStatus::Alive).count();
                let active_edges = active_in_edges + active_out_edges;
                if active_edges > max_active_edges {
                    max_active_edges = active_edges;
                    best_node = Some(*node);
                }
            }
            best_node
        });
        match best_node {
            Some(node) => {
                let var = self.mdd.srv_layers[self.layer_index_to_srv_index(node.layer)].clone();
                let value = node.index as i32;
                Some(predicate![var == value])
            }
            None => None,
        }

    }

    /// Helper function to convert layer index to srv_index as there is no srv for the source node/layer
    fn layer_index_to_srv_index(&self, layer_index: usize) -> usize {
        layer_index - 1
    }

}

impl<Var: std::fmt::Debug + Clone + std::hash::Hash + Eq + 'static> Propagator
    for MddSRVPropagator<Var>
where
    Var: IntegerVariable,
{
    fn name(&self) -> &str {
        "DecisionDiagram"
    }

    fn debug_propagate_from_scratch(
        &self,
        _context: PropagationContextMut,
    ) -> crate::basic_types::PropagationStatusCP {
        todo!()
    }

    fn propagate(
        &mut self,
        mut _context: PropagationContextMut,
    ) -> crate::basic_types::PropagationStatusCP {
        // Propagate any infeasible value, i.e. there are no edges corresponding to that assignement
        // If the edges are not present in the MDD, it means that the assignment would not be present in any feasible solution
        for var in self.mdd.layers.iter() {
            let domain = _context.iterate_domain(var).collect::<HashSet<i32>>();
            for value in domain {
                if !self.var_value_to_edges.contains_key(&(var.clone(), value)) {
                    _context.remove(var, value, PropositionalConjunction::new(vec![]))?;
                }
            }
        }

        let mut kfb_nodes: HashSet<MddNode> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut kfa_nodes: HashSet<MddNode> = FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut pinf_model: HashSet<(Var, i32)> =
            FnvHashSet::with_hasher(FnvBuildHasher::default());
        let mut pinf_srv: HashSet<(Var, i32)> = FnvHashSet::with_hasher(FnvBuildHasher::default());

        let count: i32 = self.trail.len() as i32;

        // Handle domain changes for model variables
        for (x_i, value) in self.model_domain_changes.iter() {
            // Mark restoration point
            let _ = self.limit.insert((x_i.clone(), *value), count);

            // Kill remaining edges for current (x_i, val) as val is no longer in the domain of x_i
            for edge in self.var_value_to_edges.get(&(x_i.clone(), *value)).unwrap() {
                if *self.edge_status.get(edge).unwrap() != EdgeStatus::Alive {
                    continue;
                }
                let _ = self.edge_status.insert(*edge, EdgeStatus::Dom);
                self.trail.push(MddComponentType::Edge(*edge));
                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::Begin) {
                    let _ = kfb_nodes.insert(edge.from);
                }

                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::End) {
                    let _ = kfa_nodes.insert(edge.to);
                }
            }

            // Remove the value from the domain of x_i (our state) after processing it
            let _ = self
                .current_model_domains
                .get_mut(*self.model_var_to_index.get(x_i).unwrap())
                .unwrap()
                .remove(value);
        }

        // Handle domain changes for srv variables
        for (srv, value) in self.srv_domain_changes.iter() {
            let node = self
                .srv_to_mdd_node
                .get(srv)
                .expect("srv variable should have a corresponding MDD node")
                .get(&(*value as usize))
                .expect("srv variable should have a corresponding MDD node at the given index");

            if self.node_status.get(node).unwrap() != &NodeStatus::Alive {
                // If the node is dead, we can skip it
                continue;
            }

            // Mark restoration point
            let _ = self.limit.insert((srv.clone(), *value), count);

            // Kill the node
            let _ = self.node_status.insert(*node, NodeStatus::Dom);
            self.trail.push(MddComponentType::Node(*node));
            // Kill all incoming edges of the node and queue subsequent nodes to process as they may be killed from below
            for edge in self.node_to_in_edges.get(node).unwrap() {
                if *self.edge_status.get(edge).unwrap() != EdgeStatus::Alive {
                    continue;
                }
                let _ = self.edge_status.insert(*edge, EdgeStatus::Below);
                self.trail.push(MddComponentType::Edge(*edge)); //TODO Check this
                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::Begin) {
                    let _ = kfb_nodes.insert(edge.from);
                }
                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::Value) {
                    let _ =
                        pinf_model.insert((self.mdd.layers[edge.from.layer].clone(), edge.value));
                }
            }

            // Kill all outgoing edges of the node and queue subsequent nodes to process as they may be killed from above
            for edge in self.node_to_out_edges.get(node).unwrap() {
                if *self.edge_status.get(edge).unwrap() != EdgeStatus::Alive {
                    continue;
                }
                let _ = self.edge_status.insert(*edge, EdgeStatus::Above);
                self.trail.push(MddComponentType::Edge(*edge));
                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::End) {
                    let _ = kfa_nodes.insert(edge.to);
                }
                if (*self.watched.get(edge).unwrap()).contains(&EdgeWatchFlag::Value) {
                    //TODO: case from incoming side is correct, but haven't verified this case
                    let _ =
                        pinf_model.insert((self.mdd.layers[edge.from.layer].clone(), edge.value));
                }
            }

            // Remove the value from the domain of srv (our state) after processing it
            let _ = self.current_srv_domain.get_mut(srv).unwrap().remove(value);
        }

        let (d_pinf_model, d_pinf_srv) = self.downward_pass(kfa_nodes);

        let _ = pinf_model.extend(d_pinf_model);
        let _ = pinf_srv.extend(d_pinf_srv);

        if *self
            .edge_status
            .get(self.node_to_watched_in_edge.get(&self.mdd.sink).unwrap())
            .unwrap()
            != EdgeStatus::Alive
        {
            // If the sink is dead, it is due to watched incoming edge being killed
            let edge_involved = self.node_to_watched_in_edge.get(&self.mdd.sink).unwrap();
            let var_val = (
                self.mdd.layers[edge_involved.from.layer].clone(),
                edge_involved.value,
            );
            return Err(Conflict(self.explain_model_value_removal(
                var_val.0.clone(),
                var_val.1,
                _context.as_readonly(),
            )));
        }
        let (u_pinf_model, u_pinf_srv) = self.upward_pass(kfb_nodes);
        let _ = pinf_model.extend(u_pinf_model);
        let _ = pinf_srv.extend(u_pinf_srv);
        self.collect_and_propagate(pinf_model, pinf_srv, count, _context)?;
        // Reset the following at the end of the propagation
        self.model_domain_changes.clear();
        self.srv_domain_changes.clear();
        Ok(())
    }

    fn notify(
        &mut self,
        _context: PropagationContextWithTrailedValues,
        _local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        let index = _local_id.unpack() as usize;

        // Index < self.mdd.layers.len() indicates that we are handling model variables
        if index < self.mdd.layers.len() {
            let x_i = &self.mdd.layers[index];
            let old_domain_set = self.current_model_domains[index].clone();
            let new_domain_set = _context.iterate_domain(x_i).collect::<HashSet<i32>>();
            // Assert if the new domain is a subset of the old domain
            assert!(new_domain_set.is_subset(&old_domain_set));
            let values_removed = old_domain_set.difference(&new_domain_set);
            values_removed.into_iter().for_each(|x| {
                // If the (x_i, x) pair does not have any edges, we can skip it
                if !self.var_value_to_edges.contains_key(&(x_i.clone(), *x)) {
                    return;
                }
                self.model_domain_changes.push((x_i.clone(), *x));
            });
        } else {
            // Index >= self.mdd.layers.len() indicates that we are handling srv variables
            let srv_index = index - self.mdd.layers.len();
            let srv = &self.mdd.srv_layers[srv_index];
            let old_domain_set = self.current_srv_domain.get(srv).unwrap().clone();
            let new_domain_set = _context.iterate_domain(srv).collect::<HashSet<i32>>();
            // Assert if the new domain is a subset of the old domain
            assert!(new_domain_set.is_subset(&old_domain_set));
            let values_removed = old_domain_set.difference(&new_domain_set);
            values_removed.into_iter().for_each(|x| {
                // If the srv in the propagator's current state does not contain the value x, we can skip it
                // The corresponding node should be dead TODO: check if this is correct
                if !self.current_srv_domain.get(srv).unwrap().contains(x) {
                    return;
                }
                self.srv_domain_changes.push((srv.clone(), *x));
            });
        }

        EnqueueDecision::Enqueue
    }
    fn notify_backtrack(
        &mut self,
        _context: PropagationContext,
        _local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) {
        let index = _local_id.unpack() as usize;
        // Index < self.mdd.layers.len() indicates that we are handling model variables
        if index < self.mdd.layers.len() {
            let x_i = self.mdd.layers[index].clone();
            let old_domain_set = self.current_model_domains[index].clone();
            let new_domain_set = _context.iterate_domain(&x_i).collect::<HashSet<i32>>();
            // Assert if the old domain is a subset of the new domain, to backtrack to
            assert!(old_domain_set.is_subset(&new_domain_set));
            let values_returned = new_domain_set.difference(&old_domain_set);
            values_returned.into_iter().for_each(|val| {
                if !self.var_value_to_edges.contains_key(&(x_i.clone(), *val)) {
                    return;
                }
                self.restore_to(x_i.clone(), *val)
            });
            self.current_model_domains[index] = new_domain_set;
        } else {
            // Index >= self.mdd.layers.len() indicates that we are handling srv variables
            let srv_index = index - self.mdd.layers.len();
            let srv = self.mdd.srv_layers[srv_index].clone();
            let old_domain_set = self.current_srv_domain.get(&srv).unwrap().clone();
            let new_domain_set = _context.iterate_domain(&srv).collect::<HashSet<i32>>();
            // Assert if the old domain is a subset of the new domain, to backtrack to
            assert!(old_domain_set.is_subset(&new_domain_set));
            let values_returned = new_domain_set.difference(&old_domain_set);
            values_returned
                .into_iter()
                .for_each(|val| self.restore_to(srv.clone(), *val));
        }
        self.model_domain_changes.clear(); // Reset previously recorded domain changes
        self.srv_domain_changes.clear(); // Reset previously recorded srv domain changes
    }

    fn initialise_at_root(
        &mut self,
        _context: &mut crate::engine::propagation::PropagatorInitialisationContext,
    ) -> Result<(), PropositionalConjunction> {
        let mut layer_to_node = FnvHashMap::with_hasher(FnvBuildHasher::default());
        self.mdd.layers.iter().enumerate().for_each(|(i, x_i)| {
            let _ = _context.register(x_i.clone(), DomainEvents::ANY_INT, LocalId::from(i as u32));
            let _ = _context.register_for_backtrack_events(
                x_i.clone(),
                DomainEvents::ANY_INT,
                LocalId::from(i as u32),
            );
            let domain_set = _context.iterate_domain(x_i).collect::<HashSet<i32>>();
            self.current_model_domains.push(domain_set);
            let _ = self.model_var_to_index.insert(x_i.clone(), i);
        });
        self.mdd.transitions.iter().for_each(|edge| {
            let _ = self
                .node_to_in_edges
                .entry(edge.to)
                .or_insert(FnvHashSet::with_hasher(FnvBuildHasher::default()))
                .insert(edge.clone());
            let _ = self
                .node_to_out_edges
                .entry(edge.from)
                .or_insert(FnvHashSet::with_hasher(FnvBuildHasher::default()))
                .insert(edge.clone());
            let _ = self
                .var_value_to_edges
                .entry((self.mdd.layers[edge.from.layer].clone(), edge.value))
                .or_insert(FnvHashSet::with_hasher(FnvBuildHasher::default()))
                .insert(edge.clone());
            let _ = self.edge_status.insert(edge.clone(), EdgeStatus::Alive);
            let _ = self.watched.insert(
                edge.clone(),
                FnvHashSet::with_hasher(FnvBuildHasher::default()),
            );
            let _ = self.node_status.insert(edge.to, NodeStatus::Alive);
            let _ = self.node_status.insert(edge.from, NodeStatus::Alive);

            let _ = layer_to_node
                .entry(edge.to.layer)
                .or_insert(FnvHashMap::with_hasher(FnvBuildHasher::default()))
                .insert(edge.to.index, edge.to);
            let _ = layer_to_node
                .entry(edge.from.layer)
                .or_insert(FnvHashMap::with_hasher(FnvBuildHasher::default()))
                .insert(edge.from.index, edge.from);
        });
        self.mdd.srv_layers.iter().enumerate().for_each(|(i, srv)| {
            // TODO check if i corresponds to the ith layer
            let id = i + self.mdd.layers.len();
            let _ = _context.register(srv.clone(), DomainEvents::ANY_INT, LocalId::from(id as u32));
            let _ = _context.register_for_backtrack_events(
                srv.clone(),
                DomainEvents::ANY_INT,
                LocalId::from(id as u32),
            );
            self.current_srv_domain
                .entry(srv.clone())
                .or_insert(FnvHashSet::with_hasher(FnvBuildHasher::default()))
                .extend(_context.iterate_domain(srv));
            // Note: conversion from layer index to srv index is done by adding 1 to the layer index
            let _ = self
                .srv_to_mdd_node
                .insert(srv.clone(), layer_to_node.get(&(i + 1)).unwrap().clone());
        });

        self.node_to_in_edges.keys().for_each(|node| {
            let incoming_edge = *self
                .node_to_in_edges
                .get(node)
                .unwrap()
                .iter()
                .nth(0)
                .unwrap();
            let _ = self.node_to_watched_in_edge.insert(*node, incoming_edge);
            let _ = self
                .watched
                .get_mut(&incoming_edge)
                .unwrap()
                .insert(EdgeWatchFlag::End);
        });

        self.node_to_out_edges.keys().for_each(|node| {
            let outgoing_edge = *self
                .node_to_out_edges
                .get(node)
                .unwrap()
                .iter()
                .nth(0)
                .unwrap();
            let _ = self.node_to_watched_out_edge.insert(*node, outgoing_edge);
            let _ = self
                .watched
                .get_mut(&outgoing_edge)
                .unwrap()
                .insert(EdgeWatchFlag::Begin);
        });

        self.var_value_to_edges.keys().for_each(|var_value| {
            let edge = *self
                .var_value_to_edges
                .get(var_value)
                .unwrap()
                .iter()
                .nth(0)
                .unwrap();
            let _ = self
                .var_value_to_watched_edge
                .insert(var_value.clone(), edge);
            let _ = self
                .watched
                .get_mut(&edge)
                .unwrap()
                .insert(EdgeWatchFlag::Value);
        });

        self.check_path_from_source_to_sink(_context)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::propagation::EnqueueDecision;
    use crate::engine::test_solver::TestSolver;
    use crate::propagators::mdd::mdd_srv_propagator::MddSRVPropagator;
    use crate::variables::DomainId;
    use crate::{conjunction, predicate};
    use mdd_compile::mdd::{MddEdge, MddGraph, MddNode};

    /// Creates a BDD example from \[1\] "for a regular constraint 0\*1100\*110\* over the variables
    /// \[x0, x1, x2, x3, x4, x5, x6\], and the effect of propagating x2 != 1 and x3 != 1",
    ///
    /// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
    fn create_bdd_regular_gange(solver: &mut TestSolver) -> MddGraph<DomainId> {
        let x0 = solver.new_variable(0, 1);
        let x1 = solver.new_variable(0, 1);
        let x2 = solver.new_variable(0, 1);
        let x3 = solver.new_variable(0, 1);
        let x4 = solver.new_variable(0, 1);
        let x5 = solver.new_variable(0, 1);
        let x6 = solver.new_variable(0, 1);
        let layers = vec![x0, x1, x2, x3, x4, x5, x6];
        let sink = MddNode { layer: 7, index: 0 };
        let transitions = vec![
            // layer 0 -> 1
            MddEdge {
                from: MddNode { layer: 0, index: 0 },
                to: MddNode { layer: 1, index: 0 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 0, index: 0 },
                to: MddNode { layer: 1, index: 1 },
                value: 0,
            },
            // layer 1 -> 2
            MddEdge {
                from: MddNode { layer: 1, index: 0 },
                to: MddNode { layer: 2, index: 0 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 1, index: 1 },
                to: MddNode { layer: 2, index: 1 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 1, index: 1 },
                to: MddNode { layer: 2, index: 2 },
                value: 0,
            },
            // layer 2 -> 3
            MddEdge {
                from: MddNode { layer: 2, index: 0 },
                to: MddNode { layer: 3, index: 0 },
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 2, index: 1 },
                to: MddNode { layer: 3, index: 1 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 2, index: 2 },
                to: MddNode { layer: 3, index: 2 },
                value: 1,
            },
            // layer 3 -> 4
            MddEdge {
                from: MddNode { layer: 3, index: 0 },
                to: MddNode { layer: 4, index: 0 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 3, index: 0 },
                to: MddNode { layer: 4, index: 1 },
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 3, index: 1 },
                to: MddNode { layer: 4, index: 1 },
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 3, index: 2 },
                to: MddNode { layer: 4, index: 2 },
                value: 1,
            },
            // layer 4 -> 5
            MddEdge {
                from: MddNode { layer: 4, index: 0 },
                to: MddNode { layer: 5, index: 0 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 4, index: 1 },
                to: MddNode { layer: 5, index: 1 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 4, index: 1 },
                to: MddNode { layer: 5, index: 2 },
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 4, index: 2 },
                to: MddNode { layer: 5, index: 2 },
                value: 0,
            },
            // layer 5 -> 6
            MddEdge {
                from: MddNode { layer: 5, index: 0 },
                to: MddNode { layer: 6, index: 0 },
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 5, index: 1 },
                to: MddNode { layer: 6, index: 0 },
                value: 1,
            },
            MddEdge {
                from: MddNode { layer: 5, index: 2 },
                to: MddNode { layer: 6, index: 1 },
                value: 1,
            },
            // layer 6 -> 7 (terminal node T)
            MddEdge {
                from: MddNode { layer: 6, index: 0 },
                to: sink, // Terminal node T
                value: 0,
            },
            MddEdge {
                from: MddNode { layer: 6, index: 1 },
                to: sink, // Terminal node T
                value: 1,
            },
        ];
        let y1 = solver.new_variable(0, 1);
        let y2 = solver.new_variable(0, 2);
        let y3 = solver.new_variable(0, 2);
        let y4 = solver.new_variable(0, 2);
        let y5 = solver.new_variable(0, 2);
        let y6 = solver.new_variable(0, 1);
        let mdd = MddGraph {
            layers: layers.clone(),
            srv_layers: vec![y1, y2, y3, y4, y5, y6],
            transitions,
            sink,
        };
        mdd
    }

    #[test]
    fn bdd_regular_gange_example() {
        let mut solver = TestSolver::default();
        let mdd = create_bdd_regular_gange(&mut solver);
        let layers = mdd.layers.clone();
        let srv_layers = mdd.srv_layers.clone();

        let mdd_propagator = solver
            .new_propagator(MddSRVPropagator::new(mdd))
            .expect("No Conflict");
        for x in layers.clone() {
            assert_eq!(solver.lower_bound(x), 0);
            assert_eq!(solver.upper_bound(x), 1);
        }
        let notification_status_1 =
            solver.decrease_upper_bound_and_notify(mdd_propagator, 2, layers[2], 0);
        assert!(match notification_status_1 {
            EnqueueDecision::Enqueue => true,
            EnqueueDecision::Skip => false,
        });
        let notification_status_2 =
            solver.decrease_upper_bound_and_notify(mdd_propagator, 3, layers[3], 0);
        assert!(match notification_status_2 {
            EnqueueDecision::Enqueue => true,
            EnqueueDecision::Skip => false,
        });
        let result = solver.propagate(mdd_propagator);
        assert!(result.is_ok());
        assert_eq!(solver.lower_bound(layers[0]), 1);
        assert_eq!(solver.upper_bound(layers[0]), 1);
        let reason_0 = solver.get_reason_int(predicate!(layers[0] != 0));
        assert_eq!(conjunction!([layers[2] != 1]), reason_0);
        assert_eq!(solver.lower_bound(layers[1]), 1);
        assert_eq!(solver.upper_bound(layers[1]), 1);
        let reason_1 = solver.get_reason_int(predicate!(layers[1] != 0));
        assert_eq!(conjunction!([layers[3] != 1]), reason_1); // TODO check this
        assert_eq!(solver.lower_bound(layers[5]), 1);
        assert_eq!(solver.upper_bound(layers[5]), 1);
        let reason_1 = solver.get_reason_int(predicate!(layers[5] != 0));
        assert_eq!(conjunction!([layers[3] != 1]), reason_1);
        let unchanged_vars = vec![layers[4], layers[6]];
        for x in unchanged_vars {
            assert_eq!(solver.lower_bound(x), 0);
            assert_eq!(solver.upper_bound(x), 1);
        }
        assert_eq!(solver.lower_bound(srv_layers[0]), 0);
        assert_eq!(solver.upper_bound(srv_layers[0]), 0);
        let reason_y1 = solver.get_reason_int(predicate!(srv_layers[0] != 1));
        assert_eq!(conjunction!([layers[2] != 1]), reason_y1);
        assert_eq!(solver.lower_bound(srv_layers[1]), 0);
        assert_eq!(solver.upper_bound(srv_layers[1]), 0);
        let reason_y2_1 = solver.get_reason_int(predicate!(srv_layers[1] != 1));
        assert_eq!(conjunction!([layers[2] != 1]), reason_y2_1);
        let reason_y2_2 = solver.get_reason_int(predicate!(srv_layers[1] != 2));
        assert_eq!(conjunction!([layers[3] != 1]), reason_y2_2); //TODO check this
        assert_eq!(solver.lower_bound(srv_layers[2]), 0);
        assert_eq!(solver.upper_bound(srv_layers[2]), 0);
        let reason_y3_1 = solver.get_reason_int(predicate!(srv_layers[2] != 1));
        assert_eq!(conjunction!([layers[2] != 1]), reason_y3_1);
        let reason_y3_2 = solver.get_reason_int(predicate!(srv_layers[2] != 2));
        assert_eq!(conjunction!([layers[3] != 1]), reason_y3_2);
        assert_eq!(solver.lower_bound(srv_layers[3]), 1);
        assert_eq!(solver.upper_bound(srv_layers[3]), 1);
        let reason_y4_0 = solver.get_reason_int(predicate!(srv_layers[3] != 0));
        assert_eq!(conjunction!([layers[3] != 1]), reason_y4_0);
        let reason_y4_1 = solver.get_reason_int(predicate!(srv_layers[3] != 2));
        assert_eq!(conjunction!([layers[3] != 1]), reason_y4_1);
        assert_eq!(solver.lower_bound(srv_layers[4]), 1);
        assert_eq!(solver.upper_bound(srv_layers[4]), 2);
        let reason_y5_0 = solver.get_reason_int(predicate!(srv_layers[4] != 0));
        assert_eq!(conjunction!([layers[3] != 1]), reason_y5_0);
        assert_eq!(solver.lower_bound(srv_layers[5]), 0);
        assert_eq!(solver.upper_bound(srv_layers[5]), 1);
    }

    #[test]
    /// Tests the removal of a value from a srv variable, simulating the case during a branch and bound
    fn bdd_gange_srv_domain_removal() {
        let mut solver = TestSolver::default();
        let mdd = create_bdd_regular_gange(&mut solver);
        let layers = mdd.layers.clone();
        let srv_layers = mdd.srv_layers.clone();

        let mdd_propagator = solver
            .new_propagator(MddSRVPropagator::new(mdd))
            .expect("No Conflict");
        let notification_status_1 = solver.increase_lower_bound_and_notify(
            mdd_propagator,
            (layers.len() + 2) as u32,
            srv_layers[2],
            1,
        );
        assert!(match notification_status_1 {
            EnqueueDecision::Enqueue => true,
            EnqueueDecision::Skip => false,
        });
        let result = solver.propagate(mdd_propagator);
        assert!(result.is_ok());

        assert_eq!(solver.lower_bound(layers[0]), 0);
        assert_eq!(solver.upper_bound(layers[0]), 0);
        let reason_x0_1 = solver.get_reason_int(predicate!(layers[0] != 1));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_x0_1);
        assert_eq!(solver.lower_bound(layers[2]), 1);
        assert_eq!(solver.upper_bound(layers[2]), 1);
        let reason_x2_0 = solver.get_reason_int(predicate!(layers[2] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_x2_0);
        assert_eq!(solver.lower_bound(layers[5]), 1);
        assert_eq!(solver.upper_bound(layers[5]), 1);
        let reason_x5_0 = solver.get_reason_int(predicate!(layers[5] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_x5_0);
        assert_eq!(solver.lower_bound(srv_layers[0]), 1);
        assert_eq!(solver.upper_bound(srv_layers[0]), 1);
        let reason_y1_0 = solver.get_reason_int(predicate!(srv_layers[0] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_y1_0);
        assert_eq!(solver.lower_bound(srv_layers[1]), 1);
        assert_eq!(solver.upper_bound(srv_layers[1]), 2);
        let reason_y2_0 = solver.get_reason_int(predicate!(srv_layers[1] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_y2_0);
        assert_eq!(solver.lower_bound(srv_layers[2]), 1);
        assert_eq!(solver.upper_bound(srv_layers[2]), 2);
        assert_eq!(solver.lower_bound(srv_layers[3]), 1);
        assert_eq!(solver.upper_bound(srv_layers[3]), 2);
        let reason_y4_0 = solver.get_reason_int(predicate!(srv_layers[3] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_y4_0);
        assert_eq!(solver.lower_bound(srv_layers[4]), 1);
        assert_eq!(solver.upper_bound(srv_layers[4]), 2);
        let reason_y5_0 = solver.get_reason_int(predicate!(srv_layers[4] != 0));
        assert_eq!(conjunction!([srv_layers[2] != 0]), reason_y5_0);

        //remaining unchanged
        assert_eq!(solver.lower_bound(layers[1]), 0);
        assert_eq!(solver.upper_bound(layers[1]), 1);
        assert_eq!(solver.lower_bound(layers[3]), 0);
        assert_eq!(solver.upper_bound(layers[3]), 1);
        assert_eq!(solver.lower_bound(layers[4]), 0);
        assert_eq!(solver.upper_bound(layers[4]), 1);
        assert_eq!(solver.lower_bound(layers[6]), 0);
        assert_eq!(solver.upper_bound(layers[6]), 1);
        assert_eq!(solver.lower_bound(srv_layers[5]), 0);
        assert_eq!(solver.upper_bound(srv_layers[5]), 1);
    }
}
