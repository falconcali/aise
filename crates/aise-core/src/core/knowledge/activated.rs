use crate::core::knowledge::{FactEntry, RumorEntry};
use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct ActivatedKnowledge {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
}
