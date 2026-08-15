use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::frozen_ref::FrozenCharacterCardRef;
use crate::domain::asset::ids::{PackId, Sha256Digest};
use crate::domain::asset::story_pack::StoryPack;
use crate::domain::asset::world_book::WorldBook;
use crate::persistence::asset_store::{
    AssetStore, CharacterCardInfo, FrozenCharacterCard, FrozenStoryPack, ValidatedCharacterCard, ValidatedStoryPack,
};
use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use async_trait::async_trait;
use std::str::FromStr;
use std::sync::Arc;

pub struct SqliteAssetStore {
    pool: Arc<sqlx::SqlitePool>,
}

impl SqliteAssetStore {
    pub fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
        Self { pool }
    }

    pub async fn connect(url: &str) -> Result<Arc<Self>, StoreError> {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str(url)
            .map_err(|_| StoreError::Unavailable)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(Arc::new(Self { pool: Arc::new(pool) }))
    }

    async fn store_pack_row(
        &self,
        pack_id: &PackId,
        digest: &Sha256Digest,
        pack: &StoryPack,
        manifest: &[u8],
        world_book: &WorldBook,
    ) -> Result<(), StoreError> {
        let pack_key = pack.meta.pack_key.as_str();
        let version = pack.meta.version.to_string();
        let pack_json = serde_json::to_vec(pack).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let world_json = serde_json::to_vec(world_book).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidWorldState,
        })?;
        let story_profile_json = serde_json::to_vec(&pack.story).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let role_definitions_json = serde_json::to_vec(&pack.roles).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let narrative_definition_json = serde_json::to_vec(&pack.narrative).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let topic_dictionary_json = serde_json::to_vec(&world_book.topics).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidWorldState,
        })?;
        sqlx::query(
            "INSERT INTO story_packs (pack_id, pack_key, version, digest, pack_json, manifest_json, \
             world_book_json, story_profile_json, role_definitions_json, \
             narrative_definition_json, topic_dictionary_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(pack_id.as_str())
        .bind(pack_key)
        .bind(version)
        .bind(digest.to_string())
        .bind(pack_json)
        .bind(manifest)
        .bind(world_json)
        .bind(story_profile_json)
        .bind(role_definitions_json)
        .bind(narrative_definition_json)
        .bind(topic_dictionary_json)
        .execute(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        Ok(())
    }

    async fn load_pack_row(
        &self,
        selector: PackSelector<'_>,
    ) -> Result<Option<(String, Vec<u8>, String, Vec<u8>)>, StoreError> {
        let row = match selector {
            PackSelector::PackId(pack_id) => {
                sqlx::query_as::<_, (String, Vec<u8>, String, Vec<u8>)>(
                    "SELECT pack_id, pack_json, digest, world_book_json FROM story_packs WHERE pack_id = ?1",
                )
                .bind(pack_id.as_str())
                .fetch_optional(&*self.pool)
                .await
            }
            PackSelector::Digest(digest) => {
                sqlx::query_as::<_, (String, Vec<u8>, String, Vec<u8>)>(
                    "SELECT pack_id, pack_json, digest, world_book_json FROM story_packs WHERE digest = ?1",
                )
                .bind(digest.to_string())
                .fetch_optional(&*self.pool)
                .await
            }
        }
        .map_err(sqlx_error_to_store)?;
        Ok(row)
    }

    fn hydrate_pack(row: (String, Vec<u8>, String, Vec<u8>)) -> Result<FrozenStoryPack, StoreError> {
        let (pack_id, pack_json, digest, world_json) = row;
        let pack: StoryPack = serde_json::from_slice(&pack_json).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let resolved_world_book = serde_json::from_slice(&world_json).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidWorldState,
        })?;
        Ok(FrozenStoryPack {
            pack_id: PackId::from(pack_id),
            pack,
            digest: Sha256Digest::try_new(&digest).map_err(|_| StoreError::Serialization {
                kind: StoreSerializationErrorKind::InvalidStoryState,
            })?,
            resolved_world_book,
        })
    }
}

enum PackSelector<'a> {
    PackId(&'a PackId),
    Digest(&'a Sha256Digest),
}

#[async_trait]
impl AssetStore for SqliteAssetStore {
    async fn find_pack_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenStoryPack>, StoreError> {
        match self.load_pack_row(PackSelector::Digest(digest)).await? {
            Some(row) => Ok(Some(Self::hydrate_pack(row)?)),
            None => Ok(None),
        }
    }

    async fn import_pack(&self, validated: ValidatedStoryPack) -> Result<FrozenStoryPack, StoreError> {
        if let Some(existing) = self.find_pack_by_digest(&validated.digest).await? {
            return Ok(existing);
        }
        let existing_key = sqlx::query_as::<_, (String, String)>(
            "SELECT pack_id, digest FROM story_packs WHERE pack_key = ?1 AND version = ?2",
        )
        .bind(validated.pack.meta.pack_key.as_str())
        .bind(validated.pack.meta.version.to_string())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        if let Some((_, digest)) = existing_key {
            if digest != validated.digest.to_string() {
                return Err(StoreError::ConstraintViolation {
                    constraint: "duplicate_pack_key_version_with_different_digest".into(),
                });
            }
        }
        let pack_id = PackId::from(format!("pack-{}", uuid::Uuid::new_v4()));
        self.store_pack_row(
            &pack_id,
            &validated.digest,
            &validated.pack,
            &validated.canonical_manifest,
            &validated.resolved_world_book,
        )
        .await?;
        Ok(FrozenStoryPack {
            pack_id,
            pack: validated.pack,
            digest: validated.digest,
            resolved_world_book: validated.resolved_world_book,
        })
    }

    async fn load_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError> {
        match self.load_pack_row(PackSelector::PackId(pack_id)).await? {
            Some(row) => Self::hydrate_pack(row),
            None => Err(StoreError::NotFound),
        }
    }

    async fn export_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError> {
        self.load_pack(pack_id).await
    }

    async fn list_packs(&self) -> Result<Vec<FrozenStoryPack>, StoreError> {
        let rows = sqlx::query_as::<_, (String, Vec<u8>, String, Vec<u8>)>(
            "SELECT pack_id, pack_json, digest, world_book_json FROM story_packs ORDER BY created_at DESC",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        rows.into_iter().map(Self::hydrate_pack).collect()
    }

    async fn delete_pack(&self, pack_id: &PackId) -> Result<bool, StoreError> {
        let deleted = sqlx::query("DELETE FROM story_packs WHERE pack_id = ?1")
            .bind(pack_id.as_str())
            .execute(&*self.pool)
            .await
            .map_err(sqlx_error_to_store)?
            .rows_affected();
        Ok(deleted > 0)
    }

    async fn find_character_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenCharacterCard>, StoreError> {
        let row = sqlx::query_as::<_, (Vec<u8>, String)>(
            "SELECT canonical_json, digest FROM character_cards WHERE digest = ?1",
        )
        .bind(digest.to_string())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        match row {
            Some((canonical_json, digest)) => Ok(Some(hydrate_character(canonical_json, digest)?)),
            None => Ok(None),
        }
    }

    async fn import_character(&self, validated: ValidatedCharacterCard) -> Result<FrozenCharacterCard, StoreError> {
        if let Some(existing) = self.find_character_by_digest(&validated.digest).await? {
            return Ok(existing);
        }
        let existing_version = sqlx::query_as::<_, (String,)>(
            "SELECT digest FROM character_cards WHERE character_id = ?1 AND version = ?2",
        )
        .bind(validated.card.character_id.as_str())
        .bind(validated.card.meta.version.to_string())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        if let Some((digest,)) = existing_version {
            if digest != validated.digest.to_string() {
                return Err(StoreError::ConstraintViolation {
                    constraint: "character_version_digest_conflict".into(),
                });
            }
        }
        let card_json = serde_json::to_vec(&validated.card).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidCharacterState,
        })?;
        sqlx::query(
            "INSERT INTO character_cards (character_id, version, digest, card_json, canonical_json) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(validated.card.character_id.as_str())
        .bind(validated.card.meta.version.to_string())
        .bind(validated.digest.to_string())
        .bind(card_json)
        .bind(validated.canonical_json.clone())
        .execute(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        Ok(FrozenCharacterCard {
            card: validated.card,
            digest: validated.digest,
        })
    }

    async fn load_character(&self, reference: &FrozenCharacterCardRef) -> Result<FrozenCharacterCard, StoreError> {
        let row = sqlx::query_as::<_, (Vec<u8>,)>(
            "SELECT canonical_json FROM character_cards WHERE character_id = ?1 AND version = ?2 AND digest = ?3",
        )
        .bind(reference.character_id.as_str())
        .bind(reference.version.to_string())
        .bind(reference.digest.to_string())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        match row {
            Some((canonical_json,)) => hydrate_character(canonical_json, reference.digest.to_string()),
            None => Err(StoreError::NotFound),
        }
    }

    async fn list_characters(&self) -> Result<Vec<CharacterCardInfo>, StoreError> {
        let rows = sqlx::query_as::<_, (Vec<u8>, String)>(
            "SELECT canonical_json, digest FROM character_cards ORDER BY created_at ASC",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        rows.into_iter()
            .map(|(canonical_json, digest)| {
                let frozen = hydrate_character(canonical_json, digest)?;
                Ok(CharacterCardInfo {
                    character_id: frozen.card.character_id,
                    name: frozen.card.profile.name,
                    creator: frozen.card.meta.creator,
                    version: frozen.card.meta.version,
                    digest: frozen.digest,
                })
            })
            .collect()
    }
}

fn hydrate_character(canonical_json: Vec<u8>, digest: String) -> Result<FrozenCharacterCard, StoreError> {
    let card: CharacterCard = serde_json::from_slice(&canonical_json).map_err(|_| StoreError::Serialization {
        kind: StoreSerializationErrorKind::InvalidCharacterState,
    })?;
    let digest = Sha256Digest::try_new(&digest).map_err(|_| StoreError::Serialization {
        kind: StoreSerializationErrorKind::InvalidCharacterState,
    })?;
    Ok(FrozenCharacterCard { card, digest })
}

fn sqlx_error_to_store(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::RowNotFound => StoreError::NotFound,
        sqlx::Error::Database(database_error) => StoreError::ConstraintViolation {
            constraint: database_error.message().to_string(),
        },
        _ => StoreError::Unavailable,
    }
}
