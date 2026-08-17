use super::*;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{CanonicalEventKey, LocationKey, NarrativeNodeKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::effect::{NarrativeEffectId, WorldEventIntent};
use crate::domain::narrative_graph::projector::{NarrativeDirection, NarrativePlan};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "text", 256).unwrap()
}

fn node_key(value: &str) -> NarrativeNodeKey {
    NarrativeNodeKey::try_new(value).unwrap()
}

#[test]
fn project_narrative_direction_maps_active_directions_and_world_event_intents() {
    let mut plan = NarrativePlan::empty();
    plan.active_directions.push(NarrativeDirection {
        source_node: node_key("node_climax"),
        dramatic_focus: text("The storm closes in on the village."),
    });
    plan.world_event_intents.push(WorldEventIntent {
        effect_id: NarrativeEffectId::try_new("narrative-effect:node_climax:activate:1:0").unwrap(),
        source_node: node_key("node_climax"),
        event_key: CanonicalEventKey::try_new("storm_arrives").unwrap(),
        category: text("weather"),
        participants: vec![KnowledgeEntity::Location(LocationKey::try_new("village").unwrap())],
        location: Some(LocationKey::try_new("village").unwrap()),
        description: text("A violent storm reaches the village."),
    });

    let view = project_narrative_direction(&plan);

    assert_eq!(view.active_directions.len(), 1);
    assert_eq!(view.active_directions[0].as_str(), "The storm closes in on the village.");
    assert_eq!(view.world_event_intents.len(), 1);
    let intent = &view.world_event_intents[0];
    assert_eq!(intent.category.as_str(), "weather");
    assert_eq!(intent.participants.len(), 1);
    assert_eq!(intent.location.as_ref().unwrap().as_str(), "village");
    assert_eq!(intent.description.as_str(), "A violent storm reaches the village.");
}

#[test]
fn render_narrative_direction_returns_empty_string_when_view_is_empty() {
    let view = NarrativeDirectionPromptView::default();
    assert_eq!(render_narrative_direction(&view), "");
    assert!(view.is_empty());
}

#[test]
fn render_narrative_direction_renders_active_directions_only() {
    let view = NarrativeDirectionPromptView {
        active_directions: vec![text("Tension rises between the two factions.")],
        world_event_intents: Vec::new(),
    };
    let rendered = render_narrative_direction(&view);
    assert_eq!(
        rendered,
        "### Active Directions\n\n- \"Tension rises between the two factions.\""
    );
}

#[test]
fn render_narrative_direction_renders_world_event_intents_with_participants_and_location() {
    let view = NarrativeDirectionPromptView {
        active_directions: Vec::new(),
        world_event_intents: vec![WorldEventIntentPromptView {
            category: text("ambush"),
            participants: vec![
                KnowledgeEntity::Role(RoleId::try_new("role_bandit_leader").unwrap()),
                KnowledgeEntity::Location(LocationKey::try_new("forest_road").unwrap()),
            ],
            location: Some(LocationKey::try_new("forest_road").unwrap()),
            description: text("Bandits ambush travelers on the forest road."),
        }],
    };
    let rendered = render_narrative_direction(&view);
    assert_eq!(
        rendered,
        "### World Event Intents\n\n- category: \"ambush\"\n  participants: [\"role:role_bandit_leader\", \"location:forest_road\"]\n  location: \"forest_road\"\n  description: \"Bandits ambush travelers on the forest road.\""
    );
}

#[test]
fn render_narrative_direction_omits_location_line_when_absent() {
    let view = NarrativeDirectionPromptView {
        active_directions: Vec::new(),
        world_event_intents: vec![WorldEventIntentPromptView {
            category: text("rumor_spread"),
            participants: Vec::new(),
            location: None,
            description: text("Word of the ambush spreads through the region."),
        }],
    };
    let rendered = render_narrative_direction(&view);
    assert_eq!(
        rendered,
        "### World Event Intents\n\n- category: \"rumor_spread\"\n  description: \"Word of the ambush spreads through the region.\""
    );
}

#[test]
fn render_narrative_direction_joins_both_sections_with_blank_line() {
    let view = NarrativeDirectionPromptView {
        active_directions: vec![text("A confrontation looms.")],
        world_event_intents: vec![WorldEventIntentPromptView {
            category: text("confrontation"),
            participants: Vec::new(),
            location: None,
            description: text("Rival factions prepare to clash."),
        }],
    };
    let rendered = render_narrative_direction(&view);
    assert_eq!(
        rendered,
        "### Active Directions\n\n- \"A confrontation looms.\"\n\n### World Event Intents\n\n- category: \"confrontation\"\n  description: \"Rival factions prepare to clash.\""
    );
}
