use super::*;
use crate::core::{SemanticVersion, Sha256Digest};
use crate::test_support::{SNAKE_PACK_JSON, XU_XIAN_CARD_JSON};

const LEGACY_V5_PACK_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack.json"));

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn creation_spec(pack_ref: PackRef, cast: BTreeMap<RoleId, RoleActor>) -> StoryCreationSpec {
    StoryCreationSpec {
        pack_ref,
        cast,
        player_id: PlayerId::try_new("player").expect("test player id should be valid"),
        player_role: role_id("xu_xian"),
    }
}

async fn import_snake_pack(store: &Arc<Store>) -> PackRef {
    AiseFactory
        .import_story_pack(Arc::clone(store), &AssetConfig::default(), SNAKE_PACK_JSON)
        .await
        .expect("snake pack should import")
}

#[tokio::test]
async fn imported_story_pack_is_available_to_the_store() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let pack_ref = import_snake_pack(&store).await;

    assert!(pack_ref.pack_id.as_str().starts_with("pack-"));
    let pack = store.pack_store().get_pack(&pack_ref.pack_id).await.expect("pack should load");
    assert_eq!(pack_ref.version, pack.meta.version);
}

#[tokio::test]
async fn legacy_story_pack_json_is_rejected_as_asset_error() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let error = AiseFactory
        .import_story_pack(store, &AssetConfig::default(), LEGACY_V5_PACK_JSON)
        .await
        .expect_err("legacy pack should fail");
    assert!(matches!(error, EngineError::Asset { .. }));
}

#[tokio::test]
async fn story_can_cast_an_imported_character_card() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let pack_ref = import_snake_pack(&store).await;
    let card_ref = AiseFactory
        .import_character_card(Arc::clone(&store), &AssetConfig::default(), XU_XIAN_CARD_JSON)
        .await
        .expect("card should import");
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActor::Card(card_ref))]);

    let info = AiseFactory
        .create_story(Arc::clone(&store), creation_spec(pack_ref, cast))
        .await
        .expect("story should be created");

    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");
    assert!(matches!(story_ctx.cast[&role_id("xu_xian")], RoleActor::Card(_)));
    assert!(matches!(story_ctx.cast[&role_id("bai_suzhen")], RoleActor::Default));
}

#[tokio::test]
async fn story_with_unknown_character_card_is_rejected() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let pack_ref = import_snake_pack(&store).await;
    let card_ref = CharacterCardRef {
        card_id: crate::core::CharacterCardId::try_new("ghost").expect("test card id should be valid"),
        version: SemanticVersion::try_new("1.0.0").expect("test version should be valid"),
        digest: Sha256Digest::try_new("digest").expect("test digest should be valid"),
    };
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActor::Card(card_ref))]);

    let error = AiseFactory
        .create_story(store, creation_spec(pack_ref, cast))
        .await
        .expect_err("unknown card should fail");
    assert!(matches!(error, EngineError::Persistence { .. }));
}
