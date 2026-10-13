use super::*;
use crate::core::tests::xu_xian_card;

#[test]
fn example_character_card_imports() {
    let card = xu_xian_card();

    assert_eq!(
        card.card_id,
        CharacterCardId::try_new("xu_xian_bold_physician").expect("valid card id")
    );
}

#[test]
fn card_ref_pins_card_id_version_and_digest() {
    let card = xu_xian_card();

    let card_ref = card.card_ref().expect("card ref should compute");

    assert_eq!(card_ref.card_id, card.card_id);
    assert_eq!(card_ref.version, card.meta.version);
    assert_eq!(card_ref.digest.as_str().len(), 64);
    assert!(
        card_ref
            .digest
            .as_str()
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
    );
    assert_eq!(card_ref.digest, card.card_ref().expect("card ref should compute").digest);
}
