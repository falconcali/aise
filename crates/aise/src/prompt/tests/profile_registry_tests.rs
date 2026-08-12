use super::*;

fn assets(csi: &str, rc: &str, fti: &str) -> PromptProfileAssets {
    PromptProfileAssets {
        csi_slot: csi.into(),
        rc_slot: rc.into(),
        fti_slot: fti.into(),
    }
}

#[test]
fn register_stores_three_distinct_slots() {
    let mut registry = PromptProfileRegistry::default();
    registry
        .register(
            PromptProfile::WriterPlanner,
            assets("writer_planner.csi", "writer_planner.rc", "writer_planner.fti"),
        )
        .unwrap();

    let registered = registry.assets_for(PromptProfile::WriterPlanner).unwrap();

    assert_eq!(registered.csi_slot, "writer_planner.csi");
    assert_eq!(registered.rc_slot, "writer_planner.rc");
    assert_eq!(registered.fti_slot, "writer_planner.fti");
}

#[test]
fn register_rejects_duplicate_profile() {
    let mut registry = PromptProfileRegistry::default();
    registry
        .register(
            PromptProfile::WriterPlanner,
            assets("writer_planner.csi", "writer_planner.rc", "writer_planner.fti"),
        )
        .unwrap();

    let error = registry
        .register(PromptProfile::WriterPlanner, assets("other.csi", "other.rc", "other.fti"))
        .unwrap_err();

    assert!(matches!(error, PromptError::DuplicateProfileRegistration(profile) if profile == "writer_planner"));
}

#[test]
fn register_rejects_slot_reused_across_layers() {
    let mut registry = PromptProfileRegistry::default();

    let error = registry
        .register(
            PromptProfile::CharacterThink,
            assets("character.shared", "character.rc", "character.shared"),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        PromptError::DuplicateLayerSlot { profile, slot }
            if profile == "character_think" && slot == "character.shared"
    ));
}

#[test]
fn assets_for_rejects_unregistered_profile() {
    let registry = PromptProfileRegistry::default();

    let error = registry.assets_for(PromptProfile::StoryGenerator).unwrap_err();

    assert!(matches!(error, PromptError::ProfileNotRegistered(profile) if profile == "story_generator"));
}
