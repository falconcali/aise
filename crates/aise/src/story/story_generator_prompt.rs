use crate::config::ContextPreparationConfig;
use crate::domain::asset::character_card::DialogueExample;
use crate::domain::asset::constraint::StoryConstraintRequirement;
use crate::domain::asset::ids::{AttributeKey, LocationKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::{BaselineContext, RetrievedCharacterContext, RoleContextView, StoryGeneratorOutput};
use crate::prompt::{
    NarrativeDirectionPromptView, RoleKnowledgePromptView, RuntimePromptVars, TrustedPromptVars,
    WorldKnowledgePromptView, merge_world_knowledge, project_narrative_direction, render_narrative_direction,
    render_relevant_knowledge, render_role_knowledge,
};
use crate::turn::turn_context::TurnExecutionContext;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const STORY_GENERATOR_CSI_SLOT: &str = "context.story_generator.csi";
pub const STORY_GENERATOR_RC_SLOT: &str = "context.story_generator.rc";
pub const STORY_GENERATOR_FTI_SLOT: &str = "context.story_generator.fti";

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorPromptContext {
    pub story_profile: StoryProfilePromptView,
    pub instance_settings: StoryGeneratorInstanceSettingsPromptView,
    pub story_continuity: StoryContinuityPromptView,
    pub player_role: StoryGeneratorRolePromptView,
    pub ai_roles: Vec<StoryGeneratorRolePromptView>,
    pub relevant_knowledge: WorldKnowledgePromptView,
    pub story_goal: BoundedText,
    pub narrative_direction: NarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_decisions: Vec<StoryGeneratorCharacterDecisionPromptView>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryProfilePromptView {
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorInstanceSettingsPromptView {
    pub cast_policy: CastPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryContinuityPromptView {
    pub story_summary: BoundedText,
    pub recent_story: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    pub dialogue_examples: Vec<DialogueExample>,
    pub background: Option<BoundedText>,
    pub state: StoryGeneratorRoleStatePromptView,
    pub knowledge: RoleKnowledgePromptView,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorRoleStatePromptView {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveStoryConstraintPromptView {
    pub constraint_id: String,
    pub kind: ConstraintKindPromptView,
    pub statement: BoundedText,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKindPromptView {
    Require,
    Forbid,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorCharacterDecisionPromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}

pub struct StoryGeneratorPromptProjection {
    pub context: StoryGeneratorPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryGeneratorProjectionError {
    #[error("story generator baseline is missing")]
    MissingBaseline,
    #[error("story generator writer plan is missing")]
    MissingWriterPlan,
    #[error("story generator player input is invalid")]
    InvalidPlayerInput,
    #[error("story generator character decision role is unknown: {role_id}")]
    UnknownDecisionRole { role_id: RoleId },
    #[error("story generator character decision targets player role: {role_id}")]
    PlayerRoleDecision { role_id: RoleId },
    #[error("story generator character decision role is duplicated: {role_id}")]
    DuplicateRoleDecision { role_id: RoleId },
    #[error("story generator required prompt data exceeds budget")]
    RequiredPromptDataExceedsBudget,
    #[error("story generator prompt invariant violated: {code}")]
    Invariant { code: &'static str },
}

pub trait StoryGeneratorPromptContextProjector: Send + Sync {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryGeneratorPromptProjection, StoryGeneratorProjectionError>;
}

pub struct DefaultStoryGeneratorPromptContextProjector {
    config: ContextPreparationConfig,
}

impl DefaultStoryGeneratorPromptContextProjector {
    pub fn new(config: ContextPreparationConfig) -> Self {
        Self { config }
    }
}

impl StoryGeneratorPromptContextProjector for DefaultStoryGeneratorPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryGeneratorPromptProjection, StoryGeneratorProjectionError> {
        let baseline = ctx.baseline().ok_or(StoryGeneratorProjectionError::MissingBaseline)?;
        let plan = ctx.plan().ok_or(StoryGeneratorProjectionError::MissingWriterPlan)?;
        let player_input = BoundedText::try_new(ctx.player_input().to_owned(), "player_input", 4096)
            .map_err(|_| StoryGeneratorProjectionError::InvalidPlayerInput)?;
        let player_role = project_role(
            &baseline.player_role,
            &self.config,
            ctx.retrieved().character(&baseline.player_role.role_id),
        )?;
        let ai_roles = project_ai_roles(ctx, baseline, &self.config)?;
        let character_decisions = project_decisions(ctx, baseline, &ai_roles)?;
        let story_profile = StoryProfilePromptView {
            language: baseline.story_profile.language.clone(),
            genre: baseline.story_profile.genre.clone(),
            themes: baseline.story_profile.themes.clone(),
            tone: baseline.story_profile.style.tone.clone(),
            point_of_view: baseline.story_profile.style.point_of_view.clone(),
            tense: baseline.story_profile.style.tense.clone(),
        };
        let story_continuity = StoryContinuityPromptView {
            story_summary: baseline.story_continuity.summary().text.clone(),
            recent_story: baseline
                .story_continuity
                .recent_segments()
                .iter()
                .map(|segment| segment.text.clone())
                .collect(),
        };
        let relevant_knowledge = merge_world_knowledge(&baseline.relevant_world_knowledge, ctx.retrieved().world())
            .map_err(|_| StoryGeneratorProjectionError::Invariant {
                code: "relevant_knowledge_merge_conflict",
            })?;
        let narrative_direction = ctx
            .narrative_projection()
            .map(|projection| project_narrative_direction(&projection.plan))
            .unwrap_or_default();
        let active_story_constraints = project_constraints(baseline);
        let mut context = StoryGeneratorPromptContext {
            story_profile,
            instance_settings: StoryGeneratorInstanceSettingsPromptView {
                cast_policy: baseline.instance_settings.cast_policy,
            },
            story_continuity,
            player_role,
            ai_roles,
            relevant_knowledge,
            story_goal: plan.story_goal.summary.clone(),
            narrative_direction,
            active_story_constraints,
            character_decisions,
            player_input,
        };
        let rc_vars = prune_dialogue_examples_to_budget(&mut context, ctx.budget().max_context_tokens(), 0)?;
        let output_schema = StoryGeneratorOutput::json_schema(ctx.budget().max_story_text_bytes());
        let fti_vars = TrustedPromptVars::new(HashMap::from([(
            "output_schema".into(),
            Value::String(output_schema.to_string()),
        )]));
        Ok(StoryGeneratorPromptProjection {
            context,
            rc_vars,
            fti_vars,
        })
    }
}

fn project_ai_roles(
    ctx: &TurnExecutionContext,
    baseline: &BaselineContext,
    config: &ContextPreparationConfig,
) -> Result<Vec<StoryGeneratorRolePromptView>, StoryGeneratorProjectionError> {
    let snapshot = ctx.snapshot().ok_or(StoryGeneratorProjectionError::MissingBaseline)?;
    let mut role_ids: BTreeSet<RoleId> = baseline.relevant_roles.iter().map(|role| role.role_id.clone()).collect();
    if let Some(plan) = ctx.plan() {
        role_ids.extend(plan.character_think_requests.iter().map(|request| request.role_id.clone()));
    }
    if let Some(projection) = ctx.narrative_projection() {
        role_ids.extend(
            projection
                .plan
                .character_impulses
                .iter()
                .map(|impulse| impulse.target_role_id.clone()),
        );
    }
    role_ids.remove(&baseline.player_role.role_id);
    role_ids
        .into_iter()
        .filter_map(|role_id| snapshot.role(&role_id).map(RoleContextView::from))
        .map(|role| project_role(&role, config, ctx.retrieved().character(&role.role_id)))
        .collect()
}

fn project_role(
    role: &RoleContextView,
    config: &ContextPreparationConfig,
    retrieved: Option<&RetrievedCharacterContext>,
) -> Result<StoryGeneratorRolePromptView, StoryGeneratorProjectionError> {
    if let Some(retrieved_role) = retrieved.and_then(|character| character.role.as_ref())
        && (retrieved_role.role_label != role.role_label || retrieved_role.profile.name != role.profile.name)
    {
        return Err(StoryGeneratorProjectionError::Invariant {
            code: "character_role_view_conflict",
        });
    }
    let knowledge = retrieved
        .map(|character| RoleKnowledgePromptView {
            known_rumors: character.known_rumors.iter().map(|item| item.content.clone()).collect(),
            memories: character.memories.iter().map(|item| item.content.clone()).collect(),
        })
        .unwrap_or_default();
    Ok(StoryGeneratorRolePromptView {
        role_id: role.role_id.clone(),
        name: role.profile.name.clone(),
        role_label: role.role_label.clone(),
        appearance: role.profile.appearance.clone(),
        personality: role.profile.personality.clone(),
        speaking_style: role.profile.speaking_style.clone(),
        dialogue_examples: select_dialogue_examples(&role.profile.dialogue_examples, config),
        background: role.background.clone(),
        state: StoryGeneratorRoleStatePromptView {
            location: role.state.location.clone(),
            goals: role.state.goals.clone(),
            attributes: role.state.attributes.clone(),
        },
        knowledge,
    })
}

fn project_constraints(baseline: &BaselineContext) -> Vec<ActiveStoryConstraintPromptView> {
    let mut constraints = baseline.active_story_constraints.iter().collect::<Vec<_>>();
    constraints.sort_by_key(|constraint| constraint.id.to_string());
    constraints
        .into_iter()
        .map(|constraint| {
            let (kind, statement) = match &constraint.requirement {
                StoryConstraintRequirement::Require { statement } => {
                    (ConstraintKindPromptView::Require, statement.clone())
                }
                StoryConstraintRequirement::Forbid { statement } => {
                    (ConstraintKindPromptView::Forbid, statement.clone())
                }
            };
            ActiveStoryConstraintPromptView {
                constraint_id: constraint.id.to_string(),
                kind,
                statement,
            }
        })
        .collect()
}

fn project_decisions(
    ctx: &TurnExecutionContext,
    baseline: &BaselineContext,
    ai_roles: &[StoryGeneratorRolePromptView],
) -> Result<Vec<StoryGeneratorCharacterDecisionPromptView>, StoryGeneratorProjectionError> {
    let plan = ctx.plan().ok_or(StoryGeneratorProjectionError::MissingWriterPlan)?;
    if ctx.character_decisions().len() != plan.character_think_requests.len() {
        return Err(StoryGeneratorProjectionError::Invariant {
            code: "character_decision_count_mismatch",
        });
    }
    let names = ai_roles
        .iter()
        .map(|role| (role.role_id.clone(), role.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    ctx.character_decisions()
        .iter()
        .zip(&plan.character_think_requests)
        .map(|(decision, request)| {
            if decision.role_id != request.role_id {
                return Err(StoryGeneratorProjectionError::Invariant {
                    code: "character_decision_order_mismatch",
                });
            }
            if decision.role_id == baseline.player_role.role_id {
                return Err(StoryGeneratorProjectionError::PlayerRoleDecision {
                    role_id: decision.role_id.clone(),
                });
            }
            if !seen.insert(decision.role_id.clone()) {
                return Err(StoryGeneratorProjectionError::DuplicateRoleDecision {
                    role_id: decision.role_id.clone(),
                });
            }
            let name = names.get(&decision.role_id).cloned().ok_or_else(|| {
                StoryGeneratorProjectionError::UnknownDecisionRole {
                    role_id: decision.role_id.clone(),
                }
            })?;
            Ok(StoryGeneratorCharacterDecisionPromptView {
                role_id: decision.role_id.clone(),
                name,
                decision: decision.decision.clone(),
                suggested_utterance: decision.suggested_utterance.clone(),
            })
        })
        .collect()
}

pub(crate) fn render_runtime_vars(context: &StoryGeneratorPromptContext) -> RuntimePromptVars {
    RuntimePromptVars::new(HashMap::from([
        (
            "story_profile".into(),
            Value::String(render_story_profile(&context.story_profile)),
        ),
        (
            "instance_settings".into(),
            Value::String(render_instance_settings(&context.instance_settings)),
        ),
        (
            "story_summary".into(),
            Value::String(render_story_summary(context.story_continuity.story_summary.as_str())),
        ),
        (
            "recent_story".into(),
            Value::String(render_recent_story(&context.story_continuity.recent_story)),
        ),
        (
            "player_character".into(),
            Value::String(render_role(&context.player_role, None)),
        ),
        ("ai_characters".into(), Value::String(render_roles(&context.ai_roles))),
        (
            "active_story_constraints".into(),
            Value::String(render_constraints(&context.active_story_constraints)),
        ),
        ("story_goal".into(), Value::String(quoted(context.story_goal.as_str()))),
        (
            "narrative_direction".into(),
            Value::String(render_narrative_direction(&context.narrative_direction)),
        ),
        (
            "relevant_knowledge".into(),
            Value::String(render_relevant_knowledge(&context.relevant_knowledge)),
        ),
        (
            "character_decisions".into(),
            Value::String(render_decisions(&context.character_decisions)),
        ),
        ("player_input".into(), Value::String(quoted(context.player_input.as_str()))),
    ]))
}

pub(crate) fn prune_dialogue_examples_to_budget(
    context: &mut StoryGeneratorPromptContext,
    max_tokens: u64,
    extra_tokens: u64,
) -> Result<RuntimePromptVars, StoryGeneratorProjectionError> {
    let mut vars = render_runtime_vars(context);
    let mut role_ids = std::iter::once(context.player_role.role_id.clone())
        .chain(context.ai_roles.iter().map(|role| role.role_id.clone()))
        .collect::<Vec<_>>();
    role_ids.sort_by(|left, right| right.cmp(left));
    for role_id in role_ids {
        while runtime_tokens(&vars).saturating_add(extra_tokens) > max_tokens {
            let examples = if context.player_role.role_id == role_id {
                &mut context.player_role.dialogue_examples
            } else if let Some(role) = context.ai_roles.iter_mut().find(|role| role.role_id == role_id) {
                &mut role.dialogue_examples
            } else {
                break;
            };
            if examples.pop().is_none() {
                break;
            }
            vars = render_runtime_vars(context);
        }
    }
    if runtime_tokens(&vars).saturating_add(extra_tokens) > max_tokens {
        return Err(StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget);
    }
    Ok(vars)
}

fn select_dialogue_examples(examples: &[DialogueExample], config: &ContextPreparationConfig) -> Vec<DialogueExample> {
    let mut tokens = 0u64;
    examples
        .iter()
        .take(config.max_dialogue_examples_per_role)
        .filter_map(|example| {
            let cost = estimate_text_tokens(example.situation.as_str())
                .saturating_add(estimate_text_tokens(example.response.as_str()));
            let next = tokens.saturating_add(cost);
            if next > config.max_dialogue_example_tokens_per_role {
                None
            } else {
                tokens = next;
                Some(example.clone())
            }
        })
        .collect()
}

fn runtime_tokens(vars: &RuntimePromptVars) -> u64 {
    vars.as_map()
        .values()
        .filter_map(Value::as_str)
        .map(estimate_text_tokens)
        .fold(0u64, u64::saturating_add)
}

fn render_story_profile(value: &StoryProfilePromptView) -> String {
    let mut lines = vec![format!("language: {}", quoted(value.language.as_str()))];
    if !value.genre.is_empty() {
        lines.push(format!("genre: {}", quoted_list(&value.genre)));
    }
    if !value.themes.is_empty() {
        lines.push(format!("themes: {}", quoted_list(&value.themes)));
    }
    if !value.tone.is_empty() {
        lines.push(format!("tone: {}", quoted_list(&value.tone)));
    }
    lines.push(format!("point_of_view: {}", quoted(value.point_of_view.as_str())));
    lines.push(format!("tense: {}", quoted(value.tense.as_str())));
    lines.join("\n")
}

fn render_instance_settings(value: &StoryGeneratorInstanceSettingsPromptView) -> String {
    let policy = match value.cast_policy {
        CastPolicy::Open => "open",
        CastPolicy::IncidentalOnly => "incidental_only",
        CastPolicy::Closed => "closed",
    };
    format!("cast_policy: {policy}")
}

fn render_recent_story(values: &[BoundedText]) -> String {
    values.iter().map(|value| value.as_str()).collect::<Vec<_>>().join("\n\n")
}

fn render_roles(values: &[StoryGeneratorRolePromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| render_role(value, Some("- ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_role(value: &StoryGeneratorRolePromptView, prefix: Option<&str>) -> String {
    let collection = prefix.is_some();
    let first = prefix.unwrap_or_default();
    let rest = if collection { "  " } else { "" };
    let dialogue_examples = value
        .dialogue_examples
        .iter()
        .map(|example| {
            format!(
                "{rest}- situation: {}\n{rest}  response: {}",
                quoted(example.situation.as_str()),
                quoted(example.response.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines = vec![
        format!("{first}role_id: {}", quoted(value.role_id.as_str())),
        format!("{rest}name: {}", quoted(value.name.as_str())),
    ];
    if value.role_label != value.name {
        lines.push(format!("{rest}role: {}", quoted(value.role_label.as_str())));
    }
    push_optional(&mut lines, rest, "appearance", value.appearance.as_ref());
    push_optional(&mut lines, rest, "personality", value.personality.as_ref());
    push_optional(&mut lines, rest, "speaking_style", value.speaking_style.as_ref());
    if !dialogue_examples.is_empty() {
        lines.push(format!("{rest}dialogue_examples:\n{dialogue_examples}"));
    }
    push_optional(&mut lines, rest, "background", value.background.as_ref());
    lines.push(format!("{rest}location: {}", quoted(value.state.location.as_str())));
    if !value.state.goals.is_empty() {
        lines.push(format!("{rest}goals: {}", quoted_list(&value.state.goals)));
    }
    if !value.state.attributes.is_empty() {
        lines.push(format!("{rest}attributes: {}", render_attributes(&value.state.attributes)));
    }
    if !value.knowledge.known_rumors.is_empty() || !value.knowledge.memories.is_empty() {
        lines.push(render_role_knowledge(&value.knowledge));
    }
    lines.join("\n")
}

fn render_attributes(values: &BTreeMap<AttributeKey, ScalarValue>) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("{}: {}", quoted(key.as_str()), render_scalar(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_constraints(values: &[ActiveStoryConstraintPromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| {
            let kind = match value.kind {
                ConstraintKindPromptView::Require => "require",
                ConstraintKindPromptView::Forbid => "forbid",
            };
            format!(
                "- constraint_id: {}\n  kind: {kind}\n  statement: {}",
                quoted(&value.constraint_id),
                quoted(value.statement.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_decisions(values: &[StoryGeneratorCharacterDecisionPromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| {
            let mut lines = vec![
                format!("- role_id: {}", quoted(value.role_id.as_str())),
                format!("  name: {}", quoted(value.name.as_str())),
                format!("  decision: {}", quoted(value.decision.as_str())),
            ];
            if let Some(utterance) = non_empty(value.suggested_utterance.as_ref()) {
                lines.push(format!("  suggested_utterance: {}", quoted(utterance)));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_empty(value: Option<&BoundedText>) -> Option<&str> {
    value.map(BoundedText::as_str).filter(|value| !value.trim().is_empty())
}

fn render_story_summary(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        value.to_owned()
    }
}

fn push_optional(lines: &mut Vec<String>, indent: &str, name: &str, value: Option<&BoundedText>) {
    if let Some(value) = value {
        lines.push(format!("{indent}{name}: {}", quoted(value.as_str())));
    }
}

fn quoted_list(values: &[BoundedText]) -> String {
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn render_scalar(value: &ScalarValue) -> String {
    match value {
        ScalarValue::Bool(value) => value.to_string(),
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Decimal(value) | ScalarValue::Text(value) => quoted(value),
    }
}

#[cfg(test)]
#[path = "tests/story_generator_prompt_tests.rs"]
mod tests;
