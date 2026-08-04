use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreamblePosition {
    Prepend,
    Append,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum PromptPolicy {
    Preamble {
        name: String,
        content: String,
        position: PreamblePosition,
    },
    RuntimeGuard {
        name: String,
        description: String,
    },
    PostValidator {
        name: String,
    },
}

impl PromptPolicy {
    pub fn name(&self) -> &str {
        match self {
            PromptPolicy::Preamble { name, .. } => name,
            PromptPolicy::RuntimeGuard { name, .. } => name,
            PromptPolicy::PostValidator { name } => name,
        }
    }

    pub fn apply_to_text(&self, text: &str) -> Option<String> {
        match self {
            PromptPolicy::Preamble { content, position, .. } => {
                let result = match position {
                    PreamblePosition::Prepend => format!("{}\n{}", content, text),
                    PreamblePosition::Append => format!("{}\n{}", text, content),
                };
                Some(result)
            }
            PromptPolicy::RuntimeGuard { .. } | PromptPolicy::PostValidator { .. } => None,
        }
    }
}

#[cfg(test)]
#[path = "tests/policy_tests.rs"]
mod tests;
