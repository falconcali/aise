use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{RoleActor, RoleEntry, StoryAssetSpec, StoryContext};
use std::collections::BTreeMap;

fn context_with_pack_roles() -> StoryContext {
    let pack = snake_pack();
    let mut story_ctx = StoryContext::new();
    story_ctx.roles = pack
        .roles
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RoleEntry {
                    id: id.clone(),
                    actor: RoleActor::Default,
                    body: body.clone(),
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
    let mut story_ctx = StoryContext::new();
    story_ctx.roles = ["z", "a", "m"]
        .into_iter()
        .map(|id| {
            let role_id = RoleId::try_new(id).expect("valid role id");
            (
                role_id.clone(),
                RoleEntry {
                    id: role_id,
                    actor: RoleActor::Default,
                    body: role.clone(),
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
    let body = &story_ctx.roles[entry.role_id].body;

    assert_eq!(entry.name.as_str(), body.name.as_str());
    assert_eq!(
        entry.aliases.iter().map(BoundedText::as_str).collect::<Vec<_>>(),
        body.alias.iter().map(BoundedText::as_str).collect::<Vec<_>>()
    );
    assert_eq!(entry.retrieval_hint.as_str(), body.retrieval.as_str());
    assert!(contains_role(&catalog, entry.role_id));
}

#[tokio::test]
async fn player_role_resolution_uses_effective_card_profile_without_memories() {
    let store = Store::default();
    let pack = snake_pack();
    let player_role = RoleId::try_new("xu_xian").expect("valid role id");
    let role = &pack.roles[&player_role];
    let card = xu_xian_card();
    let expected_profile = card.profile.clone();
    let card_ref = card.card_ref().expect("card ref resolves");
    let mut story_ctx = context_with_pack_roles();
    story_ctx.player = player_role.clone();
    story_ctx.roles.get_mut(&player_role).expect("player role exists").actor = RoleActor::Card(card_ref.clone());
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryAssetSpec {
                pack_ref: story_ctx.pack_ref.clone(),
                pack: pack.clone(),
                cards: vec![(card_ref, card)],
            },
        )
        .await
        .expect("assets retain");

    let resolved = resolve_player_role(&store, &story_ctx).await.expect("player role resolves");

    assert_eq!(resolved.role_id, player_role);
    assert_eq!(resolved.name, role.name);
    assert_eq!(resolved.aliases, role.alias);
    assert_eq!(resolved.backstory, role.backstory);
    assert_eq!(
        serde_json::to_value(resolved.profile).expect("profile serializes"),
        serde_json::to_value(expected_profile).expect("profile serializes")
    );
}

#[tokio::test]
async fn missing_player_role_in_context_returns_inconsistent_data() {
    let store = Store::default();
    let story_ctx = StoryContext::new();

    let error = resolve_player_role(&store, &story_ctx)
        .await
        .expect_err("missing player role should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Plan,
            code: "plan_player_role_missing",
            ..
        }
    ));
}
