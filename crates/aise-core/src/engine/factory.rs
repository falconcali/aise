use crate::core::{CharacterCardRef, PackRef, PlayerId, RoleId, StoryId, StoryInstanceInfo, StoryInstanceSpec};
use crate::engine::EngineError;
use crate::persistence::StoryStore;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoryCreationSpec {
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, CharacterCardRef>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

pub struct StoryFactory {
    story_store: Arc<dyn StoryStore>,
}

impl StoryFactory {
    pub fn new(story_store: Arc<dyn StoryStore>) -> Self {
        Self { story_store }
    }

    pub async fn create_story(&self, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError> {
        let story_instance_spec = StoryInstanceSpec {
            story_id: self.generate_story_id(),
            pack_ref: spec.pack_ref,
            cast: spec.cast,
            player_id: spec.player_id,
            player_role: spec.player_role,
        };

        self.story_store.create(story_instance_spec).await.map_err(EngineError::from)
    }

    pub async fn remove_story(&self, story_id: &StoryId) -> Result<(), EngineError> {
        self.story_store.remove(story_id).await.map_err(EngineError::from)
    }

    fn generate_story_id(&self) -> StoryId {
        StoryId::try_new(format!("story-{}", Uuid::new_v4())).expect("generated story id must be valid")
    }
}
