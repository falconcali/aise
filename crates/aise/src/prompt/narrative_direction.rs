use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::LocationKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::narrative_graph::projector::NarrativePlan;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WorldEventIntentPromptView {
    pub category: BoundedText,
    pub participants: Vec<KnowledgeEntity>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NarrativeDirectionPromptView {
    pub active_directions: Vec<BoundedText>,
    pub world_event_intents: Vec<WorldEventIntentPromptView>,
}

impl NarrativeDirectionPromptView {
    pub fn is_empty(&self) -> bool {
        self.active_directions.is_empty() && self.world_event_intents.is_empty()
    }
}

pub fn project_narrative_direction(plan: &NarrativePlan) -> NarrativeDirectionPromptView {
    NarrativeDirectionPromptView {
        active_directions: plan
            .active_directions
            .iter()
            .map(|direction| direction.dramatic_focus.clone())
            .collect(),
        world_event_intents: plan
            .world_event_intents
            .iter()
            .map(|intent| WorldEventIntentPromptView {
                category: intent.category.clone(),
                participants: intent.participants.clone(),
                location: intent.location.clone(),
                description: intent.description.clone(),
            })
            .collect(),
    }
}

pub fn render_narrative_direction(view: &NarrativeDirectionPromptView) -> String {
    let mut sections = Vec::new();
    if !view.active_directions.is_empty() {
        let items = view
            .active_directions
            .iter()
            .map(|direction| format!("- {}", direction.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Active Directions\n\n{items}"));
    }
    if !view.world_event_intents.is_empty() {
        let items = view
            .world_event_intents
            .iter()
            .map(render_world_event_intent)
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### World Event Intents\n\n{items}"));
    }
    sections.join("\n\n")
}

fn render_world_event_intent(intent: &WorldEventIntentPromptView) -> String {
    let mut lines = vec![format!("- category: {}", quoted(intent.category.as_str()))];
    if !intent.participants.is_empty() {
        let participants = intent
            .participants
            .iter()
            .map(|entity| quoted(&entity_reference(entity)))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("  participants: [{participants}]"));
    }
    if let Some(location) = &intent.location {
        lines.push(format!("  location: {}", quoted(location.as_str())));
    }
    lines.push(format!("  description: {}", quoted(intent.description.as_str())));
    lines.join("\n")
}

fn entity_reference(entity: &KnowledgeEntity) -> String {
    match entity {
        KnowledgeEntity::World(key) => format!("world:{}", key.as_str()),
        KnowledgeEntity::Role(id) => format!("role:{}", id.as_str()),
        KnowledgeEntity::Location(key) => format!("location:{}", key.as_str()),
        KnowledgeEntity::Scene(key) => format!("scene:{}", key.as_str()),
        KnowledgeEntity::NarrativeNode(key) => format!("narrative_node:{}", key.as_str()),
        KnowledgeEntity::Event(key) => format!("event:{}", key.as_str()),
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
#[path = "tests/narrative_direction_tests.rs"]
mod tests;
