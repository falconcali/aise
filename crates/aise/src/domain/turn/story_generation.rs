use crate::domain::asset::validation::BoundedText;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryGeneratorOutput {
    pub story_text: BoundedText,
}

impl StoryGeneratorOutput {
    pub fn json_schema(max_story_text_bytes: usize) -> Value {
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "required": ["story_text"],
            "properties": {
                "story_text": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": max_story_text_bytes
                }
            }
        })
    }
}

#[cfg(test)]
#[path = "tests/story_generation_tests.rs"]
mod tests;
