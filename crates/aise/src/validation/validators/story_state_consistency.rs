use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::ValidationIssue;
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct StoryStateConsistencyValidator;

impl DeterministicValidator for StoryStateConsistencyValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let (Some(_story), Some(_extraction)) = (ctx.story(), ctx.extraction()) else {
            return Ok(Vec::new());
        };
        Ok(Vec::new())
    }
}

#[cfg(test)]
#[path = "tests/story_state_consistency_tests.rs"]
mod tests;
