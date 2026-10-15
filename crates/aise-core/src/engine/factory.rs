use crate::core::{
    CharacterCard, CharacterCardRef, NarrativeGraph, PlayerId, RoleActor, RoleId, StoryAssetSpec, StoryId,
    StoryInstanceInfo, StoryInstanceSpec, StoryPack,
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

struct FrozenStory {
    assets: StoryAssetSpec,
    instance: StoryInstanceSpec,
}

struct FrozenCast {
    actors: BTreeMap<RoleId, RoleActor>,
    cards: Vec<(CharacterCardRef, CharacterCard)>,
}

pub struct AiseFactory;

impl AiseFactory {
    pub async fn create_story(
        &self,
        store: Arc<Store>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError> {
        let story = self.freeze_story(self.generate_story_id(), spec)?;
        let story_id = story.instance.story_id.clone();
        store.pack_store().retain(&story_id, story.assets).await?;
        match store.story_store().create(story.instance).await {
            Ok(info) => Ok(info),
            Err(error) => {
                self.release_story_assets(&store, &story_id).await;
                Err(EngineError::from(error))
            }
        }
    }

    pub async fn remove_story(&self, store: Arc<Store>, story_id: &StoryId) -> Result<(), EngineError> {
        store.story_store().remove(story_id).await?;
        store.pack_store().release(story_id).await.map_err(EngineError::from)
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

    fn freeze_story(&self, story_id: StoryId, spec: StoryCreationSpec) -> Result<FrozenStory, EngineError> {
        // WAIVER: R-ARCH-03
        // WAIVER: R-ARCH-04
        spec.story_pack.validate_play()?;
        spec.story_pack.validate_narrative()?;
        spec.story_pack.validate_roles()?;
        let frozen_cast = self.freeze_cast(spec.cast)?;
        let cast = spec.story_pack.resolve_cast(&spec.player_role, frozen_cast.actors)?;
        let pack_ref = spec.story_pack.pack_ref()?;
        let narrative = spec
            .story_pack
            .narrative
            .as_ref()
            .map(NarrativeGraph::initial_state)
            .unwrap_or_default();
        Ok(FrozenStory {
            instance: StoryInstanceSpec {
                story_id,
                pack_ref: pack_ref.clone(),
                cast,
                player_id: spec.player_id,
                player_role: spec.player_role,
                narrative,
            },
            assets: StoryAssetSpec {
                pack_ref,
                pack: spec.story_pack,
                cards: frozen_cast.cards,
            },
        })
    }

    fn freeze_cast(&self, cast: BTreeMap<RoleId, RoleActorSpec>) -> Result<FrozenCast, EngineError> {
        let mut frozen_cast = FrozenCast {
            actors: BTreeMap::new(),
            cards: Vec::new(),
        };
        for (role_id, actor) in cast {
            let actor = match actor {
                RoleActorSpec::Default => RoleActor::Default,
                RoleActorSpec::Card(card) => {
                    let card_ref = card.card_ref()?;
                    frozen_cast.cards.push((card_ref.clone(), *card));
                    RoleActor::Card(card_ref)
                }
            };
            frozen_cast.actors.insert(role_id, actor);
        }
        Ok(frozen_cast)
    }

    async fn release_story_assets(&self, store: &Store, story_id: &StoryId) {
        if let Err(error) = store.pack_store().release(story_id).await {
            tracing::error!(
                target: "aise::engine",
                story_id = %story_id,
                error = %error,
                "story asset release failed"
            );
        }
    }

    fn generate_story_id(&self) -> StoryId {
        StoryId::try_new(format!("story-{}", Uuid::new_v4())).expect("generated story id must be valid")
    }
}

#[cfg(test)]
#[path = "tests/factory_tests.rs"]
mod tests;
