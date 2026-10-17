use super::*;
use crate::core::tests::{demo_pack, snake_pack};

fn story_id(value: &str) -> StoryId {
    StoryId::try_new(value).expect("test story id should be valid")
}

fn snapshot(pack: StoryPack) -> StoryPackSnapshot {
    StoryPackSnapshot::from_pack(pack).expect("pack snapshot should build")
}

fn store_with_capacity(max_packs: usize) -> PackStoreMem {
    PackStoreMem::with_config(PackStoreConfig {
        max_packs,
        ..PackStoreConfig::default()
    })
}

#[tokio::test]
async fn retained_pack_is_loadable_by_ref() {
    let store = PackStoreMem::new();
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");

    store
        .retain(&story_id("story"), snapshot(snake_pack()))
        .await
        .expect("pack should be retained");
    let loaded = store.get_pack(&pack_ref).await.expect("pack should load");

    assert_eq!(loaded.opening, snake_pack().opening);
}

#[tokio::test]
async fn unretained_pack_is_not_found() {
    let store = PackStoreMem::new();
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");

    let error = store.get_pack(&pack_ref).await.expect_err("missing pack should fail");
    assert!(matches!(error, PersistenceError::NotFound));
}

#[tokio::test]
async fn identical_pack_is_shared_until_last_story_releases_it() {
    let store = store_with_capacity(1);
    let pack_ref = snake_pack().pack_ref().expect("pack ref should compute");
    store
        .retain(&story_id("first"), snapshot(snake_pack()))
        .await
        .expect("first story should retain pack");
    store
        .retain(&story_id("second"), snapshot(snake_pack()))
        .await
        .expect("identical pack should share the snapshot");

    store.release(&story_id("first")).await.expect("release should succeed");
    store.get_pack(&pack_ref).await.expect("pack should stay while held");
    store.release(&story_id("second")).await.expect("release should succeed");

    assert!(matches!(store.get_pack(&pack_ref).await, Err(PersistenceError::NotFound)));
}

#[tokio::test]
async fn releasing_unknown_story_is_a_no_op() {
    let store = PackStoreMem::new();

    store
        .release(&story_id("missing"))
        .await
        .expect("release of unknown story should succeed");
}

#[tokio::test]
async fn pack_beyond_capacity_is_rejected() {
    let store = store_with_capacity(1);
    store
        .retain(&story_id("snake"), snapshot(snake_pack()))
        .await
        .expect("first pack should be retained");

    let error = store
        .retain(&story_id("demo"), snapshot(demo_pack()))
        .await
        .expect_err("pack beyond capacity should fail");
    assert!(matches!(
        error,
        PersistenceError::PackCapacityExceeded { actual: 2, maximum: 1 }
    ));
}

#[tokio::test]
async fn duplicate_story_holder_leaves_original_snapshot_unchanged() {
    let store = PackStoreMem::new();
    let holder = story_id("story");
    let original_ref = snake_pack().pack_ref().expect("pack ref should compute");
    let rejected_ref = demo_pack().pack_ref().expect("pack ref should compute");
    store
        .retain(&holder, snapshot(snake_pack()))
        .await
        .expect("original snapshot should be retained");

    let error = store
        .retain(&holder, snapshot(demo_pack()))
        .await
        .expect_err("duplicate holder should fail");

    assert!(matches!(
        error,
        PersistenceError::StoryHolderAlreadyExists { story_id } if story_id == holder
    ));
    store.get_pack(&original_ref).await.expect("original snapshot should remain");
    assert!(matches!(store.get_pack(&rejected_ref).await, Err(PersistenceError::NotFound)));
    store.release(&holder).await.expect("release should succeed");
    assert!(matches!(store.get_pack(&original_ref).await, Err(PersistenceError::NotFound)));
}
