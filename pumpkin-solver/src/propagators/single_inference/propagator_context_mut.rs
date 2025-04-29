use crate::engine::propagation::contexts::PropagationContextWithTrailedValues;
use crate::engine::propagation::PropagationContext;
use crate::engine::propagation::PropagationContextMut;
use crate::engine::reason::Reason;
use crate::engine::EmptyDomain;
use crate::predicates::Predicate;
use crate::proof::InferenceCode;

pub(crate) struct SIPropagationContextMut<'a> {
    inference_code: InferenceCode,
    context: PropagationContextMut<'a>,
}

impl<'a> SIPropagationContextMut<'a> {
    pub(crate) fn new(inference_code: InferenceCode, context: PropagationContextMut<'a>) -> Self {
        SIPropagationContextMut {
            inference_code,
            context,
        }
    }

    /// See [`PropagationContextMut::as_trailed_readonly`].
    pub(crate) fn as_trailed_readonly(&mut self) -> PropagationContextWithTrailedValues {
        self.context.as_trailed_readonly()
    }

    /// See [`PropagationContextMut::as_readonly`].
    pub(crate) fn as_readonly(&self) -> PropagationContext<'_> {
        self.context.as_readonly()
    }

    /// See [`PropagationContextMut::post`].
    pub(crate) fn post(
        &mut self,
        predicate: Predicate,
        reason: impl Into<Reason>,
    ) -> Result<(), EmptyDomain> {
        self.context.post(predicate, reason)
    }

    /// See [`PropagationContextMut::with_reification`].
    pub(crate) fn with_reification(&mut self, reification_literal: crate::variables::Literal) {
        self.context.with_reification(reification_literal);
    }

    pub(crate) fn reborrow<'b>(&'b mut self) -> SIPropagationContextMut<'b> {
        SIPropagationContextMut {
            inference_code: self.inference_code,
            context: self.context.reborrow(),
        }
    }
}

mod private {
    use super::*;
    use crate::engine::propagation::contexts::HasAssignments;
    use crate::engine::propagation::contexts::HasTrailedValues;
    use crate::engine::Assignments;
    use crate::engine::TrailedValues;

    impl HasTrailedValues for SIPropagationContextMut<'_> {
        fn trailed_values(&self) -> &TrailedValues {
            self.context.trailed_values()
        }

        fn trailed_values_mut(&mut self) -> &mut TrailedValues {
            self.context.trailed_values_mut()
        }
    }

    impl HasAssignments for SIPropagationContextMut<'_> {
        fn assignments(&self) -> &Assignments {
            self.context.assignments()
        }
    }
}
