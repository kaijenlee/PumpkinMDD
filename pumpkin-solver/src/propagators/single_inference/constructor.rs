use super::SIPropagator;
use crate::engine::propagation::constructor::PropagatorConstructorContext;
use crate::engine::propagation::LocalId;
use crate::engine::propagation::PropagationContext;
use crate::engine::Assignments;
use crate::engine::DomainEvents;
use crate::engine::TrailedValues;
use crate::proof::ConstraintTag;
use crate::proof::InferenceCode;
use crate::proof::InferenceLabel;
use crate::variables::IntegerVariable;

/// The single-inference counterpart to the [`PropagatorConstructor`].
pub(crate) trait SIPropagatorConstructor {
    /// The propagator that is produced by this constructor.
    type PropagatorImpl: SIPropagator;

    /// The inference label used by the propagator.
    type InferenceLabelImpl: InferenceLabel;

    /// Create the propagator instance from `Self`.
    fn create(
        self,
        context: SIPropagatorConstructorContext,
    ) -> (Self::PropagatorImpl, Self::InferenceLabelImpl);
}

#[derive(Debug)]
pub(crate) struct SIPropagatorConstructorContext<'a> {
    context: PropagatorConstructorContext<'a>,
    constraint_tag: ConstraintTag,
}

impl SIPropagatorConstructorContext<'_> {
    pub(crate) fn new<'a>(
        context: PropagatorConstructorContext<'a>,
        constraint_tag: ConstraintTag,
    ) -> SIPropagatorConstructorContext<'a> {
        SIPropagatorConstructorContext {
            context,
            constraint_tag,
        }
    }

    pub(crate) fn reborrow<'a>(&'a mut self) -> SIPropagatorConstructorContext<'a> {
        SIPropagatorConstructorContext {
            context: self.context.reborrow(),
            constraint_tag: self.constraint_tag,
        }
    }

    pub(crate) fn as_readonly(&self) -> PropagationContext {
        self.context.as_readonly()
    }

    pub(crate) fn register(
        &mut self,
        var: impl IntegerVariable,
        domain_events: DomainEvents,
        local_id: LocalId,
    ) {
        self.context.register(var, domain_events, local_id);
    }

    pub(crate) fn register_for_backtrack_events<Var: IntegerVariable>(
        &mut self,
        var: Var,
        domain_events: DomainEvents,
        local_id: LocalId,
    ) {
        self.context.register(var, domain_events, local_id);
    }

    pub(crate) fn get_next_local_id(&self) -> LocalId {
        self.context.get_next_local_id()
    }

    /// Create a new inference code to use whenever the propagator makes a propagation.
    pub(crate) fn create_inference_code(
        &mut self,
        inference_label: impl InferenceLabel,
    ) -> InferenceCode {
        self.context
            .create_inference_code(self.constraint_tag, inference_label)
    }
}

mod private {
    use super::*;
    use crate::engine::propagation::contexts::HasAssignments;
    use crate::engine::propagation::contexts::HasTrailedValues;

    impl HasAssignments for SIPropagatorConstructorContext<'_> {
        fn assignments(&self) -> &Assignments {
            self.context.assignments()
        }
    }

    impl HasTrailedValues for SIPropagatorConstructorContext<'_> {
        fn trailed_values(&self) -> &TrailedValues {
            self.context.trailed_values()
        }

        fn trailed_values_mut(&mut self) -> &mut TrailedValues {
            self.context.trailed_values_mut()
        }
    }
}
