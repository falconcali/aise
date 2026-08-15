use aise::domain::asset::character_card::CharacterProfile;
use aise::domain::asset::ids::{LocationKey, PlayerId, SceneKey};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::RoleId;
use aise::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState, StoryRoleView};
use aise::domain::story_instance::state::CurrentScene;

fn sample_profile(name: &str) -> CharacterProfile {
    CharacterProfile {
        name: BoundedText::try_new(name, "name", 256).unwrap(),
        appearance: None,
        personality: None,
        speaking_style: None,
        dialogue_examples: Vec::new(),
    }
}

fn sample_role(role_id: &str, controller: RoleController) -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new(role_id).unwrap(),
        controller,
        role_label: BoundedText::try_new("Protagonist", "role_label", 256).unwrap(),
        narrative_function: BoundedText::try_new("Drives the main plot", "narrative_function", 256).unwrap(),
        background: None,
        effective_profile: sample_profile("Hero"),
        source_character: None,
        state: StoryRoleState {
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: Default::default(),
        },
    }
}

#[test]
fn story_role_is_player_controlled_only_for_player_controller() {
    let player = sample_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    assert!(player.is_player_controlled());

    let ai = sample_role("narrator", RoleController::Ai);
    assert!(!ai.is_player_controlled());
}

#[test]
fn story_role_view_from_preserves_story_fields() {
    let role = sample_role("protagonist", RoleController::Ai);
    let view: StoryRoleView = StoryRoleView::from(&role);

    assert_eq!(view.role_id, role.role_id);
    assert_eq!(view.role_label, role.role_label);
    assert_eq!(view.narrative_function, role.narrative_function);
    assert_eq!(view.effective_profile.name, role.effective_profile.name);
    assert_eq!(view.state, role.state);
    assert!(view.source_character_id.is_none());
}

#[test]
fn current_scene_is_structured_by_role_id() {
    let scene = CurrentScene {
        scene_key: SceneKey::from("scene_1"),
        location_key: LocationKey::from("village"),
        time: BoundedText::try_new("morning", "time", 64).unwrap(),
        description: BoundedText::try_new("The village wakes.", "desc", 256).unwrap(),
        present_role_ids: vec![RoleId::try_new("protagonist").unwrap()],
    };
    assert_eq!(scene.scene_key.as_str(), "scene_1");
    assert_eq!(scene.present_role_ids.len(), 1);
    assert_eq!(scene.present_role_ids[0].as_str(), "protagonist");
}

#[test]
fn duplicate_role_labels_do_not_merge_distinct_role_ids() {
    let first = sample_role("protagonist", RoleController::Ai);
    let mut second = sample_role("narrator", RoleController::Ai);
    second.role_label = first.role_label.clone();

    assert_ne!(first.role_id, second.role_id);
    assert_eq!(first.role_label, second.role_label);
}
