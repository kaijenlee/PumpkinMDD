use crate::HashMap;
use fnv::{FnvBuildHasher, FnvHashMap};
use log::warn;
use pumpkin_solver::constraints::Constraint;
use pumpkin_solver::constraints::{self};
use pumpkin_solver::options::DecisionDiagramOptions;
use pumpkin_solver::statistics::log_statistic;
use pumpkin_solver::variables::DomainId;

pub(crate) fn run(
    context: &mut super::context::CompilationContext<'_>,
    options: DecisionDiagramOptions,
) -> Result<bool, ()> {
    let constraint_groups = match options.intersection_strategy {
        pumpkin_solver::options::DecisionDiagramIntersection::None => context
            .dd_constraints
            .iter()
            .map(|c| vec![c.clone()])
            .collect::<Vec<_>>(),
        pumpkin_solver::options::DecisionDiagramIntersection::All => {
            vec![context.dd_constraints.clone()]
        }
    };
    let mut sat = true;
    let start = std::time::Instant::now();
    for (i, group) in constraint_groups.iter().enumerate() {
        warn!(
            "Processing MDD constraints group {} of {}",
            i + 1,
            constraint_groups.len()
        );
        match process_group(group.clone(), context, options) {
            Ok(mut mdd_graph) => {
                warn!(
                    "Compiled MDD with {} layers and {} transitions",
                    mdd_graph.layers.len(),
                    mdd_graph.transitions.len()
                );
                // warn!("{:?}", mdd_graph.transitions);
                let status = match options.srv_enable {
                    true => {
                        let layer_to_indices: HashMap<usize, usize> =
                            mdd_graph.transitions.iter().fold(
                                FnvHashMap::with_hasher(FnvBuildHasher::default()),
                                |mut acc, transition| {
                                    // Prevent constructing SRV for the root layer
                                    if !(transition.from.layer == 0 && transition.from.index == 0) {
                                        let _ = acc
                                            .entry(transition.from.layer)
                                            .and_modify(|v| *v = (*v).max(transition.from.index))
                                            .or_insert(transition.from.index);
                                    }
                                    // Prevent constructing SRV for the sink layer
                                    if transition.to != mdd_graph.sink {
                                        let _ = acc
                                            .entry(transition.to.layer)
                                            .and_modify(|v| *v = (*v).max(transition.to.index))
                                            .or_insert(transition.to.index);
                                    }
                                    acc
                                },
                            );
                        let mut srv_layers = mdd_graph.layers.clone();
                        let _ = srv_layers.pop(); // Remove one element as we dont need the source layer for SRV
                        assert_eq!(srv_layers.len(), layer_to_indices.len());

                        for (layer, index) in layer_to_indices.clone() {
                            if let Some(srv_layer) = srv_layers.get_mut(layer - 1) {
                                *srv_layer = context.solver.new_bounded_integer(0, index as i32);
                            }
                        }
                        mdd_graph.set_srv_layer(srv_layers);
                        warn!("MDD SRVs : {:?}", mdd_graph.srv_layers);
                        warn!("MDD Width: {}", layer_to_indices.values().max().unwrap() + 1);  
                        constraints::srv_mdd(mdd_graph).post(context.solver, None)
                    }
                    false => constraints::base_mdd(mdd_graph).post(context.solver, None),
                };
                warn!(
                    "MDD constraints group {group:?} solver post status: {:?}",
                    status
                );
                sat &= status.is_ok();
            }
            Err(_) => {
                warn!("Failed to compile an MDD from a constraint group {group:?}");
            }
        }
    }
    let elapsed = start.elapsed();
    log_statistic("mdd_compilation_time", elapsed.as_secs_f64());
    Ok(sat)
}

fn process_group(
    constraints: Vec<mdd_compile::constraints::Constraint<DomainId>>,
    context: &mut super::context::CompilationContext<'_>,
    options: DecisionDiagramOptions,
) -> Result<mdd_compile::mdd::MddGraph<DomainId>, mdd_compile::mdd::MddConstructionError> {
    // Preserve the order of variables as they appear in the constraints
    let mut seen = std::collections::HashSet::new();
    let var_ids: Vec<_> = constraints
        .iter()
        .flat_map(|cons| cons.variables())
        .filter(|&id| seen.insert(id))
        .collect();
    let mut mdd_builder = mdd_compile::mdd::MddBuilder::<DomainId>::new(options.max_width);
    for &var_id in var_ids {
        let lb = context.solver.lower_bound(&var_id);
        let ub = context.solver.upper_bound(&var_id);
        mdd_builder = mdd_builder.add_variable(var_id, lb, ub);
    }
    for constraint in constraints {
        mdd_builder = mdd_builder.add_constraint(constraint)?;
    }
    mdd_builder.build()
}
