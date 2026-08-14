use super::*;
use crate::domain::ids::{MemoryId, RumorId};
use crate::domain::turn::DeletableKnowledgeId;

#[test]
fn target_key_distinguishes_rumor_and_memory() {
    let rumor = DeletableKnowledgeId::Rumor(RumorId::from("r1".to_owned()));
    let memory = DeletableKnowledgeId::Memory(MemoryId::from("m1".to_owned()));
    assert_ne!(target_key(&rumor), target_key(&memory));
    assert_eq!(target_key(&rumor), "rumor:r1");
    assert_eq!(target_key(&memory), "memory:m1");
}

#[test]
fn extraction_schema_validator_is_default_constructible() {
    let _validator = ExtractionSchemaValidator;
}
