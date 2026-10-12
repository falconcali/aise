use super::*;
use crate::core::tests::snake_pack;
use crate::core::{NarrativeNodeId, NodeStatus};
use serde_json::json;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn graph() -> crate::core::NarrativeGraph {
    serde_json::from_value(json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "dramatic_focus": "The traveler meets the forest keeper.",
            "activate_when": { "type": "story_started" },
            "complete_when": { "type": "semantic", "condition_id": "identified", "criterion": "visitor identified" },
            "constraints": [{ "forbid": "The ritual truth is revealed.", "window": "while_active" }]
        }
    }}))
    .expect("test graph should deserialize")
}

#[test]
fn pack_without_narrative_projects_only_global_constraints() {
    let mut pack = snake_pack();
    pack.constraints = vec![text("Modern technology appears.")];
    pack.narrative = None;

    let output = project_narrative(&pack, &StoryContext::new()).expect("projection should succeed");

    assert!(output.directions.is_empty());
    assert!(output.queries.is_empty());
    assert_eq!(output.constraints, [text("Modern technology appears.")]);
}

#[test]
fn global_constraints_come_before_active_node_constraints() {
    let mut pack = snake_pack();
    pack.constraints = vec![text("Modern technology appears.")];
    pack.narrative = Some(graph());
    let mut story_ctx = StoryContext::new();
    story_ctx.narrative.node_status.insert(
        NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        NodeStatus::Active,
    );

    let output = project_narrative(&pack, &story_ctx).expect("projection should succeed");

    assert_eq!(
        output.constraints,
        [
            text("Modern technology appears."),
            text("The ritual truth is revealed.")
        ]
    );
    assert_eq!(output.directions.len(), 1);
    assert_eq!(output.queries.len(), 1);
    assert_eq!(output.queries[0].condition_id.as_str(), "identified");
}
