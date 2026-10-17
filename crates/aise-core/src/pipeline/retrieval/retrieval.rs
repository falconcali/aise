use crate::core::{
    CharacterProfile, FactEntry, FactId, ResolvedRole, RoleActor, RoleBody, RoleId, RumorEntry, RumorId, StoryContext,
    TurnControl,
};
use crate::persistence::{PersistenceError, Store};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::retrieval::retrieval_trace;
use crate::trace::Observation;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Serialize)]
pub struct RetrievalInput<'a> {
    pub fact_ids: &'a [FactId],
    pub rumor_ids: &'a [RumorId],
    pub role_ids: &'a [RoleId],
}

#[derive(Default, Serialize)]
pub struct RetrievalOutput {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
    pub roles: Vec<ResolvedRole>,
}

pub struct RetrievalPipeline {
    store: Arc<Store>,
}

impl RetrievalPipeline {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    fn resolve_facts(&self, story_ctx: &StoryContext, fact_ids: &[FactId]) -> Result<Vec<FactEntry>, PipelineError> {
        fact_ids
            .iter()
            .map(|id| {
                story_ctx.facts.get(id).cloned().ok_or_else(|| {
                    PipelineError::inconsistent_data(
                        PipelineStage::Retrieval,
                        "retrieval_fact_missing",
                        format!("selected fact ID {id} is missing from the story context"),
                    )
                })
            })
            .collect()
    }

    fn resolve_rumors(
        &self,
        story_ctx: &StoryContext,
        rumor_ids: &[RumorId],
    ) -> Result<Vec<RumorEntry>, PipelineError> {
        rumor_ids
            .iter()
            .map(|id| {
                story_ctx.rumors.get(id).cloned().ok_or_else(|| {
                    PipelineError::inconsistent_data(
                        PipelineStage::Retrieval,
                        "retrieval_rumor_missing",
                        format!("selected rumor ID {id} is missing from the story context"),
                    )
                })
            })
            .collect()
    }

    fn resolve(&self, story_ctx: &StoryContext, input: &RetrievalInput<'_>) -> Result<RetrievalOutput, PipelineError> {
        Ok(RetrievalOutput {
            facts: self.resolve_facts(story_ctx, input.fact_ids)?,
            rumors: self.resolve_rumors(story_ctx, input.rumor_ids)?,
            roles: Vec::new(),
        })
    }

    async fn resolve_roles(
        &self,
        story_ctx: &StoryContext,
        role_ids: &[RoleId],
    ) -> Result<Vec<ResolvedRole>, PipelineError> {
        validate_unique_role_ids(role_ids)?;
        let mut roles = Vec::with_capacity(role_ids.len());
        for role_id in role_ids {
            roles.push(self.resolve_role(story_ctx, role_id).await?);
        }
        Ok(roles)
    }

    async fn resolve_role(&self, story_ctx: &StoryContext, role_id: &RoleId) -> Result<ResolvedRole, PipelineError> {
        let role_entry = story_ctx.roles.get(role_id).ok_or_else(|| {
            PipelineError::inconsistent_data(
                PipelineStage::Retrieval,
                "retrieval_role_missing",
                format!("requested role ID {role_id} is missing from the story context"),
            )
        })?;
        let profile = self.resolve_profile(role_id, &role_entry.body, &role_entry.actor).await?;
        Ok(ResolvedRole {
            role_id: role_id.clone(),
            name: role_entry.body.name.clone(),
            aliases: role_entry.body.alias.clone(),
            backstory: role_entry.body.backstory.clone(),
            retrieval_hint: role_entry.body.retrieval.clone(),
            profile,
            memories: role_entry.memories.clone(),
        })
    }

    async fn resolve_profile(
        &self,
        role_id: &RoleId,
        role_body: &RoleBody,
        actor: &RoleActor,
    ) -> Result<CharacterProfile, PipelineError> {
        match actor {
            RoleActor::Default => Ok(role_body.default_profile.clone()),
            RoleActor::Card(card_ref) => self
                .store
                .pack_store()
                .get_character_card(card_ref)
                .await
                .map(|card| card.profile.clone())
                .map_err(|error| map_card_error(role_id, error)),
        }
    }

    async fn retrieve(
        &self,
        input: &RetrievalInput<'_>,
        story_ctx: &StoryContext,
    ) -> Result<RetrievalOutput, PipelineError> {
        let mut output = self.resolve(story_ctx, input)?;
        output.roles = self.resolve_roles(story_ctx, input.role_ids).await?;
        Ok(output)
    }
}

fn validate_unique_role_ids(role_ids: &[RoleId]) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(role_ids.len());
    for role_id in role_ids {
        if !seen.insert(role_id) {
            return Err(PipelineError::inconsistent_data(
                PipelineStage::Retrieval,
                "retrieval_duplicate_role_id",
                format!("requested role ID {role_id} appears more than once"),
            ));
        }
    }
    Ok(())
}

fn map_card_error(role_id: &RoleId, error: PersistenceError) -> PipelineError {
    match error {
        PersistenceError::NotFound => PipelineError::inconsistent_data(
            PipelineStage::Retrieval,
            "retrieval_role_card_missing",
            format!("character card for requested role ID {role_id} is missing"),
        ),
        source => PipelineError::Persistence {
            stage: PipelineStage::Retrieval,
            source,
        },
    }
}

impl<'a> Pipeline<RetrievalInput<'a>> for RetrievalPipeline {
    type Output = RetrievalOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Retrieval
    }

    async fn execute(
        &self,
        input: RetrievalInput<'a>,
        story_ctx: &StoryContext,
        _control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let retrieval_observation = retrieval_trace::begin_retrieval(observation, &input);
        let result = self.retrieve(&input, story_ctx).await;
        retrieval_trace::finish_retrieval(retrieval_observation, &result);
        result
    }
}

#[cfg(test)]
#[path = "tests/retrieval_tests.rs"]
mod tests;
