use crate::core::{
    BoundedText, NarrativeDirection, NarrativeError, NarrativePlan, SemanticQuery, StoryContext, StoryPack,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NarrativeProjection {
    pub directions: Vec<NarrativeDirection>,
    pub constraints: Vec<BoundedText>,
    pub queries: Vec<SemanticQuery>,
}

pub fn project_narrative(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
) -> Result<NarrativeProjection, NarrativeError> {
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
        queries: plan.queries,
    })
}

#[cfg(test)]
#[path = "tests/baseline_narrative_tests.rs"]
mod tests;
