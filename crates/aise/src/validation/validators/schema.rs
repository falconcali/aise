use crate::core::story_proposal::ProposedKnowledgeChange;
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
        let Some(proposal) = ctx.proposal() else {
            return Ok(vec![issue("proposal", 0, "proposal is missing", Repairability::Fatal)]);
        };
        let mut issues = Vec::new();
        if proposal.story_text.trim().is_empty() {
            issues.push(issue("story_text", 0, "story text is empty", Repairability::Fatal));
        }
        for (path, count) in [
            ("events", proposal.events.len()),
            ("character_changes", proposal.character_changes.len()),
            ("relationship_changes", proposal.relationship_changes.len()),
            ("knowledge_changes", proposal.knowledge_changes.len()),
            ("perceptions", proposal.perceptions.len()),
        ] {
            if count > ctx.budget().max_total_items() {
                issues.push(issue(path, 0, "proposal collection exceeds its bound", Repairability::Fatal));
            }
        }
        if proposal.story_text.len() > ctx.budget().max_proposal_bytes()
            || proposal
                .summary_text
                .as_ref()
                .is_some_and(|text| text.len() > ctx.budget().max_proposal_bytes())
        {
            issues.push(issue("story_text", 0, "proposal text exceeds its bound", Repairability::Fatal));
        }
        for (index, event) in proposal.events.iter().enumerate() {
            if event.summary.trim().is_empty() || event.summary.len() > ctx.budget().max_item_bytes() {
                issues.push(issue("events", index, "event summary is empty", Repairability::Repairable));
            }
        }
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if change.location.is_none() && change.goals.is_none() && change.attribute_updates.is_empty() {
                issues.push(issue(
                    "character_changes",
                    index,
                    "character change is empty",
                    Repairability::Repairable,
                ));
            }
            if change.goals.as_ref().is_some_and(|goals| {
                goals.len() > ctx.budget().max_total_items()
                    || goals.iter().any(|goal| goal.len() > ctx.budget().max_item_bytes())
            }) {
                issues.push(issue(
                    "character_changes",
                    index,
                    "character goals exceed their bound",
                    Repairability::Fatal,
                ));
            }
        }
        for (index, change) in proposal.knowledge_changes.iter().enumerate() {
            let content = match change {
                ProposedKnowledgeChange::Fact { content, .. }
                | ProposedKnowledgeChange::Rumor { content, .. }
                | ProposedKnowledgeChange::Memory { content, .. } => content,
            };
            if content.trim().is_empty() || content.len() > ctx.budget().max_item_bytes() {
                issues.push(issue(
                    "knowledge_changes",
                    index,
                    "knowledge content is empty",
                    Repairability::Repairable,
                ));
            }
        }
        for (index, perception) in proposal.perceptions.iter().enumerate() {
            if perception.content.trim().is_empty() || perception.content.len() > ctx.budget().max_item_bytes() {
                issues.push(issue(
                    "perceptions",
                    index,
                    "perception content is empty or oversized",
                    Repairability::Fatal,
                ));
            }
        }
        if proposal.scene_change.as_ref().is_some_and(|scene| {
            scene.time.as_str().len() > ctx.budget().max_item_bytes()
                || scene.description.as_str().len() > ctx.budget().max_proposal_bytes()
                || scene.present_character_ids.len() > ctx.budget().max_total_items()
        }) {
            issues.push(issue("scene_change", 0, "scene exceeds its bound", Repairability::Fatal));
        }
        Ok(issues)
    }
}

fn issue(path: &str, index: usize, message: &str, repairability: Repairability) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::SchemaInvalid,
        message: message.into(),
        repairability,
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}
