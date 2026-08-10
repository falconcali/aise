use super::*;
use crate::domain::asset::ids::LocationKey;
use crate::domain::turn::RetrievalSignalOrigin;

#[test]
fn baseline_signal_entity_is_known_without_knowledge_catalog_entry() {
    let entity = KnowledgeEntity::Location(LocationKey::from("lodge_hall"));
    let signals = vec![EntitySignal {
        entity: entity.clone(),
        origin: RetrievalSignalOrigin::Scene,
        priority: 1,
    }];

    assert!(entity_is_known(&entity, &[], &signals));
}

#[test]
fn arbitrary_location_is_not_known_without_catalog_or_signal() {
    let entity = KnowledgeEntity::Location(LocationKey::from("invented_location"));

    assert!(!entity_is_known(&entity, &[], &[]));
}
