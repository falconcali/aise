use super::*;
use crate::core::tests::{snake_pack, story_context, xu_xian_card};
use crate::core::{EntryOrigin, RoleEntry, StoryContext};
use std::collections::BTreeMap;

fn context_with_pack_roles() -> StoryContext {
    let pack = snake_pack();
    let mut story_ctx = story_context();
    story_ctx.roles = pack
        .roles
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RoleEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: body.spec.clone(),
                    profile: body.default_profile.clone(),
                    memories: BTreeMap::new(),
                },
            )
        })
        .collect();
    story_ctx
}

#[test]
fn catalog_excludes_player_and_preserves_stable_role_id_order() {
    let pack = snake_pack();
    let role = pack.roles.values().next().expect("fixture role").clone();
    let mut story_ctx = story_context();
    story_ctx.roles = ["z", "a", "m"]
        .into_iter()
        .map(|id| {
            let role_id = RoleId::try_new(id).expect("valid role id");
            (
                role_id.clone(),
                RoleEntry {
                    id: role_id,
                    origin: EntryOrigin::Seed,
                    spec: role.spec.clone(),
                    profile: role.default_profile.clone(),
                    memories: BTreeMap::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let player_role = RoleId::try_new("m").expect("valid role id");
    let catalog = collect_role_catalog(&story_ctx, &player_role);

    assert_eq!(
        catalog.iter().map(|entry| entry.role_id.as_str()).collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(catalog.len(), story_ctx.roles.len() - 1);
}

#[test]
fn catalog_projects_all_lightweight_role_fields() {
    let story_ctx = context_with_pack_roles();
    let player_role = RoleId::try_new("xu_xian").expect("valid role id");
    let catalog = collect_role_catalog(&story_ctx, &player_role);
    let entry = catalog.first().expect("fixture role");
    let spec = &story_ctx.roles[entry.role_id].spec;

    assert_eq!(entry.name.as_str(), spec.name.as_str());
    assert_eq!(
        entry.aliases.iter().map(BoundedText::as_str).collect::<Vec<_>>(),
        spec.alias.iter().map(BoundedText::as_str).collect::<Vec<_>>()
    );
    assert_eq!(entry.retrieval_hint.as_str(), spec.retrieval.as_str());
    assert!(contains_role(&catalog, entry.role_id));
}

#[test]
fn player_role_resolution_uses_frozen_effective_profile_without_memories() {
    let pack = snake_pack();
    let player_role = RoleId::try_new("xu_xian").expect("valid role id");
    let role = &pack.roles[&player_role];
    let card = xu_xian_card();
    let expected_profile = card.profile.clone();
    let mut story_ctx = context_with_pack_roles();
    story_ctx.player = player_role.clone();
    story_ctx.roles.get_mut(&player_role).expect("player role exists").profile = expected_profile.clone();

    let resolved = resolve_player_role(&story_ctx).expect("player role resolves");

    assert_eq!(resolved.id, player_role);
    assert_eq!(resolved.spec.name, role.spec.name);
    assert_eq!(resolved.spec.alias, role.spec.alias);
    assert_eq!(resolved.spec.backstory, role.spec.backstory);
    assert_eq!(
        serde_json::to_value(&resolved.profile).expect("profile serializes"),
        serde_json::to_value(expected_profile).expect("profile serializes")
    );
    assert!(resolved.memories.is_empty());
}

#[test]
fn missing_player_role_in_context_returns_inconsistent_data() {
    let mut story_ctx = story_context();
    story_ctx.roles.remove(&story_ctx.player);

    let error = resolve_player_role(&story_ctx).expect_err("missing player role should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Plan,
            code: "plan_player_role_missing",
            ..
        }
    ));
}
