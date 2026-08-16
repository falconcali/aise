use crate::config::ContextPreparationConfig;
use crate::domain::asset::character_card::DialogueExample;
use crate::domain::asset::constraint::StoryConstraintRequirement;
use crate::domain::asset::ids::{AttributeKey, LocationKey, SceneKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::role::RoleController;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::{BaselineContext, RetrievalAudience, RoleContextView, StoryGeneratorOutput};
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use crate::turn::turn_context::TurnExecutionContext;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

pub const STORY_GENERATOR_CSI_SLOT: &str = "context.story_generator.csi";
pub const STORY_GENERATOR_RC_SLOT: &str = "context.story_generator.rc";
pub const STORY_GENERATOR_FTI_SLOT: &str = "context.story_generator.fti";

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorPromptContext {
    pub story_profile: StoryProfilePromptView,
    pub instance_settings: Option<StoryGeneratorInstanceSettingsPromptView>,
    pub story_continuity: StoryContinuityPromptView,
    pub current_scene: StoryGeneratorScenePromptView,
    pub player_role: StoryGeneratorRolePromptView,
    pub ai_roles: Vec<StoryGeneratorRolePromptView>,
    pub relevant_writer_knowledge: Vec<StoryGeneratorKnowledgePromptView>,
    pub story_goal: BoundedText,
    pub narrative_direction: StoryGeneratorNarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_decisions: Vec<StoryGeneratorCharacterDecisionPromptView>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryProfilePromptView {
    pub premise: BoundedText,
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
    pub recent_story: Vec<RecentStoryPromptView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentStoryPromptView {
    pub sequence: u64,
    pub text: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorScenePromptView {
    pub scene_key: Option<SceneKey>,
    pub location: BoundedText,
    pub time: BoundedText,
    pub situation: BoundedText,
    pub present_role_ids: Vec<RoleId>,
    pub observable_conditions: Vec<BoundedText>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterPresence {
    Scene,
    Referenced,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub presence: CharacterPresence,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    pub dialogue_examples: Vec<DialogueExample>,
    pub background: Option<BoundedText>,
    pub state: StoryGeneratorRoleStatePromptView,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorRoleStatePromptView {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorKnowledgePromptView {
    pub entry_id: Option<KnowledgeSourceId>,
    pub title: Option<BoundedText>,
    pub kind: KnowledgeKind,
    pub scope: KnowledgeScopePromptView,
    pub content: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnowledgeScopePromptView {
    ObjectiveWorld,
    PublicClaim,
    CharacterMemory { owner: RoleId },
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorNarrativeDirectionPromptView {
    pub active_goals: Vec<BoundedText>,
    pub event_intents: Vec<BoundedText>,
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
        let player_role = project_role(&baseline.player_role, CharacterPresence::Scene, &self.config)?;
        let ai_roles = project_ai_roles(baseline, &self.config)?;
        let character_decisions = project_decisions(ctx, baseline, &ai_roles)?;
        let story_profile = StoryProfilePromptView {
            premise: baseline.story_profile.premise.clone(),
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
                .map(|segment| RecentStoryPromptView {
                    sequence: segment.sequence.get(),
                    text: segment.text.clone(),
                })
                .collect(),
        };
        let current_scene = StoryGeneratorScenePromptView {
            scene_key: Some(baseline.current_scene.scene_key.clone()),
            location: bounded_key(
                baseline.current_scene.location_key.as_str(),
                "scene_location",
                ctx.budget().max_item_bytes(),
            )?,
            time: baseline.current_scene.time.clone(),
            situation: baseline.current_scene.description.clone(),
            present_role_ids: baseline.current_scene.present_role_ids.clone(),
            observable_conditions: Vec::new(),
        };
        let relevant_writer_knowledge = project_writer_knowledge(ctx)?;
        let narrative_projection = ctx.narrative_projection();
        let narrative_direction = StoryGeneratorNarrativeDirectionPromptView {
            active_goals: narrative_projection
                .map(|projection| projection.plan.active_directions.as_slice())
                .unwrap_or(&[])
                .iter()
                .map(|direction| direction.dramatic_focus.clone())
                .collect(),
            event_intents: narrative_projection
                .map(|projection| projection.plan.world_event_intents.as_slice())
                .unwrap_or(&[])
                .iter()
                .map(|intent| intent.description.clone())
                .collect(),
        };
        let active_story_constraints = project_constraints(baseline);
        let mut context = StoryGeneratorPromptContext {
            story_profile,
            instance_settings: Some(StoryGeneratorInstanceSettingsPromptView {
                cast_policy: baseline.instance_settings.cast_policy,
            }),
            story_continuity,
            current_scene,
            player_role,
            ai_roles,
            relevant_writer_knowledge,
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

fn bounded_key(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<BoundedText, StoryGeneratorProjectionError> {
    BoundedText::try_new(value.to_owned(), field, max_bytes).map_err(|_| StoryGeneratorProjectionError::Invariant {
        code: "invalid_key_text",
    })
}

fn project_ai_roles(
    baseline: &BaselineContext,
    config: &ContextPreparationConfig,
) -> Result<Vec<StoryGeneratorRolePromptView>, StoryGeneratorProjectionError> {
    let mut seen = HashSet::new();
    baseline
        .scene_roles
        .iter()
        .map(|role| (role, CharacterPresence::Scene))
        .chain(
            baseline
                .referenced_roles
                .iter()
                .map(|role| (role, CharacterPresence::Referenced)),
        )
        .filter(|(role, _)| {
            role.role_id != baseline.player_role.role_id
                && matches!(role.controller, RoleController::Ai)
                && seen.insert(role.role_id.clone())
        })
        .map(|(role, presence)| project_role(role, presence, config))
        .collect()
}

fn project_role(
    role: &RoleContextView,
    presence: CharacterPresence,
    config: &ContextPreparationConfig,
) -> Result<StoryGeneratorRolePromptView, StoryGeneratorProjectionError> {
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
        presence,
    })
}

fn project_writer_knowledge(
    ctx: &TurnExecutionContext,
) -> Result<Vec<StoryGeneratorKnowledgePromptView>, StoryGeneratorProjectionError> {
    ctx.retrieved()
        .writer()
        .iter()
        .map(|item| {
            if item.provenance.audience != RetrievalAudience::GlobalWriter {
                return Err(StoryGeneratorProjectionError::Invariant {
                    code: "writer_knowledge_audience_invalid",
                });
            }
            let scope = match item.provenance.knowledge_kind {
                KnowledgeKind::Fact if item.provenance.memory_owner.is_none() => {
                    KnowledgeScopePromptView::ObjectiveWorld
                }
                KnowledgeKind::Rumor if item.provenance.memory_owner.is_none() => KnowledgeScopePromptView::PublicClaim,
                KnowledgeKind::Memory => item
                    .provenance
                    .memory_owner
                    .clone()
                    .map(|owner| KnowledgeScopePromptView::CharacterMemory { owner })
                    .ok_or(StoryGeneratorProjectionError::Invariant {
                        code: "writer_memory_owner_missing",
                    })?,
                _ => {
                    return Err(StoryGeneratorProjectionError::Invariant {
                        code: "writer_knowledge_scope_invalid",
                    });
                }
            };
            Ok(StoryGeneratorKnowledgePromptView {
                entry_id: Some(item.provenance.source_id.clone()),
                title: None,
                kind: item.provenance.knowledge_kind,
                scope,
                content: item.content.clone(),
            })
        })
        .collect()
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
            Value::String(render_instance_settings(context.instance_settings.as_ref())),
        ),
        (
            "story_summary".into(),
            Value::String(render_optional_text(context.story_continuity.story_summary.as_str())),
        ),
        (
            "recent_story".into(),
            Value::String(render_recent_story(&context.story_continuity.recent_story)),
        ),
        ("current_scene".into(), Value::String(render_scene(&context.current_scene))),
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
            "relevant_writer_knowledge".into(),
            Value::String(render_knowledge(&context.relevant_writer_knowledge)),
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
    [
        format!("premise: {}", quoted(value.premise.as_str())),
        format!("language: {}", quoted(value.language.as_str())),
        format!("genre: {}", quoted_list(&value.genre)),
        format!("themes: {}", quoted_list(&value.themes)),
        format!("tone: {}", quoted_list(&value.tone)),
        format!("point_of_view: {}", quoted(value.point_of_view.as_str())),
        format!("tense: {}", quoted(value.tense.as_str())),
    ]
    .join("\n")
}

fn render_instance_settings(value: Option<&StoryGeneratorInstanceSettingsPromptView>) -> String {
    let Some(value) = value else {
        return "None.".into();
    };
    let policy = match value.cast_policy {
        CastPolicy::Open => "open",
        CastPolicy::IncidentalOnly => "incidental_only",
        CastPolicy::Closed => "closed",
    };
    format!("cast_policy: {policy}")
}

fn render_recent_story(values: &[RecentStoryPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| format!("- sequence: {}\n  text: {}", value.sequence, quoted(value.text.as_str())))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_scene(value: &StoryGeneratorScenePromptView) -> String {
    [
        format!(
            "scene_key: {}",
            value
                .scene_key
                .as_ref()
                .map(|key| quoted(key.as_str()))
                .unwrap_or_else(|| "None.".into())
        ),
        format!("location: {}", quoted(value.location.as_str())),
        format!("time: {}", quoted(value.time.as_str())),
        format!("situation: {}", quoted(value.situation.as_str())),
        format!("present_role_ids: {}", id_list(&value.present_role_ids)),
        format!("observable_conditions: {}", quoted_list(&value.observable_conditions)),
    ]
    .join("\n")
}

fn render_roles(values: &[StoryGeneratorRolePromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| render_role(value, Some("- ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_role(value: &StoryGeneratorRolePromptView, prefix: Option<&str>) -> String {
    let presence = match value.presence {
        CharacterPresence::Scene => "scene",
        CharacterPresence::Referenced => "referenced",
    };
    let collection = prefix.is_some();
    let first = prefix.unwrap_or_default();
    let rest = if collection { "  " } else { "" };
    let attributes = if value.state.attributes.is_empty() {
        "None.".into()
    } else {
        format!(
            "{{{}}}",
            value
                .state
                .attributes
                .iter()
                .map(|(key, value)| format!("{}: {}", quoted(key.as_str()), render_scalar(value)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
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
    if collection {
        lines.push(format!("{rest}presence: {presence}"));
    }
    push_optional(&mut lines, rest, "appearance", value.appearance.as_ref());
    push_optional(&mut lines, rest, "personality", value.personality.as_ref());
    push_optional(&mut lines, rest, "speaking_style", value.speaking_style.as_ref());
    if !dialogue_examples.is_empty() {
        lines.push(format!("{rest}dialogue_examples:\n{dialogue_examples}"));
    }
    push_optional(&mut lines, rest, "background", value.background.as_ref());
    lines.push(format!("{rest}location: {}", quoted(value.state.location.as_str())));
    lines.push(format!("{rest}goals: {}", quoted_list(&value.state.goals)));
    lines.push(format!("{rest}attributes: {attributes}"));
    lines.join("\n")
}

fn render_constraints(values: &[ActiveStoryConstraintPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
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

fn render_narrative_direction(value: &StoryGeneratorNarrativeDirectionPromptView) -> String {
    if value.active_goals.is_empty() && value.event_intents.is_empty() {
        return "None.".into();
    }
    format!(
        "active_goals: {}\nevent_intents: {}",
        quoted_list(&value.active_goals),
        quoted_list(&value.event_intents)
    )
}

fn render_knowledge(values: &[StoryGeneratorKnowledgePromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            let kind = match value.kind {
                KnowledgeKind::Fact => "fact",
                KnowledgeKind::Rumor => "rumor",
                KnowledgeKind::Memory => "memory",
            };
            let scope = match &value.scope {
                KnowledgeScopePromptView::ObjectiveWorld => "objective_world".into(),
                KnowledgeScopePromptView::PublicClaim => "public_claim".into(),
                KnowledgeScopePromptView::CharacterMemory { owner } => {
                    format!("character_memory:{}", quoted(owner.as_str()))
                }
            };
            let entry_id = value
                .entry_id
                .as_ref()
                .map(|id| quoted(id.as_str()))
                .unwrap_or_else(|| "None.".into());
            let title = value
                .title
                .as_ref()
                .map(|title| quoted(title.as_str()))
                .unwrap_or_else(|| "None.".into());
            format!(
                "- entry_id: {entry_id}\n  title: {title}\n  kind: {kind}\n  scope: {scope}\n  content: {}",
                quoted(value.content.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_decisions(values: &[StoryGeneratorCharacterDecisionPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            let suggested_utterance = value
                .suggested_utterance
                .as_ref()
                .map(|value| quoted(value.as_str()))
                .unwrap_or_else(|| "None.".into());
            format!(
                "- role_id: {}\n  name: {}\n  decision: {}\n  suggested_utterance: {suggested_utterance}",
                quoted(value.role_id.as_str()),
                quoted(value.name.as_str()),
                quoted(value.decision.as_str()),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_optional_text(value: &str) -> String {
    if value.trim().is_empty() {
        "None.".into()
    } else {
        quoted(value)
    }
}

fn push_optional(lines: &mut Vec<String>, indent: &str, name: &str, value: Option<&BoundedText>) {
    if let Some(value) = value {
        lines.push(format!("{indent}{name}: {}", quoted(value.as_str())));
    }
}

fn quoted_list(values: &[BoundedText]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn id_list(values: &[RoleId]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
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
