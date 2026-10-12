use super::error::PersistenceError;
use crate::core::{CharacterCard, CharacterCardRef, PackRef, StoryAssetSpec, StoryId, StoryPack};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait PackStore: Send + Sync {
    async fn retain(&self, story_id: &StoryId, assets: StoryAssetSpec) -> Result<(), PersistenceError>;
    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError>;
    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError>;
    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError>;
}
