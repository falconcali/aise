use crate::core::{BoundedText, FactEntry, NarrativeDirection, RumorEntry, StoryContext, Turn, TurnStatus};

pub(in crate::pipeline) fn story_summary(story_ctx: &StoryContext) -> Option<&str> {
    story_ctx
        .summary
        .as_ref()
        .map(|summary| summary.text.trim())
        .filter(|text| !text.is_empty())
}

pub(in crate::pipeline) fn recent_story(story_ctx: &StoryContext) -> String {
    turns_story(story_ctx.recent_turns.iter())
}

pub(in crate::pipeline) fn turns_story<'a>(turns: impl Iterator<Item = &'a Turn>) -> String {
    turns
        .filter(|turn| matches!(turn.turn_status, TurnStatus::Pass | TurnStatus::Choice))
        .map(|turn| turn.turn_segment.text().trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(in crate::pipeline) fn narrative_directions(directions: &[NarrativeDirection]) -> String {
    bullet_list(directions.iter().map(|direction| direction.dramatic_focus.as_str()))
}

pub(in crate::pipeline) fn story_constraints(constraints: &[BoundedText]) -> String {
    bullet_list(constraints.iter().map(BoundedText::as_str))
}

pub(in crate::pipeline) fn known_facts(facts: &[FactEntry]) -> String {
    known_facts_with_retrieved(facts, &[])
}

pub(in crate::pipeline) fn known_facts_with_retrieved(activated: &[FactEntry], retrieved: &[FactEntry]) -> String {
    bullet_list(activated.iter().chain(retrieved).map(|fact| fact.body.content.as_str()))
}

pub(in crate::pipeline) fn rumors(rumors: &[RumorEntry]) -> String {
    rumors_with_retrieved(rumors, &[])
}

pub(in crate::pipeline) fn rumors_with_retrieved(activated: &[RumorEntry], retrieved: &[RumorEntry]) -> String {
    bullet_list(activated.iter().chain(retrieved).map(|rumor| rumor.body.content.as_str()))
}

fn bullet_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    items
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(in crate::pipeline) fn story_opening(story_ctx: &StoryContext) -> &str {
    match story_summary(story_ctx) {
        Some(_) => "",
        None => story_ctx.opening.as_str(),
    }
}

#[cfg(test)]
#[path = "tests/story_context_prompt_tests.rs"]
mod tests;
