use super::*;
use crate::config::{TurnConfig, TurnContentLimitsConfig};
use crate::core::turn_budget::TurnBudget;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{
    IdempotencyKey, StoryRevision, TurnCancellation, TurnControl, TurnIdentity, TurnRequest,
};
use crate::core::turn_data::{BaselineContext, ContextRequest, ContextSource, StoryGoal, WriterPlan};
use crate::core::turn_trace::TraceRecorder;
use crate::domain::character::{CharacterState, InternalState};
use crate::domain::ids::{CharacterId, MemoryId, StoryId, TurnId};
use crate::domain::memory::{MemoryEntry, MemoryKind};
use crate::domain::story_state::{AuthoritativeStoryState, PlayerStoryState, StoryReadSnapshot};
use crate::domain::world::{FactSource, WorldFact, WorldState};
use std::time::{Duration, Instant};

fn story_id() -> StoryId {
    StoryId::try_new("story-1").expect("story id")
}

fn snapshot() -> StoryReadSnapshot {
    let world = WorldState {
        id: story_id(),
        name: "world".into(),
        facts: vec![WorldFact {
            id: crate::domain::ids::FactId::from("f-1"),
            text: "the gate is guarded".into(),
            source: FactSource::Seed,
        }],
    };
    StoryReadSnapshot::new(
        story_id(),
        StoryRevision::new(0),
        AuthoritativeStoryState::default(),
        PlayerStoryState {
            player_character_id: None,
            player_memories: vec![MemoryEntry {
                id: crate::domain::ids::MemoryId::from("m-1"),
                owner: CharacterId::from("c-1"),
                kind: MemoryKind::Observed,
                content: "Mira saw the gate".into(),
                created_at: 0,
            }],
        },
        Some(world),
        vec![CharacterState {
            id: CharacterId::from("c-1"),
            name: "Mira".into(),
            bio: "bio".into(),
            internal_state: InternalState::default(),
        }],
        Vec::new(),
    )
}

#[test]
fn collect_source_reads_historical_story_from_baseline() {
    let baseline = BaselineContext {
        recent_story: vec!["first scene".into(), "second scene".into()],
        ..BaselineContext::default()
    };
    let items = collect_source(&snapshot(), &baseline, ContextSource::HistoricalStory);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].content, "first scene");
}

#[test]
fn collect_source_reads_world_facts_and_player_memories() {
    let baseline = BaselineContext::default();
    let world_items = collect_source(&snapshot(), &baseline, ContextSource::WorldKnowledge);
    assert_eq!(world_items.len(), 1);
    assert_eq!(world_items[0].content, "the gate is guarded");
    let memory_items = collect_source(&snapshot(), &baseline, ContextSource::CharacterMemory);
    assert_eq!(memory_items.len(), 1);
    assert_eq!(memory_items[0].content, "Mira saw the gate");
}

#[test]
fn keyword_score_ranks_term_coverage() {
    assert_eq!(keyword_score("gate", "the gate is guarded"), 1.0);
    assert_eq!(keyword_score("gate key", "the gate is guarded"), 0.5);
    assert_eq!(keyword_score("dragon", "the gate is guarded"), 0.0);
    assert_eq!(keyword_score("", "anything"), 1.0);
}

fn overflow_snapshot() -> StoryReadSnapshot {
    let mut facts: Vec<WorldFact> = (0..64)
        .map(|index| WorldFact {
            id: crate::domain::ids::FactId::from(format!("f-{index}")),
            text: format!("unrelated fact number {index}"),
            source: FactSource::Seed,
        })
        .collect();
    facts.push(WorldFact {
        id: crate::domain::ids::FactId::from("f-guarded"),
        text: "the gate is guarded".into(),
        source: FactSource::Seed,
    });
    let world = WorldState {
        id: story_id(),
        name: "world".into(),
        facts,
    };
    StoryReadSnapshot::new(
        story_id(),
        StoryRevision::new(0),
        AuthoritativeStoryState::default(),
        PlayerStoryState {
            player_character_id: Some(CharacterId::from("c-1")),
            player_memories: (0..16)
                .map(|index| MemoryEntry {
                    id: MemoryId::from(format!("m-{index}")),
                    owner: CharacterId::from("c-1"),
                    kind: MemoryKind::Observed,
                    content: format!("memory {index}"),
                    created_at: 0,
                })
                .collect(),
        },
        Some(world),
        vec![CharacterState {
            id: CharacterId::from("c-1"),
            name: "Mira".into(),
            bio: "bio".into(),
            internal_state: InternalState::default(),
        }],
        Vec::new(),
    )
}

#[tokio::test]
async fn retrieval_uses_bounded_top_k_candidates() {
    let turn = TurnConfig {
        max_retrieved_items: 4,
        max_retrieval_candidates: 8,
        ..TurnConfig::default()
    };
    let content = TurnContentLimitsConfig::default();
    let budget = TurnBudget::from_config(&turn, &content).expect("budget");
    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            story_id(),
            TurnId::try_new("turn-topk").unwrap(),
            IdempotencyKey::try_new("key-topk".to_string()).unwrap(),
            1000,
        )
        .unwrap(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        budget,
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    ctx.complete_initialization().unwrap();
    let baseline = BaselineContext {
        recent_story: (0..32).map(|index| format!("story scene {index}")).collect(),
        ..BaselineContext::default()
    };
    ctx.set_prepared_context(overflow_snapshot(), baseline).unwrap();
    let plan = WriterPlan {
        retrieval_requests: vec![ContextRequest {
            query: "guarded".into(),
            sources: vec![
                ContextSource::WorldKnowledge,
                ContextSource::HistoricalStory,
                ContextSource::CharacterMemory,
            ],
        }],
        character_requests: Vec::new(),
        story_goal: StoryGoal::default(),
    };
    ctx.set_writer_plan(plan).unwrap();

    ContextRetrievalPipeline.execute(&mut ctx).await.expect("retrieval runs");
    assert!(
        ctx.retrieved().len() <= turn.max_retrieved_items,
        "retrieval must cap its output at max_retrieved_items"
    );
    assert!(
        ctx.retrieved().iter().any(|item| item.content.contains("guarded")),
        "the highest-scored candidate must survive the bounded top-k selection"
    );
    assert!(
        ctx.retrieved().iter().all(|item| item.score <= 1.0),
        "unrelated candidates must score below the query match"
    );
}
