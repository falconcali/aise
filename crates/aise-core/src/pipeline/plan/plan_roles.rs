use crate::core::{BoundedText, RetrievalHint, RoleId, StoryPack};

pub(super) struct RoleCatalogEntry<'a> {
    pub role_id: &'a RoleId,
    pub name: &'a BoundedText,
    pub aliases: &'a [BoundedText],
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) fn collect_role_catalog(story_pack: &StoryPack) -> Vec<RoleCatalogEntry<'_>> {
    story_pack
        .roles
        .iter()
        .map(|(role_id, role)| RoleCatalogEntry {
            role_id,
            name: &role.name,
            aliases: &role.alias,
            retrieval_hint: &role.retrieval,
        })
        .collect()
}

pub(super) fn contains_role(catalog: &[RoleCatalogEntry<'_>], role_id: &RoleId) -> bool {
    catalog.iter().any(|entry| entry.role_id == role_id)
}

pub(super) fn catalog_bytes(catalog: &[RoleCatalogEntry<'_>]) -> usize {
    catalog.iter().fold(0usize, |total, entry| {
        total
            .saturating_add(entry.role_id.as_str().len())
            .saturating_add(entry.name.as_str().len())
            .saturating_add(
                entry
                    .aliases
                    .iter()
                    .fold(0usize, |alias_total, alias| alias_total.saturating_add(alias.as_str().len())),
            )
            .saturating_add(entry.retrieval_hint.as_str().len())
    })
}

#[cfg(test)]
#[path = "tests/plan_roles_tests.rs"]
mod tests;
