use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    IdempotencyKey, NarrativeState, PackRef, PlayerId, RoleId, StoryCommit, StoryContext, StoryCreateBundle, StoryId,
    StoryInstanceInfo, StoryInstanceSpec, StoryPack, StoryPackSnapshot,
};
use crate::persistence::PersistenceError;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FailingStoryStore;

#[async_trait]
impl StoryStore for FailingStoryStore {
    async fn create(&self, _spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError> {
        Err(PersistenceError::ConstraintViolation {
            constraint: "create failed".to_owned(),
        })
    }

    async fn remove(&self, _story_id: &StoryId) -> Result<(), PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn get_info(&self, _story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn load(&self, _story_id: &StoryId) -> Result<StoryContext, PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn find_committed(
        &self,
        _story_id: &StoryId,
        _idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError> {
        Ok(None)
    }

    async fn commit(&self, _commit: StoryCommit) -> Result<StoryCommit, PersistenceError> {
        Err(PersistenceError::Unavailable {
            message: "commit unavailable".to_owned(),
        })
    }
}

struct ControlledReleasePackStore {
    release_failures: usize,
    release_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl PackStore for ControlledReleasePackStore {
    async fn retain(&self, _story_id: &StoryId, _snapshot: StoryPackSnapshot) -> Result<(), PersistenceError> {
        Ok(())
    }

    async fn release(&self, _story_id: &StoryId) -> Result<(), PersistenceError> {
        let call = self.release_calls.fetch_add(1, Ordering::SeqCst);
        if call < self.release_failures {
            return Err(PersistenceError::Unavailable {
                message: "release unavailable".to_owned(),
            });
        }
        Ok(())
    }

    async fn get_pack(&self, _pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        Err(PersistenceError::NotFound)
    }
}

fn story_id() -> StoryId {
    StoryId::try_new("story").expect("test story id should be valid")
}

fn story_snapshot() -> StoryPackSnapshot {
    StoryPackSnapshot::from_pack(snake_pack()).expect("pack snapshot should build")
}

fn story_bundle() -> StoryCreateBundle {
    let pack = snake_pack();
    StoryCreateBundle::assemble(
        StoryPackSnapshot::from_pack(pack.clone()).expect("pack snapshot should build"),
        story_id(),
        Arc::new(pack.opening),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        PlayerId::try_new("player").expect("test player id should be valid"),
        RoleId::try_new("xu_xian").expect("test role id should be valid"),
        NarrativeState::default(),
    )
}

#[tokio::test]
async fn store_exposes_pack_store() {
    let store = Store::default();
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");

    store
        .pack_store()
        .retain(&story_id(), story_snapshot())
        .await
        .expect("assets should be retained");
    let pack = store.pack_store().get_pack(&pack_ref).await.expect("pack should load");
    assert!(!pack.opening.as_str().is_empty());
}

#[tokio::test]
async fn store_exposes_story_store() {
    let store = Store::default();

    let error = store
        .story_store()
        .load(&story_id())
        .await
        .expect_err("missing story should fail");
    assert!(matches!(error, PersistenceError::NotFound));
}

#[tokio::test]
async fn store_builds_sqlite_placeholders_from_config() {
    let store = Store::new(PersistanceConfig {
        story_store: StoryStoreConfig {
            store_type: StoreType::Sqlite,
            ..StoryStoreConfig::default()
        },
        pack_store: PackStoreConfig {
            store_type: StoreType::Sqlite,
            ..PackStoreConfig::default()
        },
    });
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");

    store
        .pack_store()
        .retain(&story_id(), story_snapshot())
        .await
        .expect("assets should be retained");
    store.pack_store().get_pack(&pack_ref).await.expect("pack should load");
}

#[test]
fn pack_store_config_rejects_removed_character_card_capacity() {
    let config = r#"{"store_type":"Memory","max_packs":1,"max_character_cards":1}"#;

    assert!(serde_json::from_str::<PackStoreConfig>(config).is_err());
}

#[tokio::test]
async fn removing_an_unknown_story_is_idempotent() {
    let store = Store::default();

    store
        .remove_story(&story_id())
        .await
        .expect("first missing-story removal should succeed");
    store
        .remove_story(&story_id())
        .await
        .expect("repeated missing-story removal should succeed");
}

#[tokio::test]
async fn create_failure_releases_the_retained_pack_and_returns_original_error() {
    let store = Store::default();
    let (unused_snapshot, instance) = story_bundle().into_parts();
    drop(unused_snapshot);
    store
        .story_store()
        .create(instance)
        .await
        .expect("preexisting story should be created");
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");

    let error = store
        .create_story(story_bundle())
        .await
        .expect_err("duplicate story should fail");

    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
    assert!(matches!(
        store.pack_store().get_pack(&pack_ref).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn create_and_cleanup_failures_preserve_both_sources_and_story_id() {
    let release_calls = Arc::new(AtomicUsize::new(0));
    let store = Store {
        story_store: Box::new(FailingStoryStore),
        pack_store: Box::new(ControlledReleasePackStore {
            release_failures: 1,
            release_calls: Arc::clone(&release_calls),
        }),
    };

    let error = store
        .create_story(story_bundle())
        .await
        .expect_err("create and cleanup should fail");

    assert!(matches!(
        error,
        PersistenceError::CreateCompensationFailed {
            story_id: failed_story_id,
            create_source,
            cleanup_source,
        } if failed_story_id == story_id()
            && matches!(*create_source, PersistenceError::ConstraintViolation { .. })
            && matches!(*cleanup_source, PersistenceError::Unavailable { .. })
    ));
    assert_eq!(release_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn repeated_remove_retries_a_failed_pack_release() {
    let release_calls = Arc::new(AtomicUsize::new(0));
    let store = Store {
        story_store: Box::new(FailingStoryStore),
        pack_store: Box::new(ControlledReleasePackStore {
            release_failures: 1,
            release_calls: Arc::clone(&release_calls),
        }),
    };

    assert!(matches!(
        store.remove_story(&story_id()).await,
        Err(PersistenceError::Unavailable { .. })
    ));
    store
        .remove_story(&story_id())
        .await
        .expect("repeated removal should retry release");
    assert_eq!(release_calls.load(Ordering::SeqCst), 2);
}
