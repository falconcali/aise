use crate::domain::asset::constraint::StoryConstraintRequirement;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::narrative_graph::projector::NarrativePlan;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::{BaselineContext, RoleContextView};
use crate::prompt::{
    RuntimePromptVars, StoryProfilePromptView, TrustedPromptVars, project_narrative_direction,
    render_narrative_direction, render_relevant_knowledge, render_story_profile_view,
    world_knowledge_view_from_baseline,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub const WRITER_PLANNER_CSI_SLOT: &str = "context.writer_planner.csi";
pub const WRITER_PLANNER_RC_SLOT: &str = "context.writer_planner.rc";
pub const WRITER_PLANNER_FTI_SLOT: &str = "context.writer_planner.fti";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexedRetrievalTarget {
    Role(RoleId),
    Knowledge(KnowledgeSourceId),
}

#[derive(Debug, Clone)]
pub struct WriterPlannerPromptContext {
    pub indexed_targets: BTreeMap<String, IndexedRetrievalTarget>,
    pub provided_role_ids: Vec<RoleId>,
    pub provided_knowledge_ids: Vec<KnowledgeSourceId>,
}

pub struct WriterPlannerPromptProjection {
    pub context: WriterPlannerPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

#[derive(Debug, thiserror::Error)]
pub enum WriterPlannerProjectionError {
    #[error("writer planner role target is unknown: {role_id}")]
    UnknownRoleTarget { role_id: RoleId },
    #[error("writer planner role target is player-controlled: {role_id}")]
    PlayerRoleTarget { role_id: RoleId },
    #[error("writer planner role target is duplicated: {role_id}")]
    DuplicateRoleTarget { role_id: RoleId },
    #[error("writer planner retrieval target key collides across target domains: {key}")]
    RetrievalTargetCollision { key: String },
    #[error("writer planner required prompt data exceeds budget")]
    RequiredPromptDataExceedsBudget,
}

pub struct WriterPlannerPromptContextProjector;

impl WriterPlannerPromptContextProjector {
    pub fn project(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        player_contribution: &BoundedText,
        max_input_tokens: u64,
    ) -> Result<WriterPlannerPromptProjection, WriterPlannerProjectionError> {
        let mut indexed_targets = BTreeMap::new();
        for entry in &baseline.role_index {
            insert_target(
                &mut indexed_targets,
                entry.role_id.as_str().to_owned(),
                IndexedRetrievalTarget::Role(entry.role_id.clone()),
            )?;
        }
        for entry in &baseline.knowledge_index {
            insert_target(
                &mut indexed_targets,
                entry.source_id.as_str().to_owned(),
                IndexedRetrievalTarget::Knowledge(entry.source_id.clone()),
            )?;
        }
        let provided_role_ids = std::iter::once(baseline.player_role.role_id.clone())
            .chain(baseline.relevant_roles.iter().map(|role| role.role_id.clone()))
            .collect();
        let provided_knowledge_ids = baseline
            .relevant_world_knowledge
            .facts
            .iter()
            .chain(baseline.relevant_world_knowledge.rumors.iter())
            .map(|entry| entry.source_id.clone())
            .collect();
        let continuity = &baseline.story_continuity;
        let narrative_direction = project_narrative_direction(narrative_plan);
        let rc_vars = HashMap::from([
            (
                "story_profile".into(),
                Value::String(render_story_profile_view(&StoryProfilePromptView::new(
                    &baseline.story_title,
                    &baseline.story_profile,
                ))),
            ),
            (
                "instance_settings".into(),
                Value::String(render_instance_settings(baseline.instance_settings.cast_policy)),
            ),
            (
                "story_summary".into(),
                Value::String(render_story_summary(continuity.summary().text.as_str())),
            ),
            ("recent_story".into(), Value::String(render_recent_story(baseline))),
            (
                "player_character".into(),
                Value::String(render_role(&baseline.player_role, false)),
            ),
            (
                "relevant_characters".into(),
                Value::String(render_roles(&baseline.relevant_roles)),
            ),
            (
                "relevant_knowledge".into(),
                Value::String(render_relevant_knowledge(&world_knowledge_view_from_baseline(
                    &baseline.relevant_world_knowledge,
                ))),
            ),
            ("character_index".into(), Value::String(render_role_index(baseline))),
            ("knowledge_index".into(), Value::String(render_knowledge_index(baseline))),
            (
                "narrative_direction".into(),
                Value::String(render_narrative_direction(&narrative_direction)),
            ),
            ("active_story_constraints".into(), Value::String(render_constraints(baseline))),
            (
                "player_contribution".into(),
                Value::String(render_data(player_contribution.as_str())),
            ),
        ]);
        let fti_vars: HashMap<String, Value> = HashMap::new();
        let input_tokens = rc_vars
            .values()
            .filter_map(Value::as_str)
            .map(estimate_text_tokens)
            .fold(0u64, u64::saturating_add);
        if input_tokens > max_input_tokens {
            return Err(WriterPlannerProjectionError::RequiredPromptDataExceedsBudget);
        }
        Ok(WriterPlannerPromptProjection {
            context: WriterPlannerPromptContext {
                indexed_targets,
                provided_role_ids,
                provided_knowledge_ids,
            },
            rc_vars: RuntimePromptVars::new(rc_vars),
            fti_vars: TrustedPromptVars::new(fti_vars),
        })
    }
}

fn insert_target(
    targets: &mut BTreeMap<String, IndexedRetrievalTarget>,
    key: String,
    target: IndexedRetrievalTarget,
) -> Result<(), WriterPlannerProjectionError> {
    match targets.get(&key) {
        Some(existing) if existing != &target => Err(WriterPlannerProjectionError::RetrievalTargetCollision { key }),
        _ => {
            targets.insert(key, target);
            Ok(())
        }
    }
}

fn render_instance_settings(policy: CastPolicy) -> String {
    let value = match policy {
        CastPolicy::Open => "open",
        CastPolicy::IncidentalOnly => "incidental_only",
        CastPolicy::Closed => "closed",
    };
    format!("cast_policy: {value}")
}

fn render_recent_story(baseline: &BaselineContext) -> String {
    baseline
        .story_continuity
        .recent_segments()
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_roles(roles: &[RoleContextView]) -> String {
    if roles.is_empty() {
        return String::new();
    }
    roles.iter().map(|role| render_role(role, true)).collect::<Vec<_>>().join("\n")
}

fn render_role(role: &RoleContextView, collection: bool) -> String {
    let profile = &role.profile;
    let first = if collection { "- " } else { "" };
    let rest = if collection { "  " } else { "" };
    let mut lines = vec![
        format!("{first}role_id: {}", quoted(role.role_id.as_str())),
        format!("{rest}name: {}", quoted(profile.name.as_str())),
    ];
    if role.role_label != profile.name {
        lines.push(format!("{rest}role: {}", quoted(role.role_label.as_str())));
    }
    push_optional(&mut lines, rest, "appearance", profile.appearance.as_ref());
    push_optional(&mut lines, rest, "personality", profile.personality.as_ref());
    push_optional(&mut lines, rest, "speaking_style", profile.speaking_style.as_ref());
    push_optional(&mut lines, rest, "background", role.background.as_ref());
    lines.push(format!("{rest}location: {}", quoted(role.state.location.as_str())));
    if !role.state.goals.is_empty() {
        lines.push(format!("{rest}goals: {}", quoted_list(&role.state.goals)));
    }
    if !role.state.attributes.is_empty() {
        lines.push(format!("{rest}attributes: {}", render_attributes(&role.state.attributes)));
    }
    lines.join("\n")
}

fn render_role_index(baseline: &BaselineContext) -> String {
    if baseline.role_index.is_empty() {
        return String::new();
    }
    let entries = baseline
        .role_index
        .iter()
        .map(|entry| {
            format!(
                "- target_id: {}\n  retrieval_hint: {}",
                quoted(entry.role_id.as_str()),
                quoted(entry.retrieval_hint.as_str())
            )
        })
        .collect::<Vec<_>>();
    format!("### Retrievable Characters\n\n{}", entries.join("\n"))
}

fn render_knowledge_index(baseline: &BaselineContext) -> String {
    if baseline.knowledge_index.is_empty() {
        return String::new();
    }
    let mut facts = Vec::new();
    let mut rumors = Vec::new();
    for entry in &baseline.knowledge_index {
        let line = format!(
            "- target_id: {}\n  retrieval_hint: {}",
            quoted(entry.source_id.as_str()),
            quoted(entry.retrieval_hint.as_str())
        );
        match entry.source_id.kind() {
            crate::domain::knowledge::KnowledgeKind::Fact => facts.push(line),
            crate::domain::knowledge::KnowledgeKind::Rumor => rumors.push(line),
            crate::domain::knowledge::KnowledgeKind::Memory => {}
        }
    }
    let mut sections = Vec::new();
    if !facts.is_empty() {
        sections.push(format!("### Retrievable Facts\n\n{}", facts.join("\n")));
    }
    if !rumors.is_empty() {
        sections.push(format!("### Retrievable Rumors\n\n{}", rumors.join("\n")));
    }
    sections.join("\n\n")
}

fn render_constraints(baseline: &BaselineContext) -> String {
    if baseline.active_story_constraints.is_empty() {
        return String::new();
    }
    let mut constraints = baseline.active_story_constraints.iter().collect::<Vec<_>>();
    constraints.sort_by_key(|constraint| constraint.id.to_string());
    constraints
        .into_iter()
        .map(|constraint| {
            let (kind, statement) = match &constraint.requirement {
                StoryConstraintRequirement::Require { statement } => ("require", statement.as_str()),
                StoryConstraintRequirement::Forbid { statement } => ("forbid", statement.as_str()),
            };
            format!(
                "- constraint_id: {}\n  kind: {kind}\n  requirement: {}",
                quoted(&constraint.id.to_string()),
                quoted(statement)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_story_summary(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        value.to_owned()
    }
}

fn render_data(value: &str) -> String {
    quoted(value)
}

fn quoted_list(values: &[BoundedText]) -> String {
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn push_optional(lines: &mut Vec<String>, indent: &str, name: &str, value: Option<&BoundedText>) {
    if let Some(value) = value {
        lines.push(format!("{indent}{name}: {}", quoted(value.as_str())));
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn render_attributes(values: &BTreeMap<crate::domain::asset::ids::AttributeKey, ScalarValue>) -> String {
    let items = values
        .iter()
        .map(|(key, value)| format!("{}: {}", quoted(key.as_str()), scalar(value)))
        .collect::<Vec<_>>();
    format!("{{{}}}", items.join(", "))
}

fn scalar(value: &ScalarValue) -> String {
    match value {
        ScalarValue::Bool(value) => value.to_string(),
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Decimal(value) | ScalarValue::Text(value) => quoted(value),
    }
}

#[cfg(test)]
#[path = "tests/writer_planner_prompt_tests.rs"]
mod tests;
