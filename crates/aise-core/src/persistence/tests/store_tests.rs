use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{StoryAssetSpec, StoryId};
use crate::persistence::PersistenceError;

fn story_id() -> StoryId {
    StoryId::try_new("story").expect("test story id should be valid")
}

fn story_assets() -> StoryAssetSpec {
    let pack = snake_pack();
    let card = xu_xian_card();
    StoryAssetSpec {
        pack_ref: pack.pack_ref().expect("pack ref should compute"),
        pack,
        cards: vec![(card.card_ref().expect("card ref should compute"), card)],
    }
}

#[tokio::test]
async fn store_exposes_pack_store() {
    let store = Store::default();
    let assets = story_assets();
    let pack_ref = assets.pack_ref.clone();

    store
        .pack_store()
        .retain(&story_id(), assets)
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
    let assets = story_assets();
    let pack_ref = assets.pack_ref.clone();
    let card_ref = assets.cards[0].0.clone();

    store
        .pack_store()
        .retain(&story_id(), assets)
        .await
        .expect("assets should be retained");
    store.pack_store().get_pack(&pack_ref).await.expect("pack should load");
    store
        .pack_store()
        .get_character_card(&card_ref)
        .await
        .expect("card should load");
}
