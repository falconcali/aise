use crate::config::CharacterThinkConfig;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::narrative_graph::effect::ImpulseUrgency;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::{CharacterThinkRequest, RetrievalAudience};
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use crate::turn::turn_context::TurnExecutionContext;
use serde_json::{Value, json};
use std::collections::HashMap;

pub const CHARACTER_THINK_CSI_SLOT: &str = "context.character_think.csi";
pub const CHARACTER_THINK_RC_SLOT: &str = "context.character_think.rc";
pub const CHARACTER_THINK_FTI_SLOT: &str = "context.character_think.fti";

#[derive(Debug, Clone)]
pub struct CharacterThinkPromptContext {
    pub target_character: CharacterThinkCharacterPromptView,
    pub current_character_state: CharacterThinkStatePromptView,
    pub story_continuity: CharacterThinkStoryContinuityPromptView,
    pub current_scene: CharacterThinkScenePromptView,
    pub relevant_character_knowledge: Vec<CharacterThinkKnowledgePromptView>,
    pub narrative_character_impulses: Vec<CharacterThinkImpulsePromptView>,
    pub thinking_focus: BoundedText,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkStoryContinuityPromptView {
    pub story_summary: BoundedText,
    pub recent_story: Vec<BoundedText>,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkCharacterPromptView {
    pub character_id: CharacterId,
    pub name: BoundedText,
    pub description: Option<BoundedText>,
    pub personality: Vec<BoundedText>,
    pub values: Vec<BoundedText>,
    pub fears: Vec<BoundedText>,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkStatePromptView {
    pub location: Option<BoundedText>,
    pub goals: Vec<BoundedText>,
    pub relevant_attributes: Vec<CharacterStateAttributePromptView>,
}

#[derive(Debug, Clone)]
pub struct CharacterStateAttributePromptView {
    pub name: BoundedText,
    pub value: ScalarValue,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkScenePromptView {
    pub location: Option<BoundedText>,
    pub time: Option<BoundedText>,
    pub situation: Option<BoundedText>,
    pub observable_conditions: Vec<BoundedText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterThinkKnowledgeKind {
    Rumor,
    Memory,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkKnowledgePromptView {
    pub kind: CharacterThinkKnowledgeKind,
    pub content: BoundedText,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkImpulsePromptView {
    pub goal: BoundedText,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub reason: Option<BoundedText>,
}

pub struct CharacterThinkPromptProjection {
    pub context: CharacterThinkPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

#[derive(Debug, thiserror::Error)]
pub enum CharacterThinkProjectionError {
    #[error("character think stage state is missing")]
    MissingStageState,
    #[error("character think target is the player character")]
    PlayerCharacterTarget,
    #[error("character think target is unknown or off-scene")]
    InvalidTarget,
    #[error("character think target is not AI-controlled")]
    NonAiTarget,
    #[error("character think private knowledge is unauthorized")]
    UnauthorizedKnowledge,
    #[error("character think prompt input budget exceeded")]
    InputBudgetExceeded,
    #[error("character think prompt field is invalid")]
    InvalidPromptField,
}

pub trait CharacterThinkPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
        request: &CharacterThinkRequest,
    ) -> Result<CharacterThinkPromptProjection, CharacterThinkProjectionError>;
}

pub struct DefaultCharacterThinkPromptContextProjector {
    config: CharacterThinkConfig,
}

impl DefaultCharacterThinkPromptContextProjector {
    pub fn new(config: CharacterThinkConfig) -> Self {
        Self { config }
    }
}

impl CharacterThinkPromptContextProjector for DefaultCharacterThinkPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
        request: &CharacterThinkRequest,
    ) -> Result<CharacterThinkPromptProjection, CharacterThinkProjectionError> {
        let baseline = ctx.baseline().ok_or(CharacterThinkProjectionError::MissingStageState)?;
        let _plan = ctx.plan().ok_or(CharacterThinkProjectionError::MissingStageState)?;
        if request.reason.as_str().trim().is_empty()
            || request.reason.as_str().len() > self.config.max_thinking_focus_bytes
        {
            return Err(CharacterThinkProjectionError::InvalidPromptField);
        }
        if request.character_id == baseline.player_character.character_id {
            return Err(CharacterThinkProjectionError::PlayerCharacterTarget);
        }
        let character = baseline
            .scene_characters
            .iter()
            .find(|candidate| candidate.character_id == request.character_id)
            .ok_or(CharacterThinkProjectionError::InvalidTarget)?;
        if !baseline.current_scene.present_character_ids.contains(&request.character_id) {
            return Err(CharacterThinkProjectionError::InvalidTarget);
        }
        if !matches!(
            character.binding.controller,
            crate::domain::story_instance::binding::RoleController::Ai
        ) {
            return Err(CharacterThinkProjectionError::NonAiTarget);
        }
        let player_input = BoundedText::try_new(
            ctx.player_input().to_owned(),
            "player_input",
            self.config.max_input_tokens.saturating_mul(4) as usize,
        )
        .map_err(|_| CharacterThinkProjectionError::InvalidPromptField)?;
        let relevant_character_knowledge = project_knowledge(ctx, &request.character_id)?;
        let narrative_character_impulses = ctx
            .narrative_projection()
            .map(|projection| projection.plan.character_impulses.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|impulse| impulse.target_character_id == request.character_id)
            .map(|impulse| CharacterThinkImpulsePromptView {
                goal: impulse.goal.clone(),
                emotion: impulse.emotion.clone(),
                urgency: impulse.urgency,
                reason: impulse.reason.clone(),
            })
            .collect();
        let target_character = CharacterThinkCharacterPromptView {
            character_id: character.character_id.clone(),
            name: character.card.meta.name.clone(),
            description: Some(character.card.profile.description.clone()),
            personality: character.card.profile.personality.clone(),
            values: character.card.profile.values.clone(),
            fears: character.card.profile.fears.clone(),
        };
        let current_character_state = CharacterThinkStatePromptView {
            location: Some(
                BoundedText::try_new(
                    character.state.location.as_str().to_owned(),
                    "character_location",
                    self.config.max_input_tokens.saturating_mul(4) as usize,
                )
                .map_err(|_| CharacterThinkProjectionError::InvalidPromptField)?,
            ),
            goals: character.state.goals.clone(),
            relevant_attributes: character
                .state
                .attributes
                .iter()
                .map(|(name, value)| {
                    Ok(CharacterStateAttributePromptView {
                        name: BoundedText::try_new(
                            name.as_str().to_owned(),
                            "attribute_name",
                            self.config.max_input_tokens.saturating_mul(4) as usize,
                        )
                        .map_err(|_| CharacterThinkProjectionError::InvalidPromptField)?,
                        value: value.clone(),
                    })
                })
                .collect::<Result<_, CharacterThinkProjectionError>>()?,
        };
        let story_continuity = CharacterThinkStoryContinuityPromptView {
            story_summary: baseline.story_continuity.summary().text.clone(),
            recent_story: baseline
                .story_continuity
                .recent_segments()
                .iter()
                .map(|segment| segment.text.clone())
                .collect(),
        };
        let current_scene = CharacterThinkScenePromptView {
            location: Some(
                BoundedText::try_new(
                    baseline.current_scene.location_key.as_str().to_owned(),
                    "scene_location",
                    self.config.max_input_tokens.saturating_mul(4) as usize,
                )
                .map_err(|_| CharacterThinkProjectionError::InvalidPromptField)?,
            ),
            time: Some(baseline.current_scene.time.clone()),
            situation: Some(baseline.current_scene.description.clone()),
            observable_conditions: Vec::new(),
        };
        let context = CharacterThinkPromptContext {
            target_character,
            current_character_state,
            story_continuity,
            current_scene,
            relevant_character_knowledge,
            narrative_character_impulses,
            thinking_focus: request.reason.clone(),
            player_input,
        };
        let rc_vars = render_runtime_vars(&context);
        let input_tokens = rc_vars
            .as_map()
            .values()
            .filter_map(Value::as_str)
            .map(estimate_text_tokens)
            .fold(0u64, u64::saturating_add);
        if input_tokens > self.config.max_input_tokens {
            return Err(CharacterThinkProjectionError::InputBudgetExceeded);
        }
        let fti_vars = TrustedPromptVars::new(HashMap::from([(
            "output_schema".into(),
            Value::String(character_decision_output_schema(&self.config).to_string()),
        )]));
        Ok(CharacterThinkPromptProjection {
            context,
            rc_vars,
            fti_vars,
        })
    }
}

pub fn character_decision_output_schema(config: &CharacterThinkConfig) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["decision"],
        "properties": {
            "decision": {"type": "string", "minLength": 1, "maxLength": config.max_field_bytes},
            "suggested_utterance": {"type": ["string", "null"], "minLength": 1, "maxLength": config.max_field_bytes}
        }
    })
}

fn project_knowledge(
    ctx: &TurnExecutionContext,
    character_id: &CharacterId,
) -> Result<Vec<CharacterThinkKnowledgePromptView>, CharacterThinkProjectionError> {
    ctx.retrieved()
        .for_character(character_id)
        .iter()
        .map(|item| {
            if item.provenance.audience
                != (RetrievalAudience::Character {
                    character_id: character_id.clone(),
                })
            {
                return Err(CharacterThinkProjectionError::UnauthorizedKnowledge);
            }
            let kind = match item.provenance.knowledge_kind {
                KnowledgeKind::Rumor if item.provenance.memory_owner.is_none() => CharacterThinkKnowledgeKind::Rumor,
                KnowledgeKind::Memory if item.provenance.memory_owner.as_ref() == Some(character_id) => {
                    CharacterThinkKnowledgeKind::Memory
                }
                _ => return Err(CharacterThinkProjectionError::UnauthorizedKnowledge),
            };
            Ok(CharacterThinkKnowledgePromptView {
                kind,
                content: item.content.clone(),
            })
        })
        .collect()
}

fn render_runtime_vars(context: &CharacterThinkPromptContext) -> RuntimePromptVars {
    RuntimePromptVars::new(HashMap::from([
        (
            "target_character".into(),
            Value::String(render_target_character(&context.target_character)),
        ),
        (
            "current_character_state".into(),
            Value::String(render_character_state(&context.current_character_state)),
        ),
        (
            "story_summary".into(),
            Value::String(render_optional_text(context.story_continuity.story_summary.as_str())),
        ),
        (
            "recent_story".into(),
            Value::String(render_recent_story(&context.story_continuity.recent_story)),
        ),
        (
            "current_scene".into(),
            Value::String(render_current_scene(&context.current_scene)),
        ),
        (
            "relevant_character_knowledge".into(),
            Value::String(render_knowledge(&context.relevant_character_knowledge)),
        ),
        (
            "narrative_character_impulses".into(),
            Value::String(render_impulses(&context.narrative_character_impulses)),
        ),
        ("thinking_focus".into(), Value::String(quoted(context.thinking_focus.as_str()))),
        ("player_input".into(), Value::String(quoted(context.player_input.as_str()))),
    ]))
}

fn render_target_character(value: &CharacterThinkCharacterPromptView) -> String {
    [
        format!("character_id: {}", quoted(value.character_id.as_str())),
        format!("name: {}", quoted(value.name.as_str())),
        format!("description: {}", render_optional(value.description.as_ref())),
        format!("personality: {}", quoted_list(&value.personality)),
        format!("values: {}", quoted_list(&value.values)),
        format!("fears: {}", quoted_list(&value.fears)),
    ]
    .join("\n")
}

fn render_character_state(value: &CharacterThinkStatePromptView) -> String {
    let attributes = if value.relevant_attributes.is_empty() {
        "None.".into()
    } else {
        value
            .relevant_attributes
            .iter()
            .map(|attribute| {
                format!(
                    "- name: {}\n  value: {}",
                    quoted(attribute.name.as_str()),
                    render_scalar(&attribute.value)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    [
        format!("location: {}", render_optional(value.location.as_ref())),
        format!("goals: {}", quoted_list(&value.goals)),
        format!("relevant_attributes:\n{attributes}"),
    ]
    .join("\n")
}

fn render_recent_story(values: &[BoundedText]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| format!("- text: {}", quoted(value.as_str())))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_current_scene(value: &CharacterThinkScenePromptView) -> String {
    [
        format!("location: {}", render_optional(value.location.as_ref())),
        format!("time: {}", render_optional(value.time.as_ref())),
        format!("immediate_situation: {}", render_optional(value.situation.as_ref())),
        format!("observable_conditions: {}", quoted_list(&value.observable_conditions)),
    ]
    .join("\n")
}

fn render_knowledge(values: &[CharacterThinkKnowledgePromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            let kind = match value.kind {
                CharacterThinkKnowledgeKind::Rumor => "rumor",
                CharacterThinkKnowledgeKind::Memory => "memory",
            };
            format!("- kind: {kind}\n  content: {}", quoted(value.content.as_str()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_impulses(values: &[CharacterThinkImpulsePromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            let urgency = match value.urgency {
                ImpulseUrgency::Low => "low",
                ImpulseUrgency::Medium => "medium",
                ImpulseUrgency::High => "high",
            };
            format!(
                "- goal: {}\n  emotion: {}\n  urgency: {urgency}\n  reason: {}",
                quoted(value.goal.as_str()),
                render_optional(value.emotion.as_ref()),
                render_optional(value.reason.as_ref())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_optional(value: Option<&BoundedText>) -> String {
    value.map(|value| quoted(value.as_str())).unwrap_or_else(|| "None.".into())
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
#[path = "tests/character_think_prompt_tests.rs"]
mod tests;
