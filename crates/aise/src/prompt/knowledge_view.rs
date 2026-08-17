use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::turn::{RelevantWorldKnowledge, RetrievedWorldKnowledge};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorldKnowledgePromptView {
    pub facts: Vec<BoundedText>,
    pub rumors: Vec<BoundedText>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RoleKnowledgePromptView {
    pub known_rumors: Vec<BoundedText>,
    pub memories: Vec<BoundedText>,
}

#[derive(Debug, thiserror::Error)]
pub enum PromptProjectionError {
    #[error("world knowledge merge conflicts on id: {id}")]
    IdConflict { id: String },
}

pub fn merge_world_knowledge(
    baseline: &RelevantWorldKnowledge,
    retrieved: &RetrievedWorldKnowledge,
) -> Result<WorldKnowledgePromptView, PromptProjectionError> {
    Ok(WorldKnowledgePromptView {
        facts: merge_group(
            baseline.facts.iter().map(|entry| (&entry.source_id, &entry.content)),
            retrieved.facts.iter().map(|item| (&item.source_id, &item.content)),
        )?,
        rumors: merge_group(
            baseline.rumors.iter().map(|entry| (&entry.source_id, &entry.content)),
            retrieved.rumors.iter().map(|item| (&item.source_id, &item.content)),
        )?,
    })
}

fn merge_group<'a>(
    baseline: impl Iterator<Item = (&'a KnowledgeSourceId, &'a BoundedText)>,
    retrieved: impl Iterator<Item = (&'a KnowledgeSourceId, &'a BoundedText)>,
) -> Result<Vec<BoundedText>, PromptProjectionError> {
    let mut seen: BTreeMap<KnowledgeSourceId, BoundedText> = BTreeMap::new();
    let mut order = Vec::new();
    for (id, content) in baseline.chain(retrieved) {
        match seen.get(id) {
            Some(existing) if existing.as_str() != content.as_str() => {
                return Err(PromptProjectionError::IdConflict {
                    id: id.as_str().to_owned(),
                });
            }
            Some(_) => {}
            None => {
                seen.insert(id.clone(), content.clone());
                order.push(id.clone());
            }
        }
    }
    Ok(order
        .into_iter()
        .map(|id| seen.remove(&id).expect("tracked id present"))
        .collect())
}

pub fn world_knowledge_view_from_baseline(baseline: &RelevantWorldKnowledge) -> WorldKnowledgePromptView {
    WorldKnowledgePromptView {
        facts: baseline.facts.iter().map(|entry| entry.content.clone()).collect(),
        rumors: baseline.rumors.iter().map(|entry| entry.content.clone()).collect(),
    }
}

pub fn render_relevant_knowledge(knowledge: &WorldKnowledgePromptView) -> String {
    let mut sections = Vec::new();
    if !knowledge.facts.is_empty() {
        let items = knowledge
            .facts
            .iter()
            .map(|content| format!("- {}", quoted(content.as_str())))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Facts\n\n{items}"));
    }
    if !knowledge.rumors.is_empty() {
        let items = knowledge
            .rumors
            .iter()
            .map(|content| format!("- {}", quoted(content.as_str())))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Rumors\n\n{items}"));
    }
    sections.join("\n\n")
}

pub fn render_role_knowledge(knowledge: &RoleKnowledgePromptView) -> String {
    let mut sections = Vec::new();
    if !knowledge.known_rumors.is_empty() {
        let items = knowledge
            .known_rumors
            .iter()
            .map(|content| format!("- {}", quoted(content.as_str())))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Known Rumors\n\n{items}"));
    }
    if !knowledge.memories.is_empty() {
        let items = knowledge
            .memories
            .iter()
            .map(|content| format!("- {}", quoted(content.as_str())))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Memories\n\n{items}"));
    }
    sections.join("\n\n")
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
#[path = "tests/knowledge_view_tests.rs"]
mod tests;
