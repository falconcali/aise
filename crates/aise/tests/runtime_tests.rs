use aise::config::{RetrievalConfig, TurnConfig, TurnContentLimitsConfig};
use aise::core::turn_budget::TurnBudget;
use aise::core::turn_data::{
    CharacterThinkRequest, RetrievalAudience, RetrievalPlan, RetrievalRequest, RetrievalRequestOrigin, WriterPlan,
    WriterStoryGoal,
};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::CharacterId;
use aise::domain::knowledge::KnowledgeKind;
use aise::domain::narrative_graph::director::NarrativePlan;

fn sample_plan(with_requests: bool) -> WriterPlan {
    let mut plan = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: BoundedText::try_new("goal", "goal", 256).unwrap(),
        },
        narrative_plan: NarrativePlan::empty(),
        retrieval_plan: RetrievalPlan::default(),
        character_think_requests: Vec::new(),
    };
    if with_requests {
        plan.retrieval_plan.requests.push(RetrievalRequest {
            audience: RetrievalAudience::GlobalWriter,
            knowledge_kinds: vec![KnowledgeKind::Fact],
            entities: Vec::new(),
            topics: Vec::new(),
            query_text: None,
            authorized_memory_owners: Vec::new(),
            reason: BoundedText::try_new("need", "reason", 64).unwrap(),
            origin: RetrievalRequestOrigin::Automatic,
            signal_priority: 0,
        });
        plan.character_think_requests.push(CharacterThinkRequest {
            character_id: CharacterId::from("c-1"),
            reason: BoundedText::try_new("present", "reason", 64).unwrap(),
        });
    }
    plan
}

#[test]
fn retrieval_and_character_think_are_enabled_from_plan_collections() {
    let empty = sample_plan(false);
    assert!(empty.retrieval_plan.requests.is_empty());
    assert!(empty.character_think_requests.is_empty());
    let filled = sample_plan(true);
    assert_eq!(filled.retrieval_plan.requests.len(), 1);
    assert_eq!(filled.character_think_requests.len(), 1);
}

#[test]
fn turn_budget_from_config_uses_retrieval_totals() {
    let turn = TurnConfig {
        max_llm_calls: 3,
        ..TurnConfig::default()
    };
    let retrieval = RetrievalConfig {
        max_total_tokens: 2_000,
        max_tokens_per_audience: 500,
        ..RetrievalConfig::default()
    };
    let budget = TurnBudget::from_config(&turn, &TurnContentLimitsConfig::default(), &retrieval).unwrap();
    assert_eq!(budget.max_retrieved_tokens(), 2_000);
    assert_eq!(budget.max_llm_calls(), 3);
}
