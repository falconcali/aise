use aise::domain::story_sequence::StorySequence;
use aise::persistence::StoryOpeningView;
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
