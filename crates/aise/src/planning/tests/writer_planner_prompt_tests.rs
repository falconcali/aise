use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::LocationKey;
use crate::domain::story_instance::role::StoryRoleState;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 1024).unwrap()
}

fn role() -> RoleContextView {
    RoleContextView {
        role_id: RoleId::try_new("guard").unwrap(),
        role_label: bounded("Guard Captain"),
        narrative_function: bounded("blocks the gate"),
        background: Some(bounded("secret orders")),
        profile: CharacterProfile {
            name: bounded("Guard"),
            appearance: Some(bounded("scarred")),
            personality: Some(bounded("watchful")),
            speaking_style: Some(bounded("formal")),
            dialogue_examples: Vec::new(),
        },
        state: StoryRoleState {
            location: LocationKey::from("gate"),
            goals: vec![bounded("hold the gate")],
            attributes: BTreeMap::new(),
        },
        controller: RoleController::Ai,
    }
}

#[test]
fn writer_role_rendering_has_exact_profile_and_state_fields() {
    let rendered = render_role(&role(), false);
    assert_eq!(
        rendered,
        "role_id: \"guard\"\nname: \"Guard\"\nrole: \"Guard Captain\"\nappearance: \"scarred\"\npersonality: \"watchful\"\nspeaking_style: \"formal\"\nbackground: \"secret orders\"\nlocation: \"gate\"\ngoals: [\"hold the gate\"]\nattributes: {}"
    );
    assert!(!rendered.contains("control:"));
}

#[test]
fn writer_role_rendering_omits_absent_and_duplicate_fields() {
    let mut value = role();
    value.role_label = value.profile.name.clone();
    value.background = None;
    value.profile.appearance = None;
    value.profile.personality = None;
    value.profile.speaking_style = None;
    let rendered = render_role(&value, false);
    assert!(!rendered.contains("\nrole:"));
    assert!(!rendered.contains("background:"));
    assert!(!rendered.contains("appearance:"));
    assert!(!rendered.contains("personality:"));
    assert!(!rendered.contains("speaking_style:"));
}

#[test]
fn writer_role_collection_uses_required_indentation() {
    let rendered = render_roles(&[role()]);
    assert!(rendered.starts_with("- role_id: \"guard\"\n  name: \"Guard\""));
    assert!(rendered.contains("\n  attributes: {}"));
}

#[test]
fn writer_planner_assets_preserve_required_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/writer-planner.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/writer-planner.md.j2");
    assert_eq!(section_item_count(csi, "## MUST", "## SHOULD"), 10);
    assert_eq!(section_item_count(csi, "## SHOULD", "## NEVER"), 3);
    assert_eq!(section_item_count(csi, "## NEVER", "# Runtime Data Boundary"), 5);
    assert_eq!(section_item_count(fti, "## MUST", "## NEVER"), 5);
    assert_eq!(section_item_count(fti, "## NEVER", "# Output"), 3);
}

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}
