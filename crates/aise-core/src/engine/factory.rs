use crate::core::{CharacterCardRef, PackRef, PlayerId, RoleId, StoryId, StoryInstanceInfo, StoryInstanceSpec};
use crate::engine::EngineError;
use crate::llm::{LlmConfig, LlmGateway, LlmProviderType, OpenAiCompatProvider};
use crate::persistence::{PersistanceConfig, StoryStore, StoryStoreMem, StoryStoreSqlite, StoryStoreType};
use crate::prompt::{Prompt, PromptConfig, PromptError};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoryCreationSpec {
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, CharacterCardRef>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

pub struct AiseFactory;

impl AiseFactory {
    pub async fn create_story(
        &self,
        story_store: Arc<dyn StoryStore>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError> {
        let story_instance_spec = StoryInstanceSpec {
            story_id: self.generate_story_id(),
            pack_ref: spec.pack_ref,
            cast: spec.cast,
            player_id: spec.player_id,
            player_role: spec.player_role,
        };

        story_store.create(story_instance_spec).await.map_err(EngineError::from)
    }

    pub async fn remove_story(&self, story_store: Arc<dyn StoryStore>, story_id: &StoryId) -> Result<(), EngineError> {
        story_store.remove(story_id).await.map_err(EngineError::from)
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

    pub fn create_story_store(&self, persistence_config: PersistanceConfig) -> Arc<dyn StoryStore> {
        match persistence_config.story_store.store_type {
            StoryStoreType::Memory => Arc::new(StoryStoreMem::with_config(persistence_config.story_store)),
            StoryStoreType::Sqlite => Arc::new(StoryStoreSqlite::new(persistence_config.story_store)),
        }
    }

    fn generate_story_id(&self) -> StoryId {
        StoryId::try_new(format!("story-{}", Uuid::new_v4())).expect("generated story id must be valid")
    }
}
