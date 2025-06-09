use mdd_compile::mdd::{MddEdge, MddNode};

/// Enum to represent the status of an edge in the MDD
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum EdgeStatus {
    /// Edge is alive
    Alive,
    /// Edge is killed due to a domain change
    Dom,
    /// Edge is killed from above (due to downward pass)
    Above,
    /// Edge is killed from below (due to upward pass)
    Below,
}

/// Enum to represent the status of an edge in the MDD
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum NodeStatus {
    /// Node is alive
    Alive,
    /// Node is killed due to a domain change
    Dom,
    /// Node is killed from above (due to downward pass)
    Above,
    /// Node is killed from below (due to upward pass)
    Below,
}

/// Enum to represent the different types of components in the MDD
#[derive(Debug)]
pub(crate) enum MddComponentType {
    Edge(MddEdge),
    Node(MddNode),
}

/// Enum to represent the edge watch flags for the edge watching scheme proposed by \[1\].
///
/// \[1\] G. Gange, P. J. Stuckey, and R. Szymanek, “Mdd propagators with explanation,” Constraints, vol. 16, pp. 407–429, 4 Oct. 2011, issn: 13837133. Doi: 10.1007/s10601-011-9111-x
#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) enum EdgeWatchFlag {
    /// Edge is being watched by a value, i.e. (var, val)
    Value,
    /// Edge is being watched as an outgoing edge for a node
    Begin,
    /// Edge is being watched as an incoming edge for a node
    End,
}