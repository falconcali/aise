use super::*;
use crate::core::{PackId, StoryId};
use crate::persistence::PersistenceError;
use crate::test_support::{snake_pack, xu_xian_card};

fn pack_id() -> PackId {
    PackId::try_new("pack").expect("test pack id should be valid")
}

#[tokio::test]
async fn store_exposes_pack_store() {
    let store = Store::default();

    store
        .pack_store()
        .import_pack(pack_id(), snake_pack())
        .await
        .expect("pack should import");
    let pack = store.pack_store().get_pack(&pack_id()).await.expect("pack should load");
    assert!(!pack.opening.as_str().is_empty());
}

#[tokio::test]
async fn store_exposes_story_store() {
    let store = Store::default();
    let story_id = StoryId::try_new("missing").expect("test story id should be valid");

    let error = store
        .story_store()
        .load(&story_id)
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

    store
        .pack_store()
        .import_pack(pack_id(), snake_pack())
        .await
        .expect("pack should import");
    store.pack_store().get_pack(&pack_id()).await.expect("pack should load");
    let card_ref = store
        .pack_store()
        .import_character_card(xu_xian_card())
        .await
        .expect("card should import");
    store
        .pack_store()
        .get_character_card(&card_ref)
        .await
        .expect("card should load");
}
