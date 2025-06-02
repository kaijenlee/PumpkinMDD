use mdd_compile::mdd::MddGraph;
use crate::propagators::mdd::{MddBasePropagator, MddSRVPropagator};
use super::Constraint;
use crate::variables::IntegerVariable;

pub fn base_mdd<Var: std::fmt::Debug + IntegerVariable + std::hash::Hash + Eq + 'static>(
    mdd_graph: MddGraph<Var>,
) -> impl Constraint {
    MddBasePropagator::new(mdd_graph)
}

pub fn srv_mdd<Var: std::fmt::Debug + IntegerVariable + std::hash::Hash + Eq + 'static>(
    mdd_graph: MddGraph<Var>,
) -> impl Constraint {
    MddSRVPropagator::new(mdd_graph)
}
