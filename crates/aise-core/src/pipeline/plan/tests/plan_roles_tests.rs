use super::*;
use crate::core::tests::snake_pack;
use std::collections::BTreeMap;

#[test]
fn catalog_contains_every_role_in_stable_role_id_order() {
    let mut pack = snake_pack();
    let role = pack.roles.values().next().expect("fixture role").clone();
    pack.roles = ["z", "a", "m"]
        .into_iter()
        .map(|id| (RoleId::try_new(id).expect("valid role id"), role.clone()))
        .collect::<BTreeMap<_, _>>();

    let catalog = collect_role_catalog(&pack);

    assert_eq!(
        catalog.iter().map(|entry| entry.role_id.as_str()).collect::<Vec<_>>(),
        ["a", "m", "z"]
    );
    assert_eq!(catalog.len(), pack.roles.len());
}

#[test]
fn catalog_projects_all_lightweight_role_fields() {
    let pack = snake_pack();
    let catalog = collect_role_catalog(&pack);
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
    let catalog = collect_role_catalog(&pack);
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
