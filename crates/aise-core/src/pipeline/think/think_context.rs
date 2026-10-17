use crate::core::{ResolvedRole, RumorEntry};
use crate::pipeline::think::ThinkConfig;
use serde_json::Value;

pub(super) fn memories(role: &ResolvedRole, config: &ThinkConfig) -> String {
    let mut rendered = String::new();
    for (index, memory) in role.memories.values().enumerate() {
        if index >= config.max_seed_memories {
            break;
        }
        let item = format!("- {}", json_string(memory.body.content.as_str()));
        let separator_bytes = usize::from(!rendered.is_empty());
        if rendered.len().saturating_add(separator_bytes).saturating_add(item.len()) > config.max_seed_memory_bytes {
            break;
        }
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&item);
    }
    rendered
}

pub(super) fn rumors(activated: &[RumorEntry], retrieved: &[RumorEntry], config: &ThinkConfig) -> String {
    let mut rendered = String::new();
    for rumor in activated.iter().chain(retrieved) {
        let item = format!("- {}", json_string(rumor.body.content.as_str()));
        let separator_bytes = usize::from(!rendered.is_empty());
        if rendered.len().saturating_add(separator_bytes).saturating_add(item.len()) > config.max_rumor_bytes {
            break;
        }
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&item);
    }
    rendered
}

fn json_string(value: &str) -> Value {
    Value::String(value.to_owned())
}

#[cfg(test)]
#[path = "tests/think_context_tests.rs"]
mod tests;
