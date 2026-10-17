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
fn character_card_round_trips_without_runtime_reference() {
    let card = xu_xian_card();
    let bytes = serde_json::to_vec(&card).expect("character card should serialize");
    let reparsed: CharacterCard = serde_json::from_slice(&bytes).expect("character card should deserialize");

    assert_eq!(
        serde_json::to_vec(&reparsed).expect("reparsed character card should serialize"),
        bytes
    );
    assert_eq!(reparsed.card_id, card.card_id);
    assert_eq!(reparsed.meta.version, card.meta.version);
}

#[test]
fn character_card_rejects_unknown_fields() {
    let mut value = serde_json::to_value(xu_xian_card()).expect("character card should serialize");
    value["unknown"] = serde_json::json!(true);

    assert!(serde_json::from_value::<CharacterCard>(value).is_err());
}
