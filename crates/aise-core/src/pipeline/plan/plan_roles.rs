use crate::core::{BoundedText, PlayerRoleContext, RetrievalHint, RoleActor, RoleId, StoryContext};
use crate::persistence::{PersistenceError, Store};
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
            name: &role.body.name,
            aliases: &role.body.alias,
            retrieval_hint: &role.body.retrieval,
        })
        .collect()
}

pub(super) async fn resolve_player_role(
    store: &Store,
    story_ctx: &StoryContext,
) -> Result<PlayerRoleContext, PipelineError> {
    let role_id = &story_ctx.player;
    let role_entry = story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::inconsistent_data(
            PipelineStage::Plan,
            "plan_player_role_missing",
            format!("player role ID {role_id} is missing from the story context"),
        )
    })?;
    let profile = match &role_entry.actor {
        RoleActor::Default => role_entry.body.default_profile.clone(),
        RoleActor::Card(card_ref) => store
            .pack_store()
            .get_character_card(card_ref)
            .await
            .map(|card| card.profile.clone())
            .map_err(|error| map_player_card_error(role_id, error))?,
    };
    Ok(PlayerRoleContext {
        role_id: role_id.clone(),
        name: role_entry.body.name.clone(),
        aliases: role_entry.body.alias.clone(),
        backstory: role_entry.body.backstory.clone(),
        profile,
    })
}

pub(super) fn contains_role(catalog: &[RoleCatalogEntry<'_>], role_id: &RoleId) -> bool {
    catalog.iter().any(|entry| entry.role_id == role_id)
}

fn map_player_card_error(role_id: &RoleId, error: PersistenceError) -> PipelineError {
    match error {
        PersistenceError::NotFound => PipelineError::inconsistent_data(
            PipelineStage::Plan,
            "plan_player_role_card_missing",
            format!("character card for player role ID {role_id} is missing"),
        ),
        source => PipelineError::Persistence {
            stage: PipelineStage::Plan,
            source,
        },
    }
}

#[cfg(test)]
#[path = "tests/plan_roles_tests.rs"]
mod tests;
