use crate::domain::asset::constraint::StoryConstraintRequirement;
use crate::domain::asset::ids::SceneKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::binding::RoleController;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::{BaselineContext, CharacterView, RetrievalAudience, StoryGeneratorOutput};
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
    pub player_character: StoryGeneratorCharacterPromptView,
    pub ai_characters: Vec<StoryGeneratorCharacterPromptView>,
    pub relevant_writer_knowledge: Vec<StoryGeneratorKnowledgePromptView>,
    pub story_goal: BoundedText,
    pub narrative_direction: StoryGeneratorNarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_thoughts: Vec<StoryGeneratorCharacterThoughtPromptView>,
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
    pub present_character_ids: Vec<CharacterId>,
    pub observable_conditions: Vec<BoundedText>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterControl {
    Player,
    Ai,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterPresence {
    Present,
    DirectParticipant,
    Referenced,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorCharacterPromptView {
    pub character_id: CharacterId,
    pub name: BoundedText,
    pub control: CharacterControl,
    pub story_role: Option<BoundedText>,
    pub profile: CharacterProfilePromptView,
    pub state: CharacterStatePromptView,
    pub presence: CharacterPresence,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterProfilePromptView {
    pub description: BoundedText,
    pub personality: Vec<BoundedText>,
    pub values: Vec<BoundedText>,
    pub fears: Vec<BoundedText>,
    pub speaking_register: BoundedText,
    pub speaking_verbosity: BoundedText,
    pub speaking_traits: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterStatePromptView {
    pub location: BoundedText,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<String, ScalarValue>,
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
    CharacterMemory { owner: CharacterId },
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
pub struct StoryGeneratorCharacterThoughtPromptView {
    pub character_id: CharacterId,
    pub name: BoundedText,
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
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
    #[error("story generator character thought target is unknown: {character_id}")]
    UnknownThoughtCharacter { character_id: CharacterId },
    #[error("story generator character thought targets player character: {character_id}")]
    PlayerCharacterThought { character_id: CharacterId },
    #[error("story generator character thought is duplicated: {character_id}")]
    DuplicateCharacterThought { character_id: CharacterId },
    #[error("story generator required prompt data exceeds budget: {section}")]
    RequiredPromptDataExceedsBudget { section: &'static str },
    #[error("story generator prompt invariant violated: {code}")]
    Invariant { code: &'static str },
}

pub trait StoryGeneratorPromptContextProjector: Send + Sync {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryGeneratorPromptProjection, StoryGeneratorProjectionError>;
}

pub struct DefaultStoryGeneratorPromptContextProjector;

impl StoryGeneratorPromptContextProjector for DefaultStoryGeneratorPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryGeneratorPromptProjection, StoryGeneratorProjectionError> {
        let baseline = ctx.baseline().ok_or(StoryGeneratorProjectionError::MissingBaseline)?;
        let plan = ctx.plan().ok_or(StoryGeneratorProjectionError::MissingWriterPlan)?;
        let player_input = BoundedText::try_new(ctx.player_input().to_owned(), "player_input", 4096)
            .map_err(|_| StoryGeneratorProjectionError::InvalidPlayerInput)?;
        let player_character = project_character(
            &baseline.player_character,
            CharacterControl::Player,
            CharacterPresence::Present,
            ctx.budget().max_item_bytes(),
        )?;
        let ai_characters = project_ai_characters(baseline, ctx.budget().max_item_bytes())?;
        let character_thoughts = project_thoughts(ctx, baseline, &ai_characters)?;
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
            present_character_ids: baseline.current_scene.present_character_ids.clone(),
            observable_conditions: Vec::new(),
        };
        let relevant_writer_knowledge = project_writer_knowledge(ctx)?;
        let narrative_direction = StoryGeneratorNarrativeDirectionPromptView {
            active_goals: plan
                .narrative_plan
                .active_goals
                .iter()
                .map(|goal| goal.summary.clone())
                .collect(),
            event_intents: plan
                .narrative_plan
                .global_event_intents
                .iter()
                .map(|intent| intent.description.clone())
                .collect(),
        };
        let active_story_constraints = project_constraints(baseline);
        let context = StoryGeneratorPromptContext {
            story_profile,
            instance_settings: Some(StoryGeneratorInstanceSettingsPromptView {
                cast_policy: baseline.instance_settings.cast_policy,
            }),
            story_continuity,
            current_scene,
            player_character,
            ai_characters,
            relevant_writer_knowledge,
            story_goal: plan.story_goal.summary.clone(),
            narrative_direction,
            active_story_constraints,
            character_thoughts,
            player_input,
        };
        let rc_vars = render_runtime_vars(&context);
        let input_tokens = rc_vars
            .as_map()
            .values()
            .filter_map(Value::as_str)
            .map(estimate_text_tokens)
            .fold(0u64, u64::saturating_add);
        if input_tokens > ctx.budget().max_context_tokens() {
            return Err(StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget {
                section: "runtime_context",
            });
        }
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

fn project_ai_characters(
    baseline: &BaselineContext,
    max_item_bytes: usize,
) -> Result<Vec<StoryGeneratorCharacterPromptView>, StoryGeneratorProjectionError> {
    let mut seen = HashSet::new();
    baseline
        .scene_characters
        .iter()
        .map(|character| {
            let presence = if baseline.current_scene.present_character_ids.contains(&character.character_id) {
                CharacterPresence::Present
            } else {
                CharacterPresence::DirectParticipant
            };
            (character, presence)
        })
        .chain(
            baseline
                .referenced_characters
                .iter()
                .map(|character| (character, CharacterPresence::Referenced)),
        )
        .filter(|(character, _)| {
            character.character_id != baseline.player_character.character_id
                && matches!(character.binding.controller, RoleController::Ai)
                && seen.insert(character.character_id.clone())
        })
        .map(|(character, presence)| project_character(character, CharacterControl::Ai, presence, max_item_bytes))
        .collect()
}

fn project_character(
    character: &CharacterView,
    control: CharacterControl,
    presence: CharacterPresence,
    max_item_bytes: usize,
) -> Result<StoryGeneratorCharacterPromptView, StoryGeneratorProjectionError> {
    Ok(StoryGeneratorCharacterPromptView {
        character_id: character.character_id.clone(),
        name: character.card.meta.name.clone(),
        control,
        story_role: Some(character.role.role_label.clone()),
        profile: CharacterProfilePromptView {
            description: character.card.profile.description.clone(),
            personality: character.card.profile.personality.clone(),
            values: character.card.profile.values.clone(),
            fears: character.card.profile.fears.clone(),
            speaking_register: character.card.profile.speaking_style.register.clone(),
            speaking_verbosity: character.card.profile.speaking_style.verbosity.clone(),
            speaking_traits: character.card.profile.speaking_style.traits.clone(),
        },
        state: CharacterStatePromptView {
            location: bounded_key(character.state.location.as_str(), "character_location", max_item_bytes)?,
            goals: character.state.goals.clone(),
            attributes: character
                .state
                .attributes
                .iter()
                .map(|(key, value)| (key.as_str().to_owned(), value.clone()))
                .collect(),
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

fn project_thoughts(
    ctx: &TurnExecutionContext,
    baseline: &BaselineContext,
    ai_characters: &[StoryGeneratorCharacterPromptView],
) -> Result<Vec<StoryGeneratorCharacterThoughtPromptView>, StoryGeneratorProjectionError> {
    let plan = ctx.plan().ok_or(StoryGeneratorProjectionError::MissingWriterPlan)?;
    if ctx.thoughts().len() != plan.character_think_requests.len() {
        return Err(StoryGeneratorProjectionError::Invariant {
            code: "character_thought_count_mismatch",
        });
    }
    let names = ai_characters
        .iter()
        .map(|character| (character.character_id.clone(), character.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    ctx.thoughts()
        .iter()
        .zip(&plan.character_think_requests)
        .map(|(thought, request)| {
            if thought.character_id != request.character_id {
                return Err(StoryGeneratorProjectionError::Invariant {
                    code: "character_thought_order_mismatch",
                });
            }
            if thought.character_id == baseline.player_character.character_id {
                return Err(StoryGeneratorProjectionError::PlayerCharacterThought {
                    character_id: thought.character_id.clone(),
                });
            }
            if !seen.insert(thought.character_id.clone()) {
                return Err(StoryGeneratorProjectionError::DuplicateCharacterThought {
                    character_id: thought.character_id.clone(),
                });
            }
            let name = names.get(&thought.character_id).cloned().ok_or_else(|| {
                StoryGeneratorProjectionError::UnknownThoughtCharacter {
                    character_id: thought.character_id.clone(),
                }
            })?;
            Ok(StoryGeneratorCharacterThoughtPromptView {
                character_id: thought.character_id.clone(),
                name,
                perception: thought.perception.clone(),
                emotion: thought.emotion.clone(),
                goal: thought.goal.clone(),
                possible_action: thought.possible_action.clone(),
            })
        })
        .collect()
}

fn render_runtime_vars(context: &StoryGeneratorPromptContext) -> RuntimePromptVars {
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
            Value::String(render_character(&context.player_character, None)),
        ),
        ("ai_characters".into(), Value::String(render_characters(&context.ai_characters))),
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
            "character_thoughts".into(),
            Value::String(render_thoughts(&context.character_thoughts)),
        ),
        ("player_input".into(), Value::String(quoted(context.player_input.as_str()))),
    ]))
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
        format!("present_character_ids: {}", id_list(&value.present_character_ids)),
        format!("observable_conditions: {}", quoted_list(&value.observable_conditions)),
    ]
    .join("\n")
}

fn render_characters(values: &[StoryGeneratorCharacterPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| render_character(value, Some("- ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_character(value: &StoryGeneratorCharacterPromptView, prefix: Option<&str>) -> String {
    let control = match value.control {
        CharacterControl::Player => "player",
        CharacterControl::Ai => "ai",
    };
    let presence = match value.presence {
        CharacterPresence::Present => "present",
        CharacterPresence::DirectParticipant => "direct_participant",
        CharacterPresence::Referenced => "referenced",
    };
    let role = value
        .story_role
        .as_ref()
        .map(|role| quoted(role.as_str()))
        .unwrap_or_else(|| "None.".into());
    let attributes = if value.state.attributes.is_empty() {
        "None.".into()
    } else {
        format!(
            "{{{}}}",
            value
                .state
                .attributes
                .iter()
                .map(|(key, value)| format!("{}: {}", quoted(key), render_scalar(value)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    [
        format!(
            "{}character_id: {}",
            prefix.unwrap_or_default(),
            quoted(value.character_id.as_str())
        ),
        format!("  name: {}", quoted(value.name.as_str())),
        format!("  control: {control}"),
        format!("  presence: {presence}"),
        format!("  story_role: {role}"),
        format!("  description: {}", quoted(value.profile.description.as_str())),
        format!("  personality: {}", quoted_list(&value.profile.personality)),
        format!("  values: {}", quoted_list(&value.profile.values)),
        format!("  fears: {}", quoted_list(&value.profile.fears)),
        format!("  speaking_register: {}", quoted(value.profile.speaking_register.as_str())),
        format!("  speaking_verbosity: {}", quoted(value.profile.speaking_verbosity.as_str())),
        format!("  speaking_traits: {}", quoted_list(&value.profile.speaking_traits)),
        format!("  location: {}", quoted(value.state.location.as_str())),
        format!("  goals: {}", quoted_list(&value.state.goals)),
        format!("  attributes: {attributes}"),
    ]
    .join("\n")
    .trim_start()
    .to_owned()
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

fn render_thoughts(values: &[StoryGeneratorCharacterThoughtPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            format!(
                "- character_id: {}\n  name: {}\n  perception: {}\n  emotion: {}\n  goal: {}\n  possible_action: {}",
                quoted(value.character_id.as_str()),
                quoted(value.name.as_str()),
                quoted(value.perception.as_str()),
                quoted(value.emotion.as_str()),
                quoted(value.goal.as_str()),
                quoted(value.possible_action.as_str())
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

fn quoted_list(values: &[BoundedText]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn id_list(values: &[CharacterId]) -> String {
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
