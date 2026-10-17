use super::*;
use crate::core::{ActivationRule, BoundedText, RetrievalHint};
use serde_json::json;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid bounded text")
}

fn fact_body(content: &str) -> FactBody {
    FactBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new("fact hint").expect("valid hint"),
    }
}

fn rumor_body(content: &str) -> RumorBody {
    RumorBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new("rumor hint").expect("valid hint"),
    }
}

fn memory_body(content: &str) -> MemoryBody {
    MemoryBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new("memory hint").expect("valid hint"),
    }
}

#[test]
fn default_knowledge_change_is_empty() {
    let change = KnowledgeChange::default();

    assert!(change.is_empty());
}

#[test]
fn knowledge_change_with_any_op_is_not_empty() {
    let fact_change = KnowledgeChange {
        facts: vec![FactChangeOp::Add {
            id: FactId::try_new("fact-1").expect("valid id"),
            body: fact_body("fact"),
        }],
        ..KnowledgeChange::default()
    };
    let rumor_change = KnowledgeChange {
        rumors: vec![RumorChangeOp::Remove {
            id: RumorId::try_new("rumor-1").expect("valid id"),
        }],
        ..KnowledgeChange::default()
    };
    let memory_change = KnowledgeChange {
        memories: vec![RoleMemoryChangeOp::Update {
            role_id: RoleId::try_new("role-1").expect("valid role id"),
            id: MemoryId::try_new("mem-1").expect("valid memory id"),
            body: memory_body("memory"),
        }],
        ..KnowledgeChange::default()
    };

    assert!(!fact_change.is_empty());
    assert!(!rumor_change.is_empty());
    assert!(!memory_change.is_empty());
}

#[test]
fn knowledge_change_round_trips_through_json() {
    let change = KnowledgeChange {
        facts: vec![
            FactChangeOp::Add {
                id: FactId::try_new("fact-1").expect("valid id"),
                body: fact_body("fact one"),
            },
            FactChangeOp::Update {
                id: FactId::try_new("fact-2").expect("valid id"),
                body: fact_body("fact two"),
            },
        ],
        rumors: vec![
            RumorChangeOp::Add {
                id: RumorId::try_new("rumor-1").expect("valid id"),
                body: rumor_body("rumor one"),
            },
            RumorChangeOp::Update {
                id: RumorId::try_new("rumor-2").expect("valid id"),
                body: rumor_body("rumor two"),
            },
            RumorChangeOp::Remove {
                id: RumorId::try_new("rumor-3").expect("valid id"),
            },
        ],
        memories: vec![
            RoleMemoryChangeOp::Add {
                role_id: RoleId::try_new("role-1").expect("valid role id"),
                id: MemoryId::try_new("mem-1").expect("valid memory id"),
                body: memory_body("memory one"),
            },
            RoleMemoryChangeOp::Update {
                role_id: RoleId::try_new("role-1").expect("valid role id"),
                id: MemoryId::try_new("mem-2").expect("valid memory id"),
                body: memory_body("memory two"),
            },
        ],
    };

    let value = serde_json::to_value(&change).expect("serializes");
    let decoded: KnowledgeChange = serde_json::from_value(value.clone()).expect("deserializes");
    let reencoded = serde_json::to_value(&decoded).expect("re-serializes");

    assert_eq!(reencoded, value);
}

#[test]
fn knowledge_change_rejects_unknown_fields() {
    let invalid = json!({
        "facts": [],
        "unknown": true
    });

    assert!(serde_json::from_value::<KnowledgeChange>(invalid).is_err());
}
