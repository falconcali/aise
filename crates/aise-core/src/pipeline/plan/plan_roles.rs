use crate::core::{BoundedText, RetrievalHint, RoleEntry, RoleId, StoryContext};
use crate::pipeline::common::{PipelineError, PipelineStage};

pub(super) struct RoleCatalogEntry<'a> {
    pub role_id: &'a RoleId,
    pub name: &'a BoundedText,
    pub aliases: &'a [BoundedText],
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) fn collect_role_catalog<'a>(story_ctx: &'a StoryContext, player_role: &RoleId) -> Vec<RoleCatalogEntry<'a>> {
    story_ctx
        .roles
        .iter()
        .filter(|(role_id, _)| *role_id != player_role)
        .map(|(role_id, role)| RoleCatalogEntry {
            role_id,
            name: &role.spec.name,
            aliases: &role.spec.alias,
            retrieval_hint: &role.spec.retrieval,
        })
        .collect()
}

pub(super) fn resolve_player_role(story_ctx: &StoryContext) -> Result<&RoleEntry, PipelineError> {
    let role_id = &story_ctx.player;
    story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::inconsistent_data(
            PipelineStage::Plan,
            "plan_player_role_missing",
            format!("player role ID {role_id} is missing from the story context"),
        )
    })
}

pub(super) fn contains_role(catalog: &[RoleCatalogEntry<'_>], role_id: &RoleId) -> bool {
    catalog.iter().any(|entry| entry.role_id == role_id)
}

#[cfg(test)]
#[path = "tests/plan_roles_tests.rs"]
mod tests;
