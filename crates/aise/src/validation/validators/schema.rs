use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_validation::ValidationResult;
use crate::error::AiseError;

#[derive(Default)]
pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        let proposal = match ctx.proposal() {
            Some(p) => p,
            None => {
                return Ok(ValidationResult::reject("missing_proposal", "no story proposal produced"));
            }
        };
        if proposal.story_text.trim().is_empty() {
            return Ok(ValidationResult::reject("empty_story", "story text is empty"));
        }
        if proposal.events.is_empty() {
            return Ok(ValidationResult::reject("missing_events", "proposal has no events"));
        }
        if let Some(event) = proposal.events.iter().find(|event| event.summary.trim().is_empty()) {
            return Ok(ValidationResult::reject(
                "empty_event_summary",
                format!("event {:?} has an empty summary", event.kind),
            ));
        }
        if proposal.world_change.add_facts.iter().any(|fact| fact.trim().is_empty()) {
            return Ok(ValidationResult::reject("empty_world_fact", "world fact text is empty"));
        }
        if proposal.memory_changes.iter().any(|memory| memory.content.trim().is_empty()) {
            return Ok(ValidationResult::reject("empty_memory", "memory content is empty"));
        }
        if proposal.character_changes.iter().any(|change| {
            change.goal_updates.is_empty() && change.health_delta.is_none() && change.affinity_deltas.is_empty()
        }) {
            return Ok(ValidationResult::reject(
                "empty_character_change",
                "character change carries no goal, health, or affinity update",
            ));
        }
        Ok(ValidationResult::pass())
    }
}
