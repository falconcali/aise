use aise::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{FactId, RoleId};
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
        max_role_audiences: 4,
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
            source_id: KnowledgeSourceId::Fact(FactId::try_new(id).unwrap()),
            knowledge_kind: KnowledgeKind::Fact,
            source: KnowledgeSource::Seed {
                pack_id: aise::domain::asset::ids::PackId::from("pack-1"),
                pack_digest: aise::domain::asset::ids::Sha256Digest::try_new(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            },
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
    let item = item("abcdefgh", "fact_0001");
    assert_eq!(item.token_cost, estimate_text_tokens("abcdefgh"));
}

#[test]
fn retrieved_context_rejects_audience_overflow() {
    let mut roles = BTreeMap::new();
    roles.insert(RoleId::try_new("c-1").unwrap(), vec![item("a", "fact_0001")]);
    roles.insert(RoleId::try_new("c-2").unwrap(), vec![item("b", "fact_0002")]);
    let tight = RetrievedContextLimits {
        max_role_audiences: 1,
        ..limits()
    };
    let err = RetrievedContext::try_new(vec![item("w", "fact_0003")], roles, tight);
    assert!(err.is_err());
}

#[test]
fn fact_retrieval_never_creates_character_context() {
    let ctx = RetrievedContext::try_new(vec![item("shared fact", "fact_0001")], BTreeMap::new(), limits()).unwrap();
    assert_eq!(ctx.writer().len(), 1);
    assert!(ctx.roles().is_empty());
    assert!(ctx.for_role(&RoleId::try_new("c-1").unwrap()).is_empty());
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
