use super::*;
use crate::core::tests::snake_pack;
use crate::core::{NarrativeConditionId, NarrativeNodeId, NodeStatus};
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

fn semantic_pack() -> StoryPack {
    let mut pack = snake_pack();
    pack.narrative = Some(
        serde_json::from_value(json!({ "nodes": {
            "arrival": {
                "title": "Arrival",
                "activate_when": { "type": "story_started" },
                "complete_when": {
                    "type": "all",
                    "conditions": [
                        { "type": "semantic", "condition_id": "visitor_identified", "criterion": "The visitor is identified" },
                        { "type": "semantic", "condition_id": "gate_open", "criterion": "The gate is open" }
                    ]
                },
                "terminal": true
            }
        }}))
        .expect("test graph should deserialize"),
    );
    pack
}

fn semantic_results(truth: Truth) -> BTreeMap<NarrativeConditionId, Truth> {
    ["gate_open", "visitor_identified"]
        .into_iter()
        .map(|id| (NarrativeConditionId::try_new(id).expect("valid condition id"), truth))
        .collect()
}

#[test]
fn pack_without_narrative_collects_no_queries() {
    let mut pack = pack_with_narrative();
    pack.narrative = None;

    let queries = collect_semantic_queries(&pack, &StoryContext::new()).expect("query collection should succeed");

    assert!(queries.is_empty());
}

#[test]
fn semantic_queries_are_collected_in_condition_id_order() {
    let queries =
        collect_semantic_queries(&semantic_pack(), &context_at(0, NodeStatus::Active)).expect("queries should collect");

    assert_eq!(
        queries.iter().map(|query| query.condition_id.as_str()).collect::<Vec<_>>(),
        ["gate_open", "visitor_identified"]
    );
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

#[test]
fn satisfied_semantic_results_complete_terminal_node() {
    let change = resolve_narrative(
        &semantic_pack(),
        &context_at(0, NodeStatus::Active),
        &semantic_results(Truth::Satisfied),
    );

    let Change::Replaced(narrative) = change.narrative else {
        panic!("satisfied semantic conditions should change narrative");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("valid node id")),
        NodeStatus::Completed
    );
    assert!(matches!(change.life_cycle, Change::Replaced(StoryLifeCycle::Completed)));
}

#[test]
fn unsatisfied_semantic_results_do_not_complete_node() {
    let change = resolve_narrative(
        &semantic_pack(),
        &context_at(0, NodeStatus::Active),
        &semantic_results(Truth::Unsatisfied),
    );

    assert!(matches!(change.narrative, Change::Unchanged));
    assert!(matches!(change.life_cycle, Change::Unchanged));
}

#[test]
fn unknown_semantic_results_do_not_complete_node() {
    let change = resolve_narrative(
        &semantic_pack(),
        &context_at(0, NodeStatus::Active),
        &semantic_results(Truth::Unknown),
    );

    assert!(matches!(change.narrative, Change::Unchanged));
    assert!(matches!(change.life_cycle, Change::Unchanged));
}
