use super::*;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{FactId, RumorId};
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::turn::{
    RelevantWorldKnowledge, RelevantWorldKnowledgeItem, RetrievedKnowledgeItem, RetrievedWorldKnowledge,
};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value.to_owned(), "content", 256).unwrap()
}

fn fact_id(seq: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(seq).unwrap())
}

fn rumor_id(seq: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Rumor(RumorId::try_new(seq).unwrap())
}

fn retrieved_item(source_id: KnowledgeSourceId, content: &str) -> RetrievedKnowledgeItem {
    RetrievedKnowledgeItem::from_parts(
        source_id.clone(),
        text(content),
        crate::domain::knowledge::KnowledgeSource::Seed {
            pack_id: crate::domain::asset::ids::PackId::try_new("pack-1").unwrap(),
            pack_digest: crate::domain::asset::ids::Sha256Digest::try_new(&format!("sha256:{}", "0".repeat(64)))
                .unwrap(),
        },
        crate::domain::turn::RelevanceRank {
            match_level: crate::domain::turn::MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
        std::collections::BTreeMap::new(),
    )
}

#[test]
fn merge_world_knowledge_preserves_baseline_then_retrieved_order() {
    let baseline = RelevantWorldKnowledge {
        facts: vec![RelevantWorldKnowledgeItem {
            source_id: fact_id("fact_0001"),
            content: text("baseline fact"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let retrieved = RetrievedWorldKnowledge {
        facts: vec![retrieved_item(fact_id("fact_0002"), "retrieved fact")],
        rumors: vec![retrieved_item(rumor_id("rumor_0001"), "retrieved rumor")],
    };
    let view = merge_world_knowledge(&baseline, &retrieved).unwrap();
    assert_eq!(view.facts.len(), 2);
    assert_eq!(view.facts[0].as_str(), "baseline fact");
    assert_eq!(view.facts[1].as_str(), "retrieved fact");
    assert_eq!(view.rumors.len(), 1);
    assert_eq!(view.rumors[0].as_str(), "retrieved rumor");
}

#[test]
fn merge_world_knowledge_dedupes_by_id_with_baseline_precedence() {
    let baseline = RelevantWorldKnowledge {
        facts: vec![RelevantWorldKnowledgeItem {
            source_id: fact_id("fact_0001"),
            content: text("shared fact"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let retrieved = RetrievedWorldKnowledge {
        facts: vec![retrieved_item(fact_id("fact_0001"), "shared fact")],
        rumors: Vec::new(),
    };
    let view = merge_world_knowledge(&baseline, &retrieved).unwrap();
    assert_eq!(view.facts.len(), 1);
}

#[test]
fn merge_world_knowledge_rejects_same_id_conflicting_content() {
    let baseline = RelevantWorldKnowledge {
        facts: vec![RelevantWorldKnowledgeItem {
            source_id: fact_id("fact_0001"),
            content: text("version a"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let retrieved = RetrievedWorldKnowledge {
        facts: vec![retrieved_item(fact_id("fact_0001"), "version b")],
        rumors: Vec::new(),
    };
    let error = merge_world_knowledge(&baseline, &retrieved).unwrap_err();
    assert!(matches!(error, PromptProjectionError::IdConflict { .. }));
}

#[test]
fn merge_world_knowledge_does_not_merge_distinct_ids_with_same_text() {
    let baseline = RelevantWorldKnowledge {
        facts: vec![RelevantWorldKnowledgeItem {
            source_id: fact_id("fact_0001"),
            content: text("identical text"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let retrieved = RetrievedWorldKnowledge {
        facts: vec![retrieved_item(fact_id("fact_0002"), "identical text")],
        rumors: Vec::new(),
    };
    let view = merge_world_knowledge(&baseline, &retrieved).unwrap();
    assert_eq!(view.facts.len(), 2);
}
