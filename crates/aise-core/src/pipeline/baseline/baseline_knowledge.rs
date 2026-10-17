use crate::core::{
    ActivatedKnowledge, ActivationCandidate, ActivationConfig, ActivationOutcome, ActivationTurn, FactId,
    PlayerContribution, Recallable, RumorId, ScanBuffer, ScanFragment, ScanFragmentKind, StoryContext, activate,
};
use crate::pipeline::baseline::baseline_narrative::NarrativeProjection;
use crate::pipeline::baseline::baseline_trace;
use crate::trace::Observation;
use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum KnowledgeKey {
    Fact(FactId),
    Rumor(RumorId),
}

impl Display for KnowledgeKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fact(id) => write!(f, "fact:{id}"),
            Self::Rumor(id) => write!(f, "rumor:{id}"),
        }
    }
}

pub struct KnowledgeScope<'a> {
    pub story_ctx: &'a StoryContext,
    pub player_contribution: &'a PlayerContribution,
    pub narrative: &'a NarrativeProjection,
}

pub fn activate_knowledge(
    scope: KnowledgeScope<'_>,
    config: &ActivationConfig,
    observation: &Observation,
) -> ActivatedKnowledge {
    let knowledge_observation = baseline_trace::begin_knowledge_activation(observation, scope.player_contribution);
    let outcome = run_activation(&scope, config);
    baseline_trace::finish_knowledge_activation(knowledge_observation, &outcome);
    collect_knowledge(scope.story_ctx, outcome)
}

fn run_activation(scope: &KnowledgeScope<'_>, config: &ActivationConfig) -> ActivationOutcome<KnowledgeKey> {
    let story_ctx = scope.story_ctx;
    let candidates = story_ctx
        .facts
        .iter()
        .map(|(id, entry)| candidate(KnowledgeKey::Fact(id.clone()), &entry.body))
        .chain(
            story_ctx
                .rumors
                .iter()
                .map(|(id, entry)| candidate(KnowledgeKey::Rumor(id.clone()), &entry.body)),
        )
        .collect();
    let turn = ActivationTurn {
        story_id: &scope.story_ctx.story_id,
        turn_number: &scope.story_ctx.turn_number,
    };
    activate(candidates, &build_scan_buffer(scope), turn, config)
}

fn candidate<'a>(key: KnowledgeKey, body: &'a impl Recallable) -> ActivationCandidate<'a, KnowledgeKey> {
    ActivationCandidate {
        key,
        rule: body.activation(),
        content: body.content(),
    }
}

fn build_scan_buffer(scope: &KnowledgeScope<'_>) -> ScanBuffer {
    let story_ctx = scope.story_ctx;
    let player_role_name = story_ctx
        .roles
        .get(&story_ctx.player)
        .map(|role| ScanFragment::new(ScanFragmentKind::PlayerRoleName, 0, role.spec.name.as_str()));
    let directions =
        scope.narrative.directions.iter().map(|direction| {
            ScanFragment::new(ScanFragmentKind::NarrativeDirection, 0, direction.dramatic_focus.as_str())
        });
    let recent_story = story_ctx.recent_turns.iter().rev().enumerate().map(|(order, turn)| {
        ScanFragment::new(
            ScanFragmentKind::RecentStory,
            depth(order.saturating_add(1)),
            turn.turn_segment.text(),
        )
    });
    let prior_story_depth = depth(story_ctx.recent_turns.len().saturating_add(1));
    let prior_story = story_ctx
        .summary
        .as_ref()
        .filter(|summary| !summary.text.trim().is_empty())
        .map_or_else(
            || ScanFragment::new(ScanFragmentKind::StoryOpening, prior_story_depth, story_ctx.opening.as_str()),
            |summary| ScanFragment::new(ScanFragmentKind::StorySummary, prior_story_depth, &summary.text),
        );
    ScanBuffer::new(
        std::iter::once(ScanFragment::new(
            ScanFragmentKind::PlayerContribution,
            0,
            &scope.player_contribution.processed,
        ))
        .chain(player_role_name)
        .chain(directions)
        .chain(recent_story)
        .chain(std::iter::once(prior_story)),
    )
}

fn collect_knowledge(story_ctx: &StoryContext, outcome: ActivationOutcome<KnowledgeKey>) -> ActivatedKnowledge {
    let mut knowledge = ActivatedKnowledge::default();
    for activated in outcome.activated {
        match activated.key {
            KnowledgeKey::Fact(id) => {
                if let Some(entry) = story_ctx.facts.get(&id) {
                    knowledge.facts.push(entry.clone());
                }
            }
            KnowledgeKey::Rumor(id) => {
                if let Some(entry) = story_ctx.rumors.get(&id) {
                    knowledge.rumors.push(entry.clone());
                }
            }
        }
    }
    knowledge
}

fn depth(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[path = "tests/baseline_knowledge_tests.rs"]
mod tests;
