use crate::core::BoundedText;
use crate::core::activation::ActivationRule;
use crate::core::knowledge::RetrievalHint;

pub trait Recallable {
    fn content(&self) -> &BoundedText;
    fn activation(&self) -> &ActivationRule;
    fn retrieval(&self) -> &RetrievalHint;
}
