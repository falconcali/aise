use crate::core::{BoundedText, NarrativeDirection, NarrativeError, NarrativePlan, StoryContext, StoryPack};
use crate::pipeline::baseline::baseline_trace;
use crate::trace::Observation;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NarrativeProjection {
    pub directions: Vec<NarrativeDirection>,
    pub constraints: Vec<BoundedText>,
}

pub fn project_narrative(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    observation: &Observation,
) -> Result<NarrativeProjection, NarrativeError> {
    let narrative_observation = baseline_trace::begin_narrative_projection(observation, story_ctx);
    let result = build_projection(story_pack, story_ctx);
    baseline_trace::finish_narrative_projection(narrative_observation, &result);
    result
}

fn build_projection(story_pack: &StoryPack, story_ctx: &StoryContext) -> Result<NarrativeProjection, NarrativeError> {
    let plan = match &story_pack.narrative {
        Some(graph) => graph.plan(&story_ctx.narrative)?,
        None => NarrativePlan::default(),
    };
    let constraints = story_pack
        .constraints
        .iter()
        .cloned()
        .chain(plan.constraints.into_iter().map(|constraint| constraint.forbid))
        .collect();
    Ok(NarrativeProjection {
        directions: plan.directions,
        constraints,
    })
}

#[cfg(test)]
#[path = "tests/baseline_narrative_tests.rs"]
mod tests;
