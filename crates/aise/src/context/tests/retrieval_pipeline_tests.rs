use super::*;
use crate::core::turn_contract::StoryRevision;
use crate::core::turn_data::{BaselineContext, ContextSource, StoryReadSnapshot};
use crate::domain::character::{CharacterState, InternalState};
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::{MemoryEntry, MemoryKind};
use crate::domain::world::{FactSource, WorldFact, WorldState};

fn snapshot() -> StoryReadSnapshot {
    let world = WorldState {
        id: StoryId::from("story-1"),
        name: "world".into(),
        facts: vec![WorldFact {
            id: crate::domain::ids::FactId::from("f-1"),
            text: "the gate is guarded".into(),
            source: FactSource::Seed,
        }],
    };
    StoryReadSnapshot::new(
        StoryId::from("story-1"),
        StoryRevision::new(0),
        None,
        Some(world),
        vec![CharacterState {
            id: CharacterId::from("c-1"),
            name: "Mira".into(),
            bio: "bio".into(),
            internal_state: InternalState::default(),
        }],
        Vec::new(),
        vec![MemoryEntry {
            id: crate::domain::ids::MemoryId::from("m-1"),
            owner: CharacterId::from("c-1"),
            kind: MemoryKind::Observed,
            content: "Mira saw the gate".into(),
            created_at: 0,
        }],
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
