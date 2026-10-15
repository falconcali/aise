use crate::core::ResolvedRole;
use crate::pipeline::think::ThinkConfig;
use serde_json::Value;

pub(super) fn seed_memories(role: &ResolvedRole, config: &ThinkConfig) -> String {
    let mut rendered = String::new();
    for (index, (memory_id, memory)) in role.seed_memories.iter().enumerate() {
        if index >= config.max_seed_memories {
            break;
        }
        let item = format!(
            "- memory_id: {}\n  content: {}\n  retrieval_hint: {}",
            json_string(memory_id.as_str()),
            json_string(memory.content.as_str()),
            json_string(memory.retrieval.as_str())
        );
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

fn json_string(value: &str) -> Value {
    Value::String(value.to_owned())
}

#[cfg(test)]
#[path = "tests/think_context_tests.rs"]
mod tests;
