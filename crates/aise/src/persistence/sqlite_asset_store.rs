use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::ids::{CharacterAssetKey, PackId, Sha256Digest};
use crate::domain::asset::story_pack::StoryPack;
use crate::domain::asset::world_book::WorldBook;
use crate::persistence::asset_store::{AssetStore, FrozenStoryPack, ValidatedStoryPack};
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
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

    #[allow(dead_code)]
    fn pack_digest(pack: &StoryPack) -> Sha256Digest {
        let mut hasher = Sha256::new();
        if let Ok(json) = serde_json::to_vec(pack) {
            hasher.update(&json);
        }
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        Sha256Digest::from_bytes(out)
    }

    async fn store_pack_row(
        &self,
        pack_id: &PackId,
        digest: &Sha256Digest,
        pack: &StoryPack,
        manifest: &[u8],
        characters: &BTreeMap<CharacterAssetKey, CharacterCard>,
        world_book: &WorldBook,
    ) -> Result<(), StoreError> {
        let pack_key = pack.meta.pack_key.as_str();
        let version = pack.meta.version.to_string();
        let pack_json = serde_json::to_vec(pack).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let characters_json = serde_json::to_vec(characters).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
        })?;
        let world_json = serde_json::to_vec(world_book).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
        })?;
        sqlx::query(
            "INSERT INTO story_packs (pack_id, pack_key, version, digest, pack_json, manifest_json, characters_json, world_book_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(pack_id.as_str())
        .bind(pack_key)
        .bind(version)
        .bind(digest.to_string())
        .bind(pack_json)
        .bind(manifest)
        .bind(characters_json)
        .bind(world_json)
        .execute(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        Ok(())
    }
}

#[async_trait]
impl AssetStore for SqliteAssetStore {
    async fn find_pack_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenStoryPack>, StoreError> {
        let row = sqlx::query_as::<_, (String, Vec<u8>, String, Vec<u8>, Vec<u8>)>(
            "SELECT pack_id, pack_json, digest, characters_json, world_book_json FROM story_packs WHERE digest = ?1",
        )
        .bind(digest.to_string())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        match row {
            Some((pack_id, pack_json, _, characters_json, world_json)) => {
                let pack: StoryPack = serde_json::from_slice(&pack_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?;
                let resolved_characters =
                    serde_json::from_slice(&characters_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
                    })?;
                let resolved_world_book =
                    serde_json::from_slice(&world_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
                    })?;
                Ok(Some(FrozenStoryPack {
                    pack_id: PackId::from(pack_id),
                    pack,
                    digest: digest.clone(),
                    resolved_characters,
                    resolved_world_book,
                }))
            }
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
            &validated.resolved_characters,
            &validated.resolved_world_book,
        )
        .await?;
        Ok(FrozenStoryPack {
            pack_id,
            pack: validated.pack,
            digest: validated.digest,
            resolved_characters: validated.resolved_characters,
            resolved_world_book: validated.resolved_world_book,
        })
    }

    async fn load_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError> {
        let row = sqlx::query_as::<_, (Vec<u8>, String, Vec<u8>, Vec<u8>)>(
            "SELECT pack_json, digest, characters_json, world_book_json FROM story_packs WHERE pack_id = ?1",
        )
        .bind(pack_id.as_str())
        .fetch_optional(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        match row {
            Some((pack_json, digest, characters_json, world_json)) => {
                let pack: StoryPack = serde_json::from_slice(&pack_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?;
                let resolved_characters =
                    serde_json::from_slice(&characters_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
                    })?;
                let resolved_world_book =
                    serde_json::from_slice(&world_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
                    })?;
                Ok(FrozenStoryPack {
                    pack_id: pack_id.clone(),
                    pack,
                    digest: Sha256Digest::try_new(&digest).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                    })?,
                    resolved_characters,
                    resolved_world_book,
                })
            }
            None => Err(StoreError::NotFound),
        }
    }

    async fn export_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError> {
        self.load_pack(pack_id).await
    }

    async fn list_packs(&self) -> Result<Vec<FrozenStoryPack>, StoreError> {
        let rows = sqlx::query_as::<_, (String, Vec<u8>, String, Vec<u8>, Vec<u8>)>(
            "SELECT pack_id, pack_json, digest, characters_json, world_book_json FROM story_packs ORDER BY created_at DESC",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(sqlx_error_to_store)?;
        rows.into_iter()
            .map(|(pack_id, pack_json, digest, characters_json, world_json)| {
                let pack: StoryPack = serde_json::from_slice(&pack_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?;
                let resolved_characters =
                    serde_json::from_slice(&characters_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
                    })?;
                let resolved_world_book =
                    serde_json::from_slice(&world_json).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
                    })?;
                Ok(FrozenStoryPack {
                    pack_id: PackId::from(pack_id),
                    pack,
                    digest: Sha256Digest::try_new(&digest).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                    })?,
                    resolved_characters,
                    resolved_world_book,
                })
            })
            .collect()
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

#[allow(dead_code)]
pub(crate) fn _sqlite_asset_anchor(_: &dyn AssetStore, _: &FrozenStoryPack) {}
