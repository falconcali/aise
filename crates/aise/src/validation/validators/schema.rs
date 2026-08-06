use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct SchemaValidator;

impl DeterministicValidator for SchemaValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::SchemaInvalid
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            issues.push(issue("missing_proposal", "no story proposal produced", None));
            return Ok(issues);
        };
        if proposal.story_text.trim().is_empty() {
            issues.push(fatal("empty_story", "story text is empty", None));
        }
        if proposal.events.is_empty() {
            issues.push(fatal("missing_events", "proposal has no events", None));
        }
        for (index, event) in proposal.events.iter().enumerate() {
            if event.summary.trim().is_empty() {
                issues.push(issue(
                    "empty_event_summary",
                    "event has an empty summary",
                    Some(ValidationLocation {
                        path: format!("events[{index}]"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
        }
        for (index, fact) in proposal.world_change.add_facts.iter().enumerate() {
            if fact.text.trim().is_empty() {
                issues.push(issue(
                    "empty_world_fact",
                    "world fact text is empty",
                    Some(ValidationLocation {
                        path: format!("world_change.add_facts[{index}]"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
        }
        for (index, memory) in proposal.memory_changes.iter().enumerate() {
            if memory.content.trim().is_empty() {
                issues.push(issue(
                    "empty_memory",
                    "memory content is empty",
                    Some(ValidationLocation {
                        path: format!("memory_changes[{index}]"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
        }
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if change.goal_updates.is_empty() && change.health_delta.is_none() && change.affinity_deltas.is_empty() {
                issues.push(issue(
                    "empty_character_change",
                    "character change carries no goal, health, or affinity update",
                    Some(ValidationLocation {
                        path: format!("character_changes[{index}]"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
        }
        Ok(issues)
    }
}

fn issue(code: &'static str, message: &str, location: Option<ValidationLocation>) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::SchemaInvalid,
        message: format!("{code}: {message}"),
        repairability: Repairability::Repairable,
        location,
    }
}

fn fatal(code: &'static str, message: &str, location: Option<ValidationLocation>) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::SchemaInvalid,
        message: format!("{code}: {message}"),
        repairability: Repairability::Fatal,
        location,
    }
}
