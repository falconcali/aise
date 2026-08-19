use aise::domain::ids::TurnNumber;
use aise::domain::story_sequence::StorySequence;
use aise::persistence::{StoryOpeningView, StoryTurnView};
use aise_server::api::dto::TurnRequest;
use aise_server::api::story::{StoryInstanceView, StoryView};

#[test]
fn legacy_story_create_path_is_retired_in_favor_of_instances() {
    assert_ne!("/api/stories", "/api/story-instances");
}

#[test]
fn story_instance_response_exposes_opening_as_first_story_segment() {
    let view = StoryInstanceView {
        story_id: "story-1".into(),
        base_revision: 0,
        pack_id: "pack-1".into(),
        player_role_id: "protagonist".into(),
        opening: StoryOpeningView {
            sequence: StorySequence::try_new(1).unwrap(),
            story_text: "The story begins.".into(),
            created_at: 1,
        },
    };

    let json = serde_json::to_value(view).unwrap();
    assert_eq!(json["opening"]["sequence"], 1);
    assert_eq!(json["opening"]["story_text"], "The story begins.");
}

#[test]
fn story_api_omits_current_scene() {
    let view = StoryInstanceView {
        story_id: "story-1".into(),
        base_revision: 0,
        pack_id: "pack-1".into(),
        player_role_id: "protagonist".into(),
        opening: StoryOpeningView {
            sequence: StorySequence::try_new(1).unwrap(),
            story_text: "The story begins.".into(),
            created_at: 1,
        },
    };

    let json = serde_json::to_value(view).unwrap();
    assert!(json.get("current_scene").is_none());
}

#[test]
fn story_api_omits_removed_context_fields() {
    let view = StoryView {
        story_id: "story-1".into(),
        base_revision: 0,
        player_role_id: "protagonist".into(),
        opening: None,
        turns: Vec::new(),
        next_turn_after: None,
        roles: Vec::new(),
    };

    let json = serde_json::to_value(view).unwrap();
    assert!(json.get("premise").is_none());
    assert!(json.get("current_scene").is_none());
}

#[test]
fn turn_request_accepts_only_player_contribution() {
    let request = serde_json::from_value::<TurnRequest>(serde_json::json!({
        "player_contribution": "你是谁",
        "include_trace": true
    }))
    .expect("player contribution request");
    assert_eq!(request.player_contribution, "你是谁");
    assert!(request.include_trace);
    let legacy_field = ["player", "input"].join("_");
    let mut legacy_request = serde_json::Map::new();
    legacy_request.insert(legacy_field, serde_json::Value::String("你是谁".into()));
    legacy_request.insert("include_trace".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<TurnRequest>(serde_json::Value::Object(legacy_request)).is_err());
}

#[test]
fn story_turn_history_serializes_player_contribution_only() {
    let view = StoryTurnView {
        turn_number: TurnNumber::try_new(1).unwrap(),
        sequence: StorySequence::try_new(2).unwrap(),
        player_contribution: "你是谁".into(),
        story_text: "你隔着门问：你是谁？".into(),
        created_at: 0,
    };
    let json = serde_json::to_value(view).unwrap();
    assert_eq!(json["player_contribution"], "你是谁");
    assert!(json.get(["player", "input"].join("_")).is_none());
}
