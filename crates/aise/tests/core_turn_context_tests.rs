use aise::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{FactId, RoleId, RumorId};
use aise::domain::knowledge::{KnowledgeSource, KnowledgeSourceId};
use aise::domain::text::estimate_text_tokens;
use aise::domain::turn::{
    MatchLevel, RelevanceRank, RetrievedCharacterContext, RetrievedContext, RetrievedContextLimits,
    RetrievedKnowledgeItem, RetrievedWorldKnowledge,
};
use aise::turn::turn_budget::TurnBudget;
use std::collections::BTreeMap;

fn limits() -> RetrievedContextLimits {
    RetrievedContextLimits {
        max_role_audiences: 4,
        max_items_per_audience: 8,
        max_tokens_per_audience: 1_000,
        max_total_items: 16,
        max_total_tokens: 2_000,
        max_item_bytes: 4_096,
    }
}

fn fact_item(text: &str, id: &str) -> RetrievedKnowledgeItem {
    let content = BoundedText::try_new(text, "item", 4_096).unwrap();
    RetrievedKnowledgeItem::from_parts(
        KnowledgeSourceId::Fact(FactId::try_new(id).unwrap()),
        content,
        KnowledgeSource::Seed {
            pack_id: aise::domain::asset::ids::PackId::from("pack-1"),
            pack_digest: aise::domain::asset::ids::Sha256Digest::try_new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
        },
        RelevanceRank {
            match_level: MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
        BTreeMap::new(),
    )
}

fn rumor_item(text: &str, id: &str) -> RetrievedKnowledgeItem {
    let content = BoundedText::try_new(text, "item", 4_096).unwrap();
    RetrievedKnowledgeItem::from_parts(
        KnowledgeSourceId::Rumor(RumorId::try_new(id).unwrap()),
        content,
        KnowledgeSource::Seed {
            pack_id: aise::domain::asset::ids::PackId::from("pack-1"),
            pack_digest: aise::domain::asset::ids::Sha256Digest::try_new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
        },
        RelevanceRank {
            match_level: MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
        BTreeMap::new(),
    )
}

#[test]
fn context_and_llm_accounting_share_one_token_estimator() {
    assert_eq!(estimate_text_tokens(""), 1);
    assert_eq!(estimate_text_tokens("abcd"), 1);
    assert_eq!(estimate_text_tokens("abcde"), 2);
    let item = fact_item("abcdefgh", "fact_0001");
    assert_eq!(item.token_cost, estimate_text_tokens("abcdefgh"));
}

#[test]
fn retrieved_context_rejects_audience_overflow() {
    let mut characters = BTreeMap::new();
    characters.insert(
        RoleId::try_new("c-1").unwrap(),
        RetrievedCharacterContext {
            role: None,
            known_rumors: vec![rumor_item("a", "rumor_0001")],
            memories: Vec::new(),
        },
    );
    characters.insert(
        RoleId::try_new("c-2").unwrap(),
        RetrievedCharacterContext {
            role: None,
            known_rumors: vec![rumor_item("b", "rumor_0002")],
            memories: Vec::new(),
        },
    );
    let tight = RetrievedContextLimits {
        max_role_audiences: 1,
        ..limits()
    };
    let world = RetrievedWorldKnowledge {
        facts: vec![fact_item("w", "fact_0003")],
        rumors: Vec::new(),
    };
    let err = RetrievedContext::try_new(world, characters, tight);
    assert!(err.is_err());
}

#[test]
fn fact_retrieval_never_creates_character_context() {
    let world = RetrievedWorldKnowledge {
        facts: vec![fact_item("shared fact", "fact_0001")],
        rumors: Vec::new(),
    };
    let ctx = RetrievedContext::try_new(world, BTreeMap::new(), limits()).unwrap();
    assert_eq!(ctx.world().facts.len(), 1);
    assert!(ctx.characters().is_empty());
    assert!(ctx.character(&RoleId::try_new("c-1").unwrap()).is_none());
}

#[test]
fn turn_budget_from_config_accepts_retrieval_config() {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    assert!(budget.max_total_items() > 0 || budget.max_retrieved_tokens() > 0);
}
