use aise::config::{RetrievalConfig, TurnConfig, TurnContentLimitsConfig};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{CharacterId, FactId, StoryRevision};
use aise::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use aise::domain::text::estimate_text_tokens;
use aise::domain::turn::{
    ContextItem, ContextProvenance, MatchLevel, RelevanceRank, RetrievalAudience, RetrievedContext,
    RetrievedContextLimits,
};
use aise::turn::turn_budget::TurnBudget;
use std::collections::BTreeMap;

fn limits() -> RetrievedContextLimits {
    RetrievedContextLimits {
        max_character_audiences: 4,
        max_items_per_audience: 8,
        max_tokens_per_audience: 1_000,
        max_total_items: 16,
        max_total_tokens: 2_000,
        max_item_bytes: 4_096,
    }
}

fn item(text: &str, id: &str) -> ContextItem {
    let content = BoundedText::try_new(text, "item", 4_096).unwrap();
    ContextItem::from_parts(
        content,
        ContextProvenance {
            source_id: KnowledgeSourceId::Fact(FactId::from(id)),
            knowledge_kind: KnowledgeKind::Fact,
            source: KnowledgeSource::Seed {
                pack_id: aise::domain::asset::ids::PackId::from("pack-1"),
                pack_digest: aise::domain::asset::ids::Sha256Digest::try_new(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            },
            source_revision: StoryRevision::new(0),
            audience: RetrievalAudience::GlobalWriter,
            memory_owner: None,
            evidence: BTreeMap::new(),
        },
        RelevanceRank {
            match_level: MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
    )
}

#[test]
fn context_and_llm_accounting_share_one_token_estimator() {
    assert_eq!(estimate_text_tokens(""), 1);
    assert_eq!(estimate_text_tokens("abcd"), 1);
    assert_eq!(estimate_text_tokens("abcde"), 2);
    let item = item("abcdefgh", "f1");
    assert_eq!(item.token_cost, estimate_text_tokens("abcdefgh"));
}

#[test]
fn retrieved_context_rejects_audience_overflow() {
    let mut characters = BTreeMap::new();
    characters.insert(CharacterId::from("c-1"), vec![item("a", "f1")]);
    characters.insert(CharacterId::from("c-2"), vec![item("b", "f2")]);
    let tight = RetrievedContextLimits {
        max_character_audiences: 1,
        ..limits()
    };
    let err = RetrievedContext::try_new(vec![item("w", "f0")], characters, tight);
    assert!(err.is_err());
}

#[test]
fn fact_retrieval_never_creates_character_context() {
    let ctx = RetrievedContext::try_new(vec![item("shared fact", "f1")], BTreeMap::new(), limits()).unwrap();
    assert_eq!(ctx.writer().len(), 1);
    assert!(ctx.characters().is_empty());
    assert!(ctx.for_character(&CharacterId::from("c-1")).is_empty());
}

#[test]
fn turn_budget_from_config_accepts_retrieval_config() {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
    )
    .unwrap();
    assert!(budget.max_total_items() > 0 || budget.max_retrieved_tokens() > 0);
}
