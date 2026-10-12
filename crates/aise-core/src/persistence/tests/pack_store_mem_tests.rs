use super::*;
use crate::core::AssetConfig;
use crate::persistence::PersistenceError;

fn pack_id() -> PackId {
    PackId::try_new("pack").expect("test pack id should be valid")
}

#[tokio::test]
async fn loads_default_pack() {
    let store = PackStoreMem::new();
    let pack = store.get_pack(&pack_id()).await.expect("pack should load");

    assert!(!pack.opening.as_str().is_empty());
    assert!(!pack.play.playable_roles.is_empty());
}

#[tokio::test]
async fn rejects_pack_exceeding_asset_limits() {
    let store = PackStoreMem::with_config(PackStoreConfig {
        asset: AssetConfig {
            max_opening_bytes: 1,
            ..AssetConfig::default()
        },
        ..PackStoreConfig::default()
    });

    let error = store.get_pack(&pack_id()).await.expect_err("oversized pack should fail");
    assert!(matches!(error, PersistenceError::InvalidAsset(_)));
}
