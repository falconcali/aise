use super::*;
use serde_json::json;

#[test]
fn narrative_participant_uses_tagged_role_contract() {
    let participant = NarrativeParticipant::Role(RoleId::try_new("guide").unwrap());

    assert_eq!(
        serde_json::to_value(participant).unwrap(),
        json!({"kind": "role", "key": "guide"})
    );
}

#[test]
fn narrative_participant_uses_tagged_location_contract() {
    let participant = NarrativeParticipant::Location(LocationKey::try_new("harbor").unwrap());

    assert_eq!(
        serde_json::to_value(participant).unwrap(),
        json!({"kind": "location", "key": "harbor"})
    );
}

#[test]
fn narrative_participant_rejects_non_participant_kind() {
    let result = serde_json::from_value::<NarrativeParticipant>(json!({"kind": "world", "key": "earth"}));

    assert!(result.is_err());
}

#[test]
fn narrative_participant_rejects_unknown_fields() {
    let result =
        serde_json::from_value::<NarrativeParticipant>(json!({"kind": "role", "key": "guide", "unexpected": true}));

    assert!(result.is_err());
}
