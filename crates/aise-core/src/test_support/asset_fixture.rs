use crate::core::{AssetConfig, CharacterCard, StoryContext, StoryPack};
use crate::persistence::Store;

pub(crate) const SNAKE_PACK_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack_v1.json"));
pub(crate) const DEMO_PACK_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/demo_pack_v1.json"));
pub(crate) const XU_XIAN_CARD_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/xu_xian_card_v1.json"));

pub(crate) fn snake_pack() -> StoryPack {
    StoryPack::from_json(SNAKE_PACK_JSON, &AssetConfig::default()).expect("snake pack example should import")
}

pub(crate) fn demo_pack() -> StoryPack {
    StoryPack::from_json(DEMO_PACK_JSON, &AssetConfig::default()).expect("demo pack example should import")
}

pub(crate) fn xu_xian_card() -> CharacterCard {
    CharacterCard::from_json(XU_XIAN_CARD_JSON, &AssetConfig::default()).expect("character card example should import")
}

pub(crate) async fn store_with_default_pack() -> Store {
    let store = Store::default();
    store
        .pack_store()
        .import_pack(StoryContext::new().pack_ref.pack_id, snake_pack())
        .await
        .expect("default pack should be stored");
    store
}
