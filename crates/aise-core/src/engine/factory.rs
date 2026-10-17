use crate::core::{
    CharacterCard, EntryOrigin, FactEntry, FactId, MAX_OPENING_BYTES, MemoryEntry, MemoryId, NarrativeGraph, PlayerId,
    RoleEntry, RoleId, RumorEntry, RumorId, StoryCreateBundle, StoryError, StoryId, StoryInstanceInfo,
    StoryPack, StoryPackSnapshot, validate_runtime_roles,
};
use crate::engine::EngineError;
use crate::llm::{LlmConfig, LlmGateway, LlmProviderType, OpenAiCompatProvider};
use crate::persistence::{PersistanceConfig, Store};
use crate::prompt::{Prompt, PromptConfig, PromptError};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoryCreationSpec {
    pub story_pack: StoryPack,
    pub cast: BTreeMap<RoleId, RoleActorSpec>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

pub enum RoleActorSpec {
    Default,
    Card(Box<CharacterCard>),
}

pub struct AiseFactory;

impl AiseFactory {
    pub async fn create_story(
        &self,
        store: Arc<Store>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError> {
        let bundle = self.freeze_story(self.generate_story_id(), spec)?;
        store.create_story(bundle).await.map_err(EngineError::from)
    }

    pub async fn remove_story(&self, store: Arc<Store>, story_id: &StoryId) -> Result<(), EngineError> {
        store.remove_story(story_id).await.map_err(EngineError::from)
    }

    pub fn create_llm_gateway(&self, llm_config: LlmConfig) -> Arc<LlmGateway> {
        let llm_provider = match llm_config.provider {
            LlmProviderType::OpenAiCompat => Arc::new(OpenAiCompatProvider::new(&llm_config)),
            LlmProviderType::Other => Arc::new(OpenAiCompatProvider::new(&llm_config)),
        };

        Arc::new(LlmGateway::new(llm_provider, llm_config))
    }

    pub fn create_prompt(&self, prompt_config: PromptConfig) -> Result<Prompt, PromptError> {
        Prompt::new(prompt_config)
    }

    pub fn create_store(&self, persistence_config: PersistanceConfig) -> Arc<Store> {
        Arc::new(Store::new(persistence_config))
    }

    fn freeze_story(&self, story_id: StoryId, spec: StoryCreationSpec) -> Result<StoryCreateBundle, EngineError> {
        spec.story_pack.validate_play()?;
        spec.story_pack.validate_narrative()?;
        spec.story_pack.validate_roles()?;
        validate_cast(&spec.story_pack, &spec.player_role, &spec.cast)?;
        if spec.story_pack.opening.as_str().len() > MAX_OPENING_BYTES {
            return Err(StoryError::OpeningTooLarge {
                actual: spec.story_pack.opening.as_str().len(),
                maximum: MAX_OPENING_BYTES,
            }
            .into());
        }
        let roles = freeze_roles(&spec.story_pack, spec.cast);
        validate_runtime_roles(&roles)?;
        let facts = freeze_facts(&spec.story_pack);
        let rumors = freeze_rumors(&spec.story_pack);
        let opening = Arc::new(spec.story_pack.opening.clone());
        let narrative = spec
            .story_pack
            .narrative
            .as_ref()
            .map(NarrativeGraph::initial_state)
            .unwrap_or_default();
        let snapshot = StoryPackSnapshot::from_pack(spec.story_pack)?;
        Ok(StoryCreateBundle::assemble(
            snapshot,
            story_id,
            opening,
            roles,
            facts,
            rumors,
            spec.player_id,
            spec.player_role,
            narrative,
        ))
    }

    fn generate_story_id(&self) -> StoryId {
        StoryId::try_new(format!("story-{}", Uuid::new_v4())).expect("generated story id must be valid")
    }
}

fn validate_cast(
    story_pack: &StoryPack,
    player_role: &RoleId,
    cast: &BTreeMap<RoleId, RoleActorSpec>,
) -> Result<(), StoryError> {
    if !story_pack.play.playable_roles.contains(player_role) {
        return Err(StoryError::RoleNotPlayable {
            role_id: player_role.to_string(),
        });
    }
    if let Some(role_id) = cast.keys().find(|role_id| !story_pack.roles.contains_key(*role_id)) {
        return Err(StoryError::UnknownRole {
            field: "cast",
            role_id: role_id.to_string(),
        });
    }
    Ok(())
}

fn freeze_roles(story_pack: &StoryPack, mut cast: BTreeMap<RoleId, RoleActorSpec>) -> BTreeMap<RoleId, RoleEntry> {
    story_pack
        .roles
        .iter()
        .map(|(role_id, seed_role)| {
            let profile = match cast.remove(role_id) {
                Some(RoleActorSpec::Card(card)) => card.profile,
                Some(RoleActorSpec::Default) | None => seed_role.default_profile.clone(),
            };
            let memories = seed_role
                .seed_memories
                .iter()
                .map(|(id, memory_body)| {
                    (
                        id.clone(),
                        MemoryEntry {
                            id: id.clone(),
                            origin: EntryOrigin::Seed,
                            body: memory_body.clone(),
                        },
                    )
                })
                .collect::<BTreeMap<MemoryId, MemoryEntry>>();
            (
                role_id.clone(),
                RoleEntry {
                    id: role_id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: seed_role.spec.clone(),
                    profile,
                    memories,
                },
            )
        })
        .collect()
}

fn freeze_facts(story_pack: &StoryPack) -> BTreeMap<FactId, FactEntry> {
    // WAIVER: R-ARCH-04
    story_pack
        .seed_facts
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                FactEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect()
}

fn freeze_rumors(story_pack: &StoryPack) -> BTreeMap<RumorId, RumorEntry> {
    // WAIVER: R-ARCH-04
    story_pack
        .seed_rumors
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RumorEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/factory_tests.rs"]
mod tests;
