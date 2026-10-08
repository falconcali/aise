use crate::core::{StoryContext, TurnStatus};
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage};

pub(in crate::pipeline) fn story_summary(story_ctx: &StoryContext) -> Option<&str> {
    story_ctx
        .summary
        .as_ref()
        .map(|summary| summary.text.trim())
        .filter(|text| !text.is_empty())
}

pub(in crate::pipeline) fn recent_story(story_ctx: &StoryContext) -> String {
    story_ctx
        .rencent_turns
        .iter()
        .filter(|turn| matches!(turn.turn_status, TurnStatus::Accepted))
        .map(|turn| turn.turn_segment.text().trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(in crate::pipeline) async fn load_story_opening(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    story_store: &dyn StoryStore,
) -> Result<String, PipelineError> {
    if story_summary(story_ctx).is_some() {
        return Ok(String::new());
    }
    fetch_story_opening(stage, story_ctx, story_store).await
}

pub(in crate::pipeline) async fn load_continuation_opening(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    story_store: &dyn StoryStore,
) -> Result<String, PipelineError> {
    if story_summary(story_ctx).is_some() || !recent_story(story_ctx).is_empty() {
        return Ok(String::new());
    }
    fetch_story_opening(stage, story_ctx, story_store).await
}

async fn fetch_story_opening(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    story_store: &dyn StoryStore,
) -> Result<String, PipelineError> {
    let story_pack = story_store
        .get_pack(&story_ctx.pack_ref.pack_id)
        .await
        .map_err(|error| PipelineError::new(stage, error.to_string()))?;
    Ok(story_pack.Opening)
}

#[cfg(test)]
#[path = "tests/story_context_prompt_tests.rs"]
mod tests;
