use crate::config::{AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, TurnContentLimitsConfig};
use crate::domain::narrative::StoryContinuityLimits;
use crate::domain::turn::baseline::SnapshotLimits;

impl SnapshotLimits {
    pub fn from_config(
        content: &TurnContentLimitsConfig,
        context: &ContextPreparationConfig,
        assets: &AssetLimitsConfig,
        narrative: &NarrativeConfig,
    ) -> Self {
        Self {
            max_story_profile_bytes: content.max_story_profile_bytes,
            max_instance_settings: content.max_instance_settings,
            max_instance_setting_bytes: content.max_instance_setting_bytes,
            max_roles: content.max_roles,
            max_role_bytes: content.max_role_bytes,
            max_relationships: context.max_relationships,
            max_narrative_nodes: narrative.max_graph_nodes,
            max_condition_fact_values: context.max_condition_fact_values,
            max_constraints: content.max_constraints,
            max_constraint_bytes: content.max_constraint_bytes,
            max_topics: assets.max_topics,
            max_topic_aliases_per_topic: assets.max_topic_aliases_per_topic,
            max_entity_catalog: context.max_entity_catalog,
            continuity: StoryContinuityLimits {
                max_summary_bytes: content.max_summary_bytes,
                max_recent_segments: content.max_recent_segments,
                max_recent_segment_bytes: content.max_recent_segment_bytes,
                max_recent_segment_tokens: content.max_recent_segment_tokens,
            },
        }
    }
}
