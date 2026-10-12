use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{BoundedText, PackRef};
use crate::persistence::PersistenceError;

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn snake_pack_ref() -> PackRef {
    snake_pack().pack_ref().expect("pack ref should compute")
}

fn creation_spec(cast: BTreeMap<RoleId, RoleActorSpec>, player_role: &str) -> StoryCreationSpec {
    StoryCreationSpec {
        story_pack: snake_pack(),
        cast,
        player_id: PlayerId::try_new("player").expect("test player id should be valid"),
        player_role: role_id(player_role),
    }
}

async fn create_story(
    store: &Arc<Store>,
    asset_config: &AssetConfig,
    spec: StoryCreationSpec,
) -> Result<StoryInstanceInfo, EngineError> {
    AiseFactory.create_story(Arc::clone(store), asset_config, spec).await
}

#[tokio::test]
async fn created_story_pins_a_frozen_pack_snapshot() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let info = create_story(&store, &AssetConfig::default(), creation_spec(BTreeMap::new(), "xu_xian"))
        .await
        .expect("story should be created");

    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");
    assert_eq!(story_ctx.pack_ref.pack_id, snake_pack().pack_id);
    assert_eq!(story_ctx.pack_ref.version, snake_pack_ref().version);
    assert_eq!(story_ctx.pack_ref.digest, snake_pack_ref().digest);
    let pack = store
        .pack_store()
        .get_pack(&story_ctx.pack_ref)
        .await
        .expect("pack snapshot should load");
    assert_eq!(pack.opening, snake_pack().opening);
}

#[tokio::test]
async fn story_can_cast_a_character_card() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActorSpec::Card(Box::new(xu_xian_card())))]);

    let info = create_story(&store, &AssetConfig::default(), creation_spec(cast, "xu_xian"))
        .await
        .expect("story should be created");

    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");
    let RoleActor::Card(card_ref) = &story_ctx.cast[&role_id("xu_xian")] else {
        panic!("xu_xian should be cast with a card");
    };
    let card = store
        .pack_store()
        .get_character_card(card_ref)
        .await
        .expect("card snapshot should load");
    assert_eq!(card.card_id, xu_xian_card().card_id);
    assert!(matches!(story_ctx.cast[&role_id("bai_suzhen")], RoleActor::Default));
}

#[tokio::test]
async fn story_pack_violating_asset_limits_is_rejected() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let asset_config = AssetConfig {
        max_label_bytes: 4,
        ..AssetConfig::default()
    };

    let error = create_story(&store, &asset_config, creation_spec(BTreeMap::new(), "xu_xian"))
        .await
        .expect_err("oversized pack should fail");

    assert!(matches!(error, EngineError::Asset { .. }));
    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn character_card_violating_asset_limits_is_rejected() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let mut card = xu_xian_card();
    card.name = BoundedText::try_new("x".repeat(512), "name", 1024).expect("test name should fit");
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActorSpec::Card(Box::new(card)))]);

    let error = create_story(&store, &AssetConfig::default(), creation_spec(cast, "xu_xian"))
        .await
        .expect_err("oversized card should fail");

    assert!(matches!(error, EngineError::Asset { .. }));
    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn non_playable_player_role_is_rejected_without_retaining_assets() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let error = create_story(&store, &AssetConfig::default(), creation_spec(BTreeMap::new(), "bai_suzhen"))
        .await
        .expect_err("non playable role should fail");

    assert!(matches!(error, EngineError::Asset { .. }));
    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn removing_story_releases_its_asset_snapshots() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let info = create_story(&store, &AssetConfig::default(), creation_spec(BTreeMap::new(), "xu_xian"))
        .await
        .expect("story should be created");

    AiseFactory
        .remove_story(Arc::clone(&store), &info.story_id)
        .await
        .expect("story should be removed");

    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}
