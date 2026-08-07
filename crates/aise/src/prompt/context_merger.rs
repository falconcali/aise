use crate::core::turn_data::{BaselineContext, CharacterThought, ContextItem, ContextSource, WriterPlan};
use crate::core::turn_trace::truncate;
use crate::domain::character::CharacterState;
use crate::llm::message::{ChatMessage, Role};
use std::fmt::Write;

const MAX_RECENT_STORY_ITEMS: usize = 8;
const MAX_RECENT_STORY_CHARS: usize = 800;
const MAX_CHARACTER_ITEMS: usize = 12;
const MAX_CHARACTER_CHARS: usize = 500;
const MAX_RETRIEVED_ITEMS: usize = 10;
const MAX_RETRIEVED_CHARS: usize = 500;
const MAX_THOUGHT_ITEMS: usize = 6;
const MAX_THOUGHT_CHARS: usize = 400;
const MAX_ISSUE_ITEMS: usize = 5;
const MAX_ISSUE_CHARS: usize = 300;
const MAX_PLAYER_INPUT_CHARS: usize = 4096;

const STORY_PROPOSAL_SCHEMA: &str = r#"{
  "story_text": "the story text",
  "events": [{"kind": "dialogue" | "action" | "world_change" | "chapter", "summary": "event summary"}],
  "character_changes": [{"character_id": "existing character id", "goal_updates": ["new goal"], "health_delta": 0, "affinity_deltas": [{"other": "existing character id", "delta": 0}]}],
  "world_change": {"add_facts": [{"text": "new world fact", "evidence": [{"snapshot_fact": "existing fact id"} or {"proposed_event": {"event_index": 0}}]}]},
  "memory_changes": [{"owner": "existing character id", "kind": "observed" | "inferred" | "secret", "content": "memory text"}],
  "summary_change": {"text": "updated story summary"} or null
}

Keep every field exactly in the shape above: "events" entries are objects with "kind" and "summary" fields, "add_facts" entries are objects with "text" and "evidence" fields, "character_changes" entries are objects with a "character_id" field, and "summary_change" is an object with a "text" field or null. Never output any of these fields as a plain string."#;

pub struct ContextMerger;

pub struct GenerationInput<'a> {
    pub baseline: &'a BaselineContext,
    pub plan: &'a WriterPlan,
    pub retrieved: &'a [ContextItem],
    pub thoughts: &'a [CharacterThought],
    pub player_input: &'a str,
    pub issues: &'a [String],
    pub previous_story: Option<&'a str>,
}

impl ContextMerger {
    pub fn plan_messages(&self, baseline: &BaselineContext, player_input: &str) -> Vec<ChatMessage> {
        // TODO(temp-debug): the planner no longer asks the LLM for retrieval/character requests while the
        // baseline builder is being debugged. Restore the original prompt that plans retrieval_requests,
        // character_requests, and the story_goal.
        let system = "You are the story planner of an interactive fiction engine. \
You plan a single player turn. Decide the story goal for the turn. Respond with only a JSON object of this shape: \
{\"story_goal\":{\"summary\":\"...\"}}.";
        let mut user = String::new();
        if !baseline.story_summary.trim().is_empty() {
            let _ = writeln!(
                user,
                "Story summary:\n{}\n",
                truncate(&baseline.story_summary, MAX_RECENT_STORY_CHARS)
            );
        }
        if let Some(scene) = baseline.current_scene.as_deref() {
            let _ = writeln!(user, "Current scene:\n{}\n", truncate(scene, MAX_RECENT_STORY_CHARS));
        }
        if !baseline.relevant_characters.is_empty() {
            let _ = writeln!(user, "Characters:\n{}", characters_block(&baseline.relevant_characters));
        }
        let _ = writeln!(user, "Player input:\n{}", truncate(player_input, MAX_PLAYER_INPUT_CHARS));
        vec![system_message(system), user_message(&user)]
    }

    pub fn thought_messages(
        &self,
        character: &CharacterState,
        player_input: &str,
        current_scene: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut system = format!("You are {}, a character in an interactive fiction story.", character.name);
        if !character.bio.trim().is_empty() {
            let _ = write!(system, "\nBio: {}", truncate(&character.bio, MAX_CHARACTER_CHARS));
        }
        if !character.internal_state.goals.is_empty() {
            let goals = character
                .internal_state
                .goals
                .iter()
                .take(3)
                .map(|goal| truncate(goal, 120))
                .collect::<Vec<_>>()
                .join("; ");
            let _ = write!(system, "\nCurrent goals: {goals}");
        }
        system.push_str(
            "\nDescribe your perception, emotion, current goal, and possible action. \
Respond with only a JSON object of this shape: \
{\"perception\":\"...\",\"emotion\":\"...\",\"goal\":\"...\",\"possible_action\":\"...\"}.",
        );
        let mut user = String::new();
        if let Some(scene) = current_scene {
            let _ = writeln!(user, "Current scene:\n{}\n", truncate(scene, MAX_RECENT_STORY_CHARS));
        }
        let _ = writeln!(user, "Player action:\n{}", truncate(player_input, MAX_PLAYER_INPUT_CHARS));
        vec![system_message(&system), user_message(&user)]
    }

    pub fn generation_messages(&self, input: GenerationInput<'_>) -> Vec<ChatMessage> {
        let baseline = input.baseline;
        let plan = input.plan;
        let retrieved = input.retrieved;
        let thoughts = input.thoughts;
        let player_input = input.player_input;
        let issues = input.issues;
        let previous_story = input.previous_story;
        let mut system = instructions_block(baseline);
        let _ = writeln!(
            system,
            "You are the story writer of an interactive fiction engine. Write the next part of the story \
responding to the player action. You may propose events, character changes, world facts, memories, and a summary \
change. Every proposed world fact must reference at least one piece of evidence: either an existing snapshot fact \
or one of the proposed events. You may introduce new characters in the story text who are not in the character \
list, but character_changes, memory_changes, and affinity targets may only reference characters from the list. \
Respond with only a JSON object matching this schema:\n{STORY_PROPOSAL_SCHEMA}"
        );
        if !issues.is_empty() {
            let _ = writeln!(
                system,
                "The previous draft was rejected by validation. Fix these issues and produce a complete new JSON \
object:\n{}",
                issues_block(issues)
            );
        }
        let mut user = String::new();
        if !baseline.story_summary.trim().is_empty() {
            let _ = writeln!(
                user,
                "Story summary:\n{}\n",
                truncate(&baseline.story_summary, MAX_RECENT_STORY_CHARS)
            );
        }
        if let Some(scene) = baseline.current_scene.as_deref() {
            let _ = writeln!(user, "Current scene:\n{}\n", truncate(scene, MAX_RECENT_STORY_CHARS));
        }
        if !baseline.relevant_characters.is_empty() {
            let _ = writeln!(user, "Characters:\n{}", characters_block(&baseline.relevant_characters));
        }
        if !baseline.recent_story.is_empty() {
            let _ = writeln!(user, "Recent story:\n{}", recent_story_block(&baseline.recent_story));
        }
        if !retrieved.is_empty() {
            let _ = writeln!(user, "Retrieved context:\n{}", retrieved_block(retrieved));
        }
        if !thoughts.is_empty() {
            let _ = writeln!(user, "Character thoughts:\n{}", thoughts_block(thoughts));
        }
        if !plan.story_goal.summary.trim().is_empty() {
            let _ = writeln!(
                user,
                "Story goal:\n{}",
                truncate(&plan.story_goal.summary, MAX_RECENT_STORY_CHARS)
            );
        }
        if let Some(story) = previous_story {
            let _ = writeln!(user, "Previous draft:\n{}", truncate(story, MAX_RECENT_STORY_CHARS));
        }
        let _ = writeln!(user, "Player input:\n{}", truncate(player_input, MAX_PLAYER_INPUT_CHARS));
        vec![system_message(&system), user_message(&user)]
    }
}

fn instructions_block(baseline: &BaselineContext) -> String {
    let mut out = String::new();
    if !baseline.story_instructions.trim().is_empty() {
        let _ = writeln!(out, "Story instructions:\n{}", truncate(&baseline.story_instructions, 4000));
    }
    if let Some(style) = &baseline.story_config.style {
        if !style.trim().is_empty() {
            let _ = writeln!(out, "Style: {}", style.trim());
        }
    }
    if let Some(point_of_view) = &baseline.story_config.point_of_view {
        if !point_of_view.trim().is_empty() {
            let _ = writeln!(out, "Point of view: {}", point_of_view.trim());
        }
    }
    if let Some(tense) = &baseline.story_config.tense {
        if !tense.trim().is_empty() {
            let _ = writeln!(out, "Tense: {}", tense.trim());
        }
    }
    out
}

fn characters_block(characters: &[CharacterState]) -> String {
    let mut out = String::new();
    for character in characters.iter().take(MAX_CHARACTER_ITEMS) {
        let bio = if character.bio.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", truncate(&character.bio, MAX_CHARACTER_CHARS))
        };
        let goals = if character.internal_state.goals.is_empty() {
            String::new()
        } else {
            let goals = character
                .internal_state
                .goals
                .iter()
                .take(3)
                .map(|goal| truncate(goal, 120))
                .collect::<Vec<_>>()
                .join("; ");
            format!(" [goals: {goals}]")
        };
        let _ = writeln!(out, "- {} (id: {}){}{}", character.name, character.id, bio, goals);
    }
    out
}

fn recent_story_block(turns: &[String]) -> String {
    let mut out = String::new();
    for (index, text) in turns.iter().take(MAX_RECENT_STORY_ITEMS).enumerate() {
        let _ = writeln!(out, "{}. {}", index + 1, truncate(text, MAX_RECENT_STORY_CHARS));
    }
    out
}

fn retrieved_block(items: &[ContextItem]) -> String {
    let mut out = String::new();
    for item in items.iter().take(MAX_RETRIEVED_ITEMS) {
        let _ = writeln!(
            out,
            "- [{}] {}",
            source_label(item.source),
            truncate(&item.content, MAX_RETRIEVED_CHARS)
        );
    }
    out
}

fn thoughts_block(thoughts: &[CharacterThought]) -> String {
    let mut out = String::new();
    for thought in thoughts.iter().take(MAX_THOUGHT_ITEMS) {
        let _ = writeln!(
            out,
            "- {}: perception={}; emotion={}; goal={}",
            thought.character_id,
            truncate(&thought.perception, MAX_THOUGHT_CHARS),
            truncate(&thought.emotion, MAX_THOUGHT_CHARS),
            truncate(&thought.goal, MAX_THOUGHT_CHARS)
        );
    }
    out
}

fn issues_block(issues: &[String]) -> String {
    let mut out = String::new();
    for issue in issues.iter().take(MAX_ISSUE_ITEMS) {
        let _ = writeln!(out, "- {}", truncate(issue, MAX_ISSUE_CHARS));
    }
    out
}

fn source_label(source: ContextSource) -> &'static str {
    match source {
        ContextSource::CharacterMemory => "character_memory",
        ContextSource::WorldKnowledge => "world_knowledge",
        ContextSource::NarrativeGraph => "narrative_graph",
        ContextSource::HistoricalStory => "historical_story",
    }
}

fn system_message(content: impl Into<String>) -> ChatMessage {
    ChatMessage {
        role: Role::System,
        content: content.into(),
    }
}

fn user_message(content: &str) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content: content.to_owned(),
    }
}

#[cfg(test)]
#[path = "tests/context_merger_tests.rs"]
mod tests;
