use crate::engine::EmptyDomain;
use crate::predicates::PropositionalConjunction;

/// The inconsistency returned by a single-inference propagator.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SIInconsistency {
    EmptyDomain,
    Conflict(PropositionalConjunction),
}

impl From<EmptyDomain> for SIInconsistency {
    fn from(_: EmptyDomain) -> Self {
        SIInconsistency::EmptyDomain
    }
}

impl From<PropositionalConjunction> for SIInconsistency {
    fn from(conflict: PropositionalConjunction) -> Self {
        SIInconsistency::Conflict(conflict)
    }
}
