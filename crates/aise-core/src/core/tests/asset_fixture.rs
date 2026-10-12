use crate::core::{AssetConfig, CharacterCard, StoryAssetSpec, StoryContext, StoryPack};
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
    let story_ctx = StoryContext::new();
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryAssetSpec {
                pack_ref: story_ctx.pack_ref,
                pack: snake_pack(),
                cards: Vec::new(),
            },
        )
        .await
        .expect("default pack should be retained");
    store
}
