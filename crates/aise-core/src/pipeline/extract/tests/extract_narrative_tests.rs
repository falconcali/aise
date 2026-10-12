use super::*;
use crate::core::{NarrativeNodeId, NodeStatus};
use crate::test_support::snake_pack;
use serde_json::json;

fn pack_with_narrative() -> StoryPack {
    let mut pack = snake_pack();
    pack.narrative = Some(
        serde_json::from_value(json!({ "nodes": {
            "arrival": {
                "title": "Arrival",
                "activate_when": { "type": "story_started" },
                "complete_when": { "type": "turn_reaches", "turn": 2 },
                "terminal": true
            }
        }}))
        .expect("test graph should deserialize"),
    );
    pack
}

fn context_at(turn: u64, arrival: NodeStatus) -> StoryContext {
    let mut story_ctx = StoryContext::new();
    story_ctx.turn_number = crate::core::TurnNumber::new(turn);
    story_ctx.narrative.node_status.insert(
        NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        arrival,
    );
    story_ctx
}

#[test]
fn pack_without_narrative_changes_nothing() {
    let mut pack = pack_with_narrative();
    pack.narrative = None;

    let change = resolve_narrative(&pack, &StoryContext::new(), &BTreeMap::new());

    assert!(matches!(change.narrative, Change::Unchanged));
    assert!(matches!(change.life_cycle, Change::Unchanged));
}

#[test]
fn no_transition_keeps_narrative_unchanged() {
    let change = resolve_narrative(&pack_with_narrative(), &context_at(0, NodeStatus::Active), &BTreeMap::new());

    assert!(matches!(change.narrative, Change::Unchanged));
    assert!(matches!(change.life_cycle, Change::Unchanged));
}

#[test]
fn resolution_uses_the_pending_turn_number() {
    let change = resolve_narrative(&pack_with_narrative(), &context_at(1, NodeStatus::Active), &BTreeMap::new());

    let Change::Replaced(narrative) = change.narrative else {
        panic!("narrative should change when the pending turn reaches the condition");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("test node id should be valid")),
        NodeStatus::Completed
    );
    assert!(matches!(change.life_cycle, Change::Replaced(StoryLifeCycle::Completed)));
}
