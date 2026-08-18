use crate::domain::asset::validation::BoundedText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryGeneratorOutput {
    pub story_text: BoundedText,
}

#[cfg(test)]
#[path = "tests/story_generation_tests.rs"]
mod tests;
