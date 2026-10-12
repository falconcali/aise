use super::error::PersistenceError;
use crate::core::{CharacterCard, CharacterCardRef, PackId, PackRef, StoryPack};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait PackStore: Send + Sync {
    async fn import_pack(&self, pack_id: PackId, pack: StoryPack) -> Result<PackRef, PersistenceError>;
    async fn get_pack(&self, pack_id: &PackId) -> Result<Arc<StoryPack>, PersistenceError>;
    async fn import_character_card(&self, card: CharacterCard) -> Result<CharacterCardRef, PersistenceError>;
    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError>;
}
