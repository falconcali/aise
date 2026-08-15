use super::*;
use crate::domain::asset::ids::PlayerId;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;

fn sample_role(controller: RoleController) -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new("role-1").unwrap(),
        controller,
        role_label: BoundedText::try_new("Protagonist", "role_label", 256).unwrap(),
        narrative_function: BoundedText::try_new("Drives the main plot", "narrative_function", 256).unwrap(),
        background: None,
        effective_profile: CharacterProfile {
            name: BoundedText::try_new("Alice", "name", 256).unwrap(),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character: None,
        state: StoryRoleState {
            location: "loc-1".into(),
            goals: Vec::new(),
            attributes: Default::default(),
        },
    }
}

#[test]
fn is_player_controlled_reflects_controller() {
    let player = sample_role(RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    assert!(player.is_player_controlled());

    let ai = sample_role(RoleController::Ai);
    assert!(!ai.is_player_controlled());
}

#[test]
fn story_role_view_from_preserves_fields_and_derives_source_character_id() {
    let role = sample_role(RoleController::Ai);
    let view = StoryRoleView::from(&role);

    assert_eq!(view.role_id, role.role_id);
    assert_eq!(view.role_label, role.role_label);
    assert_eq!(view.narrative_function, role.narrative_function);
    assert_eq!(view.effective_profile.name, role.effective_profile.name);
    assert!(view.source_character_id.is_none());
    assert!(!view.is_player_controlled());
}
