use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{RoleActor, StoryAssetSpec, StoryContext};
use std::collections::BTreeMap;

#[test]
fn catalog_excludes_player_and_preserves_stable_role_id_order() {
    let mut pack = snake_pack();
    let role = pack.roles.values().next().expect("fixture role").clone();
    pack.roles = ["z", "a", "m"]
        .into_iter()
        .map(|id| (RoleId::try_new(id).expect("valid role id"), role.clone()))
        .collect::<BTreeMap<_, _>>();

    let player_role = RoleId::try_new("m").expect("valid role id");
    let catalog = collect_role_catalog(&pack, &player_role);

    assert_eq!(
        catalog.iter().map(|entry| entry.role_id.as_str()).collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(catalog.len(), pack.roles.len() - 1);
}

#[test]
fn catalog_projects_all_lightweight_role_fields() {
    let pack = snake_pack();
    let player_role = RoleId::try_new("xu_xian").expect("valid role id");
    let catalog = collect_role_catalog(&pack, &player_role);
    let entry = catalog.first().expect("fixture role");
    let body = &pack.roles[entry.role_id];

    assert_eq!(entry.name.as_str(), body.name.as_str());
    assert_eq!(
        entry.aliases.iter().map(BoundedText::as_str).collect::<Vec<_>>(),
        body.alias.iter().map(BoundedText::as_str).collect::<Vec<_>>()
    );
    assert_eq!(entry.retrieval_hint.as_str(), body.retrieval.as_str());
    assert!(contains_role(&catalog, entry.role_id));
}

#[test]
fn catalog_byte_count_includes_each_projected_field_once() {
    let pack = snake_pack();
    let player_role = RoleId::try_new("xu_xian").expect("valid role id");
    let catalog = collect_role_catalog(&pack, &player_role);
    let expected = catalog
        .iter()
        .map(|entry| {
            entry.role_id.as_str().len()
                + entry.name.as_str().len()
                + entry.aliases.iter().map(|alias| alias.as_str().len()).sum::<usize>()
                + entry.retrieval_hint.as_str().len()
        })
        .sum::<usize>();

    assert_eq!(catalog_bytes(&catalog), expected);
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
    let mut story_ctx = StoryContext::new();
    story_ctx.player = player_role.clone();
    story_ctx.cast.insert(player_role.clone(), RoleActor::Card(card_ref.clone()));
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

    let resolved = resolve_player_role(&store, &pack, &story_ctx)
        .await
        .expect("player role resolves");

    assert_eq!(resolved.role_id, player_role);
    assert_eq!(resolved.name, role.name);
    assert_eq!(resolved.aliases, role.alias);
    assert_eq!(resolved.backstory, role.backstory);
    assert_eq!(
        serde_json::to_value(resolved.profile).expect("profile serializes"),
        serde_json::to_value(expected_profile).expect("profile serializes")
    );
}
