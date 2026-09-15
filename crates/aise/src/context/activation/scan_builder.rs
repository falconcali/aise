use crate::config::ActivationConfig;
use crate::context::error::ContextError;
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::activation::{
    ActivationError, ActivationScanBuffer, ScanBufferError, ScanFragment, ScanFragmentKind,
};
use crate::domain::narrative_graph::projector::NarrativeProjection;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::RoleContextView;

pub(crate) fn build_activation_scan_buffer(
    snapshot: &StoryReadSnapshot,
    player_role: &RoleContextView,
    player_contribution: &str,
    narrative_projection: &NarrativeProjection,
    activation_config: &ActivationConfig,
) -> Result<ActivationScanBuffer, ContextError> {
    let max_item_bytes = activation_config.runtime.max_single_entry_bytes;
    let mut fragments = Vec::new();
    let contribution = BoundedText::try_new(player_contribution.to_owned(), "player_contribution", max_item_bytes)
        .map_err(|_| ContextError::InvalidRecord {
            code: "scan_player_contribution",
        })?;
    fragments.push(ScanFragment::new(ScanFragmentKind::PlayerContribution, 0, 0, contribution));
    if !player_role.profile.name.as_str().is_empty() {
        fragments.push(ScanFragment::new(
            ScanFragmentKind::PlayerRoleName,
            0,
            0,
            player_role.profile.name.clone(),
        ));
    }
    if !player_role.role_label.as_str().is_empty() {
        fragments.push(ScanFragment::new(
            ScanFragmentKind::PlayerRoleLabel,
            0,
            0,
            player_role.role_label.clone(),
        ));
    }
    for (order, direction) in narrative_projection.plan.active_directions.iter().enumerate() {
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::NarrativeDirection,
            0,
            stable_order,
            direction.dramatic_focus.clone(),
        ));
    }
    for (order, event) in narrative_projection.plan.world_event_intents.iter().enumerate() {
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::NarrativeEvent,
            0,
            stable_order,
            event.description.clone(),
        ));
    }
    let recent = snapshot.story_continuity().recent_segments();
    for (order, segment) in recent.iter().rev().enumerate() {
        let depth = u16::try_from(order.saturating_add(1)).unwrap_or(u16::MAX);
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::RecentStory,
            depth,
            stable_order,
            segment.text.clone(),
        ));
    }
    let summary = snapshot.story_continuity().summary();
    if !summary.text.as_str().is_empty() {
        let summary_depth = if activation_config.runtime.include_summary_at_max_depth {
            activation_config.runtime.max_scan_depth
        } else {
            u16::try_from(recent.len().saturating_add(1)).unwrap_or(u16::MAX)
        };
        fragments.push(ScanFragment::new(
            ScanFragmentKind::StorySummary,
            summary_depth,
            0,
            summary.text.clone(),
        ));
    }
    ActivationScanBuffer::try_new(
        fragments,
        activation_config.runtime.max_scan_fragments,
        activation_config.runtime.max_scan_bytes,
    )
    .map_err(|error| match error {
        ScanBufferError::FragmentLimit => ContextError::from(ActivationError::WorkLimitExceeded {
            limit: "max_scan_fragments",
        }),
        ScanBufferError::ByteLimit => ContextError::from(ActivationError::WorkLimitExceeded {
            limit: "max_scan_bytes",
        }),
        ScanBufferError::InvalidLimit => ContextError::from(ActivationError::InvalidRule {
            code: "activation_scan_buffer_limit",
        }),
    })
}
