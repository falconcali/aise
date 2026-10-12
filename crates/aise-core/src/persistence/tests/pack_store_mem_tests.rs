use super::*;
use crate::core::tests::{demo_pack, snake_pack, xu_xian_card};

fn story_id(value: &str) -> StoryId {
    StoryId::try_new(value).expect("test story id should be valid")
}

fn assets(pack: StoryPack, cards: Vec<CharacterCard>) -> StoryAssetSpec {
    StoryAssetSpec {
        pack_ref: pack.pack_ref().expect("pack ref should compute"),
        pack,
        cards: cards
            .into_iter()
            .map(|card| (card.card_ref().expect("card ref should compute"), card))
            .collect(),
    }
}

fn store_with_capacity(max_packs: usize, max_character_cards: usize) -> PackStoreMem {
    PackStoreMem::with_config(PackStoreConfig {
        max_packs,
        max_character_cards,
        ..PackStoreConfig::default()
    })
}

#[tokio::test]
async fn retained_pack_is_loadable_by_ref() {
    let store = PackStoreMem::new();
    let spec = assets(snake_pack(), Vec::new());
    let pack_ref = spec.pack_ref.clone();

    store.retain(&story_id("story"), spec).await.expect("pack should be retained");
    let loaded = store.get_pack(&pack_ref).await.expect("pack should load");

    assert_eq!(loaded.opening, snake_pack().opening);
}

#[tokio::test]
async fn unretained_pack_is_not_found() {
    let store = PackStoreMem::new();
    let pack_ref = assets(snake_pack(), Vec::new()).pack_ref;

    let error = store.get_pack(&pack_ref).await.expect_err("missing pack should fail");
    assert!(matches!(error, PersistenceError::NotFound));
}

#[tokio::test]
async fn identical_pack_is_shared_until_last_story_releases_it() {
    let store = store_with_capacity(1, 0);
    let pack_ref = assets(snake_pack(), Vec::new()).pack_ref;
    store
        .retain(&story_id("first"), assets(snake_pack(), Vec::new()))
        .await
        .expect("first story should retain pack");
    store
        .retain(&story_id("second"), assets(snake_pack(), Vec::new()))
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
    let store = store_with_capacity(1, 1);
    store
        .retain(&story_id("snake"), assets(snake_pack(), Vec::new()))
        .await
        .expect("first pack should be retained");

    let error = store
        .retain(&story_id("demo"), assets(demo_pack(), Vec::new()))
        .await
        .expect_err("pack beyond capacity should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn card_capacity_failure_retains_nothing() {
    let store = store_with_capacity(1, 0);
    let spec = assets(snake_pack(), vec![xu_xian_card()]);
    let pack_ref = spec.pack_ref.clone();

    let error = store
        .retain(&story_id("story"), spec)
        .await
        .expect_err("card beyond capacity should fail");

    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
    assert!(matches!(store.get_pack(&pack_ref).await, Err(PersistenceError::NotFound)));
}

#[tokio::test]
async fn retained_card_is_loadable_and_released_with_story() {
    let store = PackStoreMem::new();
    let spec = assets(snake_pack(), vec![xu_xian_card(), xu_xian_card()]);
    let card_ref = spec.cards[0].0.clone();

    store.retain(&story_id("story"), spec).await.expect("assets should be retained");
    let loaded = store.get_character_card(&card_ref).await.expect("card should load");
    assert_eq!(loaded.name, xu_xian_card().name);

    store.release(&story_id("story")).await.expect("release should succeed");
    assert!(matches!(
        store.get_character_card(&card_ref).await,
        Err(PersistenceError::NotFound)
    ));
}
