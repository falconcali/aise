use super::*;
use crate::domain::ids::{MemoryId, RumorId};
use crate::domain::turn::DeletableKnowledgeId;

#[test]
fn target_key_distinguishes_rumor_and_memory() {
    let rumor = DeletableKnowledgeId::Rumor(RumorId::try_new("rumor_0001").unwrap());
    let memory = DeletableKnowledgeId::Memory(MemoryId::try_new("memory_0001").unwrap());
    assert_ne!(target_key(&rumor), target_key(&memory));
    assert_eq!(target_key(&rumor), "rumor:rumor_0001");
    assert_eq!(target_key(&memory), "memory:memory_0001");
}

#[test]
fn extraction_schema_validator_is_default_constructible() {
    let _validator = ExtractionSchemaValidator;
}
