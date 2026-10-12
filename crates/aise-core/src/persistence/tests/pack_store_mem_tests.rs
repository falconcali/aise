use super::*;
use crate::core::BoundedText;
use crate::test_support::{demo_pack, snake_pack, xu_xian_card};

fn pack_id(value: &str) -> PackId {
    PackId::try_new(value).expect("test pack id should be valid")
}

fn store_with_capacity(max_packs: usize, max_character_cards: usize) -> PackStoreMem {
    PackStoreMem::with_config(PackStoreConfig {
        max_packs,
        max_character_cards,
        ..PackStoreConfig::default()
    })
}

#[tokio::test]
async fn imported_pack_is_loadable_by_id() {
    let store = PackStoreMem::new();
    let pack = snake_pack();

    let pack_ref = store
        .import_pack(pack_id("snake"), pack.clone())
        .await
        .expect("pack should import");
    let loaded = store.get_pack(&pack_ref.pack_id).await.expect("pack should load");

    assert_eq!(pack_ref.pack_id, pack_id("snake"));
    assert_eq!(pack_ref.version, pack.meta.version);
    assert_eq!(pack_ref.digest, pack.digest().expect("digest should compute"));
    assert_eq!(loaded.opening, pack.opening);
}

#[tokio::test]
async fn missing_pack_is_not_found() {
    let store = PackStoreMem::new();

    let error = store.get_pack(&pack_id("missing")).await.expect_err("missing pack should fail");
    assert!(matches!(error, PersistenceError::NotFound));
}

#[tokio::test]
async fn reimporting_identical_pack_returns_existing_ref() {
    let store = PackStoreMem::new();

    let first = store
        .import_pack(pack_id("first"), snake_pack())
        .await
        .expect("pack should import");
    let second = store
        .import_pack(pack_id("second"), snake_pack())
        .await
        .expect("identical pack should deduplicate");

    assert_eq!(second.pack_id, first.pack_id);
    assert!(matches!(
        store.get_pack(&pack_id("second")).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn different_pack_under_existing_id_is_rejected() {
    let store = PackStoreMem::new();
    store
        .import_pack(pack_id("pack"), snake_pack())
        .await
        .expect("pack should import");

    let error = store
        .import_pack(pack_id("pack"), demo_pack())
        .await
        .expect_err("conflicting pack id should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn pack_import_beyond_capacity_is_rejected() {
    let store = store_with_capacity(1, 1);
    store
        .import_pack(pack_id("snake"), snake_pack())
        .await
        .expect("first pack should import");

    let error = store
        .import_pack(pack_id("demo"), demo_pack())
        .await
        .expect_err("pack beyond capacity should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn imported_character_card_is_loadable_by_ref() {
    let store = PackStoreMem::new();
    let card = xu_xian_card();

    let card_ref = store.import_character_card(card.clone()).await.expect("card should import");
    let loaded = store.get_character_card(&card_ref).await.expect("card should load");

    assert_eq!(card_ref.card_id, card.card_id);
    assert_eq!(card_ref.version, card.meta.version);
    assert_eq!(loaded.name, card.name);
}

#[tokio::test]
async fn reimporting_identical_card_returns_existing_ref() {
    let store = PackStoreMem::new();

    let first = store.import_character_card(xu_xian_card()).await.expect("card should import");
    let second = store
        .import_character_card(xu_xian_card())
        .await
        .expect("identical card should deduplicate");

    assert_eq!(first.digest, second.digest);
}

#[tokio::test]
async fn changed_card_under_existing_version_is_rejected() {
    let store = PackStoreMem::new();
    store.import_character_card(xu_xian_card()).await.expect("card should import");
    let mut changed = xu_xian_card();
    changed.name = BoundedText::try_new("许汉文", "name", 256).expect("test name should fit");

    let error = store
        .import_character_card(changed)
        .await
        .expect_err("changed card with same version should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn card_ref_with_wrong_digest_is_rejected() {
    let store = PackStoreMem::new();
    let mut card_ref = store.import_character_card(xu_xian_card()).await.expect("card should import");
    card_ref.digest = Sha256Digest::try_new("stale").expect("test digest should be valid");

    let error = store.get_character_card(&card_ref).await.expect_err("stale digest should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn card_import_beyond_capacity_is_rejected() {
    let store = store_with_capacity(1, 0);

    let error = store
        .import_character_card(xu_xian_card())
        .await
        .expect_err("card beyond capacity should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}
