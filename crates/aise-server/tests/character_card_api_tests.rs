use aise_server::api::character_card::{CharacterCardInfoView, ValidationIssueView, ValidationResponse};

#[test]
fn valid_validation_response_serializes_with_empty_issues() {
    let response = ValidationResponse {
        valid: true,
        issues: vec![],
    };
    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["valid"], true);
    assert_eq!(json["issues"], serde_json::json!([]));
}

#[test]
fn invalid_validation_response_exposes_code_path_and_message() {
    let response = ValidationResponse {
        valid: false,
        issues: vec![ValidationIssueView {
            code: "empty_text".into(),
            path: "/profile/name".into(),
            message: "name must not be empty".into(),
        }],
    };
    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["valid"], false);
    assert_eq!(json["issues"][0]["code"], "empty_text");
    assert_eq!(json["issues"][0]["path"], "/profile/name");
    assert_eq!(json["issues"][0]["message"], "name must not be empty");
}

#[test]
fn character_card_info_view_exposes_identity_and_digest_fields() {
    let view = CharacterCardInfoView {
        character_id: "550e8400-e29b-41d4-a716-446655440000".into(),
        name: "The Traveler".into(),
        creator: Some("aise-team".into()),
        version: "1.0.0".into(),
        digest: "0".repeat(64),
    };
    let json = serde_json::to_value(view).unwrap();
    assert_eq!(json["character_id"], "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(json["name"], "The Traveler");
    assert_eq!(json["creator"], "aise-team");
    assert_eq!(json["version"], "1.0.0");
    assert_eq!(json["digest"], "0".repeat(64));
}

#[test]
fn character_card_info_view_omits_creator_as_null_when_absent() {
    let view = CharacterCardInfoView {
        character_id: "550e8400-e29b-41d4-a716-446655440000".into(),
        name: "The Traveler".into(),
        creator: None,
        version: "1.0.0".into(),
        digest: "0".repeat(64),
    };
    let json = serde_json::to_value(view).unwrap();
    assert!(json["creator"].is_null());
}
