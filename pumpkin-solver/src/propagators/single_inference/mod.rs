mod constructor;
mod inconsistency;
mod propagator;
mod propagator_context_mut;

pub(crate) use constructor::*;
pub(crate) use inconsistency::*;
pub(crate) use propagator::*;
pub(crate) use propagator_context_mut::*;

use crate::basic_types::Inconsistency;
use crate::basic_types::PropagationStatusCP;
use crate::basic_types::PropagatorConflict;
use crate::engine::opaque_domain_event::OpaqueDomainEvent;
use crate::engine::propagation::constructor::PropagatorConstructor;
use crate::engine::propagation::constructor::PropagatorConstructorContext;
use crate::engine::propagation::contexts::PropagationContextWithTrailedValues;
use crate::engine::propagation::EnqueueDecision;
use crate::engine::propagation::ExplanationContext;
use crate::engine::propagation::LocalId;
use crate::engine::propagation::PropagationContext;
use crate::engine::propagation::PropagationContextMut;
use crate::engine::propagation::Propagator;
use crate::predicates::Predicate;
use crate::predicates::PropositionalConjunction;
use crate::proof::ConstraintTag;
use crate::proof::InferenceCode;
use crate::statistics::StatisticLogger;

/// The [`PropagatorConstructor`] for the [`SingleInferencePropagator`].
pub struct SingleInferencePropagatorArgs<Wrapped> {
    pub wrapped_args: Wrapped,
    pub constraint_tag: ConstraintTag,
}

impl<Wrapped> PropagatorConstructor for SingleInferencePropagatorArgs<Wrapped>
where
    Wrapped: SIPropagatorConstructor,
    Wrapped::PropagatorImpl: 'static,
{
    type PropagatorImpl = SingleInferencePropagator<Wrapped::PropagatorImpl>;

    fn create(self, mut context: PropagatorConstructorContext) -> Self::PropagatorImpl {
        let (wrapped, inference_label) = {
            let context =
                SIPropagatorConstructorContext::new(context.reborrow(), self.constraint_tag);
            self.wrapped_args.create(context)
        };

        let inference_code = context.create_inference_code(self.constraint_tag, inference_label);

        SingleInferencePropagator {
            wrapped,
            inference_code,
        }
    }
}

/// A wrapper around a [`SIPropagator`] that implements [`Propagator`]. It associates every
/// propagation with an inference code, meaning the wrapped propagator does not need to worry about
/// that.
pub struct SingleInferencePropagator<Wrapped> {
    wrapped: Wrapped,
    inference_code: InferenceCode,
}

impl<Wrapped: SIPropagator + 'static> Propagator for SingleInferencePropagator<Wrapped> {
    fn name(&self) -> &str {
        self.wrapped.name()
    }

    fn debug_propagate_from_scratch(&self, context: PropagationContextMut) -> PropagationStatusCP {
        self.wrapped
            .debug_propagate_from_scratch(SIPropagationContextMut::new(
                self.inference_code,
                context,
            ))
            .map_err(|conflict| match conflict {
                SIInconsistency::Conflict(conjunction) => {
                    Inconsistency::Conflict(PropagatorConflict {
                        conjunction,
                        inference_code: self.inference_code,
                    })
                }
                SIInconsistency::EmptyDomain => Inconsistency::EmptyDomain,
            })
    }

    fn propagate(&mut self, context: PropagationContextMut) -> PropagationStatusCP {
        self.wrapped
            .propagate(SIPropagationContextMut::new(self.inference_code, context))
            .map_err(|conflict| match conflict {
                SIInconsistency::Conflict(conjunction) => {
                    Inconsistency::Conflict(PropagatorConflict {
                        conjunction,
                        inference_code: self.inference_code,
                    })
                }
                SIInconsistency::EmptyDomain => Inconsistency::EmptyDomain,
            })
    }

    fn notify(
        &mut self,
        context: PropagationContextWithTrailedValues,
        local_id: LocalId,
        event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        self.wrapped.notify(context, local_id, event)
    }

    fn notify_backtrack(
        &mut self,
        context: PropagationContext,
        local_id: LocalId,
        event: OpaqueDomainEvent,
    ) {
        self.wrapped.notify_backtrack(context, local_id, event);
    }

    fn synchronise(&mut self, context: PropagationContext) {
        self.wrapped.synchronise(context);
    }

    fn priority(&self) -> u32 {
        self.wrapped.priority()
    }

    fn detect_inconsistency(
        &self,
        context: PropagationContextWithTrailedValues,
    ) -> Option<PropositionalConjunction> {
        self.wrapped.detect_inconsistency(context)
    }

    fn lazy_explanation(&mut self, code: u64, context: ExplanationContext) -> &[Predicate] {
        self.wrapped.lazy_explanation(code, context)
    }

    fn log_statistics(&self, statistic_logger: StatisticLogger) {
        self.wrapped.log_statistics(statistic_logger);
    }
}
