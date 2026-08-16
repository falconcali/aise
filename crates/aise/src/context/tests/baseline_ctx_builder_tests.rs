use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::LocationKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::story_instance::role::{RoleController, StoryRoleState, StoryRoleView};
use std::collections::BTreeMap;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 1024).unwrap()
}

#[test]
fn role_context_projection_uses_one_story_role_view() {
    let role = StoryRoleView {
        role_id: RoleId::try_new("guard").unwrap(),
        controller: RoleController::Ai,
        role_label: bounded("Guard Captain"),
        narrative_function: bounded("blocks the gate"),
        background: Some(bounded("secret orders")),
        effective_profile: CharacterProfile {
            name: bounded("Guard"),
            appearance: Some(bounded("scarred")),
            personality: Some(bounded("watchful")),
            speaking_style: Some(bounded("formal")),
            dialogue_examples: Vec::new(),
        },
        source_character_id: None,
        state: StoryRoleState {
            location: LocationKey::from("gate"),
            goals: vec![bounded("hold")],
            attributes: BTreeMap::new(),
        },
    };
    let projected = project_role_context(&role);
    assert_eq!(projected.role_id, role.role_id);
    assert_eq!(projected.role_label, role.role_label);
    assert_eq!(projected.narrative_function, role.narrative_function);
    assert_eq!(projected.background, role.background);
    assert_eq!(projected.profile, role.effective_profile);
    assert_eq!(projected.state, role.state);
    assert_eq!(projected.controller, role.controller);
}
