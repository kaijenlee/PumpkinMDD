use super::SIInconsistency;
use super::SIPropagationContextMut;
use crate::engine::opaque_domain_event::OpaqueDomainEvent;
use crate::engine::propagation::contexts::PropagationContextWithTrailedValues;
use crate::engine::propagation::EnqueueDecision;
use crate::engine::propagation::ExplanationContext;
use crate::engine::propagation::LocalId;
use crate::engine::propagation::PropagationContext;
use crate::predicates::Predicate;
use crate::predicates::PropositionalConjunction;
use crate::statistics::StatisticLogger;

/// A propagator that will propagate with a single inference code. This interface is the same as the
/// [`Propagator`] interface, with the exception that an [`SCPropagator`] never has to supply
/// inference codes.
///
/// For a motivation, see the documentation at [`pumpkin_solver::propagators::single_inference`].
///
/// For info on the functions, see [`pumpkin_solver::engine::propagation::Propagator`].
pub(crate) trait SIPropagator {
    fn name(&self) -> &str;

    fn debug_propagate_from_scratch(
        &self,
        context: SIPropagationContextMut,
    ) -> Result<(), SIInconsistency>;

    fn propagate(&mut self, context: SIPropagationContextMut) -> Result<(), SIInconsistency> {
        self.debug_propagate_from_scratch(context)
    }

    fn notify(
        &mut self,
        _context: PropagationContextWithTrailedValues,
        _local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        EnqueueDecision::Enqueue
    }

    fn notify_backtrack(
        &mut self,
        _context: PropagationContext,
        _local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) {
    }

    fn synchronise(&mut self, _context: PropagationContext) {}

    fn priority(&self) -> u32 {
        3
    }

    fn detect_inconsistency(
        &self,
        _context: PropagationContextWithTrailedValues,
    ) -> Option<PropositionalConjunction> {
        None
    }

    fn lazy_explanation(&mut self, _code: u64, _context: ExplanationContext) -> &[Predicate] {
        panic!(
            "{}",
            format!(
                "Propagator {} does not support lazy explanations.",
                self.name()
            )
        );
    }

    fn log_statistics(&self, _statistic_logger: StatisticLogger) {}
}
