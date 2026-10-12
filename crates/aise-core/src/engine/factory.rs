use crate::core::{
    AssetConfig, CharacterCard, CharacterCardRef, NarrativeGraph, PackId, PackRef, PlayerId, RoleActor, RoleId,
    StoryId, StoryInstanceInfo, StoryInstanceSpec, StoryPack,
};
use crate::engine::EngineError;
use crate::llm::{LlmConfig, LlmGateway, LlmProviderType, OpenAiCompatProvider};
use crate::persistence::{PersistanceConfig, Store};
use crate::prompt::{Prompt, PromptConfig, PromptError};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoryCreationSpec {
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, RoleActor>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

pub struct AiseFactory;

impl AiseFactory {
    pub async fn import_story_pack(
        &self,
        store: Arc<Store>,
        asset_config: &AssetConfig,
        json: &str,
    ) -> Result<PackRef, EngineError> {
        let story_pack = StoryPack::from_json(json, asset_config)?;
        store
            .pack_store()
            .import_pack(self.generate_pack_id(), story_pack)
            .await
            .map_err(EngineError::from)
    }

    pub async fn import_character_card(
        &self,
        store: Arc<Store>,
        asset_config: &AssetConfig,
        json: &str,
    ) -> Result<CharacterCardRef, EngineError> {
        let card = CharacterCard::from_json(json, asset_config)?;
        store.pack_store().import_character_card(card).await.map_err(EngineError::from)
    }

    pub async fn create_story(
        &self,
        store: Arc<Store>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError> {
        let story_pack = store.pack_store().get_pack(&spec.pack_ref.pack_id).await?;
        let cast = story_pack.resolve_cast(&spec.player_role, spec.cast)?;
        self.ensure_cast_cards(&store, &cast).await?;
        let narrative = story_pack
            .narrative
            .as_ref()
            .map(NarrativeGraph::initial_state)
            .unwrap_or_default();
        let story_instance_spec = StoryInstanceSpec {
            story_id: self.generate_story_id(),
            pack_ref: spec.pack_ref,
            cast,
            player_id: spec.player_id,
            player_role: spec.player_role,
            narrative,
        };

        store.story_store().create(story_instance_spec).await.map_err(EngineError::from)
    }

    pub async fn remove_story(&self, store: Arc<Store>, story_id: &StoryId) -> Result<(), EngineError> {
        store.story_store().remove(story_id).await.map_err(EngineError::from)
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

    async fn ensure_cast_cards(&self, store: &Store, cast: &BTreeMap<RoleId, RoleActor>) -> Result<(), EngineError> {
        for actor in cast.values() {
            if let RoleActor::Card(card_ref) = actor {
                store.pack_store().get_character_card(card_ref).await?;
            }
        }
        Ok(())
    }

    fn generate_story_id(&self) -> StoryId {
        StoryId::try_new(format!("story-{}", Uuid::new_v4())).expect("generated story id must be valid")
    }

    fn generate_pack_id(&self) -> PackId {
        PackId::try_new(format!("pack-{}", Uuid::new_v4())).expect("generated pack id must be valid")
    }
}

#[cfg(test)]
#[path = "tests/factory_tests.rs"]
mod tests;
