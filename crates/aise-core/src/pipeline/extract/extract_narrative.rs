use crate::core::{
    Change, NarrativeConditionId, NarrativeEvidence, NarrativeState, StoryContext, StoryLifeCycle, StoryPack, Truth,
};
use std::collections::BTreeMap;

pub(super) struct NarrativeChange {
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

pub(super) fn resolve_narrative(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    semantic_results: &BTreeMap<NarrativeConditionId, Truth>,
) -> NarrativeChange {
    let Some(graph) = &story_pack.narrative else {
        return NarrativeChange {
            narrative: Change::Unchanged,
            life_cycle: Change::Unchanged,
        };
    };
    let evidence = NarrativeEvidence {
        turn: story_ctx.turn_number.increment(),
        semantic_results,
    };
    let resolution = graph.resolve(&story_ctx.narrative, &evidence);

    let narrative = if resolution.transitions.is_empty() {
        Change::Unchanged
    } else {
        let mut next = story_ctx.narrative.clone();
        next.apply(&resolution.transitions);
        Change::Replaced(next)
    };
    let life_cycle = if resolution.story_completed {
        Change::Replaced(StoryLifeCycle::Completed)
    } else {
        Change::Unchanged
    };
    NarrativeChange { narrative, life_cycle }
}

#[cfg(test)]
#[path = "tests/extract_narrative_tests.rs"]
mod tests;
