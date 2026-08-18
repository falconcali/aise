use crate::config::{CharacterThinkConfig, ContextPreparationConfig};
use crate::domain::asset::character_card::DialogueExample;
use crate::domain::asset::ids::LocationKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::effect::ImpulseUrgency;
use crate::domain::story_instance::role::RoleController;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::CharacterThinkRequest;
use crate::prompt::{RoleKnowledgePromptView, RuntimePromptVars, TrustedPromptVars, render_role_knowledge};
use crate::turn::turn_context::TurnExecutionContext;
use serde_json::Value;
use std::collections::HashMap;

pub const CHARACTER_THINK_CSI_SLOT: &str = "context.character_think.csi";
pub const CHARACTER_THINK_RC_SLOT: &str = "context.character_think.rc";
pub const CHARACTER_THINK_FTI_SLOT: &str = "context.character_think.fti";

#[derive(Debug, Clone)]
pub struct CharacterThinkPromptContext {
    pub target_role: CharacterThinkRolePromptView,
    pub current_role_state: CharacterThinkStatePromptView,
    pub story_continuity: CharacterThinkStoryContinuityPromptView,
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
pub struct CharacterThinkRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    pub dialogue_examples: Vec<DialogueExample>,
    pub knowledge: RoleKnowledgePromptView,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkStatePromptView {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: Vec<CharacterStateAttributePromptView>,
}

#[derive(Debug, Clone)]
pub struct CharacterStateAttributePromptView {
    pub name: BoundedText,
    pub value: ScalarValue,
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
    #[error("character think role is unknown: {role_id}")]
    UnknownRole { role_id: RoleId },
    #[error("character think role is player-controlled: {role_id}")]
    PlayerControlledRole { role_id: RoleId },
    #[error("character think required prompt data exceeds budget")]
    RequiredPromptDataExceedsBudget,
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
    character_config: CharacterThinkConfig,
    context_config: ContextPreparationConfig,
}

impl DefaultCharacterThinkPromptContextProjector {
    pub fn new(character_config: CharacterThinkConfig, context_config: ContextPreparationConfig) -> Self {
        Self {
            character_config,
            context_config,
        }
    }
}

impl CharacterThinkPromptContextProjector for DefaultCharacterThinkPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
        request: &CharacterThinkRequest,
    ) -> Result<CharacterThinkPromptProjection, CharacterThinkProjectionError> {
        let baseline = ctx.baseline().ok_or(CharacterThinkProjectionError::MissingStageState)?;
        let snapshot = ctx.snapshot().ok_or(CharacterThinkProjectionError::MissingStageState)?;
        let _plan = ctx.plan().ok_or(CharacterThinkProjectionError::MissingStageState)?;
        if request.reason.as_str().trim().is_empty()
            || request.reason.as_str().len() > self.character_config.max_thinking_focus_bytes
        {
            return Err(CharacterThinkProjectionError::InvalidPromptField);
        }
        let role = snapshot
            .role(&request.role_id)
            .ok_or_else(|| CharacterThinkProjectionError::UnknownRole {
                role_id: request.role_id.clone(),
            })?;
        if !matches!(role.controller, RoleController::Ai) {
            return Err(CharacterThinkProjectionError::PlayerControlledRole {
                role_id: request.role_id.clone(),
            });
        }
        let player_input = BoundedText::try_new(
            ctx.player_input().to_owned(),
            "player_input",
            self.character_config.max_input_tokens.saturating_mul(4) as usize,
        )
        .map_err(|_| CharacterThinkProjectionError::InvalidPromptField)?;
        let knowledge = project_knowledge(ctx, &request.role_id);
        let narrative_character_impulses = ctx
            .narrative_projection()
            .map(|projection| projection.plan.character_impulses.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|impulse| impulse.target_role_id == request.role_id)
            .map(|impulse| CharacterThinkImpulsePromptView {
                goal: impulse.goal.clone(),
                emotion: impulse.emotion.clone(),
                urgency: impulse.urgency,
                reason: impulse.reason.clone(),
            })
            .collect();
        let mut target_role = CharacterThinkRolePromptView {
            role_id: role.role_id.clone(),
            name: role.effective_profile.name.clone(),
            role_label: role.role_label.clone(),
            appearance: role.effective_profile.appearance.clone(),
            personality: role.effective_profile.personality.clone(),
            speaking_style: role.effective_profile.speaking_style.clone(),
            dialogue_examples: select_dialogue_examples(
                &role.effective_profile.dialogue_examples,
                &self.context_config,
            ),
            knowledge,
        };
        let current_role_state = CharacterThinkStatePromptView {
            location: role.state.location.clone(),
            goals: role.state.goals.clone(),
            attributes: role
                .state
                .attributes
                .iter()
                .map(|(name, value)| {
                    Ok(CharacterStateAttributePromptView {
                        name: BoundedText::try_new(
                            name.as_str().to_owned(),
                            "attribute_name",
                            self.character_config.max_input_tokens.saturating_mul(4) as usize,
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
        let mut context = CharacterThinkPromptContext {
            target_role: target_role.clone(),
            current_role_state,
            story_continuity,
            narrative_character_impulses,
            thinking_focus: request.reason.clone(),
            player_input,
        };
        let mut rc_vars = render_runtime_vars(&context);
        while runtime_tokens(&rc_vars) > self.character_config.max_input_tokens
            && !target_role.dialogue_examples.is_empty()
        {
            target_role.dialogue_examples.pop();
            context.target_role = target_role.clone();
            rc_vars = render_runtime_vars(&context);
        }
        if runtime_tokens(&rc_vars) > self.character_config.max_input_tokens {
            return Err(CharacterThinkProjectionError::RequiredPromptDataExceedsBudget);
        }
        let fti_vars = TrustedPromptVars::new(HashMap::new());
        Ok(CharacterThinkPromptProjection {
            context,
            rc_vars,
            fti_vars,
        })
    }
}

fn project_knowledge(ctx: &TurnExecutionContext, role_id: &RoleId) -> RoleKnowledgePromptView {
    match ctx.retrieved().character(role_id) {
        Some(character) => RoleKnowledgePromptView {
            known_rumors: character.known_rumors.iter().map(|item| item.content.clone()).collect(),
            memories: character.memories.iter().map(|item| item.content.clone()).collect(),
        },
        None => RoleKnowledgePromptView::default(),
    }
}

fn render_runtime_vars(context: &CharacterThinkPromptContext) -> RuntimePromptVars {
    RuntimePromptVars::new(HashMap::from([
        (
            "target_character".into(),
            Value::String(render_target_role(&context.target_role)),
        ),
        (
            "current_character_state".into(),
            Value::String(render_role_state(&context.current_role_state)),
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
            "narrative_character_impulses".into(),
            Value::String(render_impulses(&context.narrative_character_impulses)),
        ),
        ("thinking_focus".into(), Value::String(quoted(context.thinking_focus.as_str()))),
        ("player_input".into(), Value::String(quoted(context.player_input.as_str()))),
    ]))
}

fn render_target_role(value: &CharacterThinkRolePromptView) -> String {
    let mut lines = vec![
        format!("role_id: {}", quoted(value.role_id.as_str())),
        format!("name: {}", quoted(value.name.as_str())),
    ];
    if value.role_label != value.name {
        lines.push(format!("role: {}", quoted(value.role_label.as_str())));
    }
    push_optional(&mut lines, "appearance", value.appearance.as_ref());
    push_optional(&mut lines, "personality", value.personality.as_ref());
    push_optional(&mut lines, "speaking_style", value.speaking_style.as_ref());
    if !value.dialogue_examples.is_empty() {
        lines.push(format!(
            "dialogue_examples:\n{}",
            value
                .dialogue_examples
                .iter()
                .map(|example| format!(
                    "- situation: {}\n  response: {}",
                    quoted(example.situation.as_str()),
                    quoted(example.response.as_str())
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !value.knowledge.known_rumors.is_empty() || !value.knowledge.memories.is_empty() {
        lines.push(render_role_knowledge(&value.knowledge));
    }
    lines.join("\n")
}

fn render_role_state(value: &CharacterThinkStatePromptView) -> String {
    let mut lines = vec![format!("location: {}", quoted(value.location.as_str()))];
    if !value.goals.is_empty() {
        lines.push(format!("goals: {}", quoted_list(&value.goals)));
    }
    if !value.attributes.is_empty() {
        let attributes = value
            .attributes
            .iter()
            .map(|attribute| {
                format!(
                    "- name: {}\n  value: {}",
                    quoted(attribute.name.as_str()),
                    render_scalar(&attribute.value)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(format!("attributes:\n{attributes}"));
    }
    lines.join("\n")
}

fn render_recent_story(values: &[BoundedText]) -> String {
    values.iter().map(|value| value.as_str()).collect::<Vec<_>>().join("\n\n")
}

fn render_impulses(values: &[CharacterThinkImpulsePromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| {
            let urgency = match value.urgency {
                ImpulseUrgency::Low => "low",
                ImpulseUrgency::Medium => "medium",
                ImpulseUrgency::High => "high",
            };
            let mut lines = vec![format!("- goal: {}", quoted(value.goal.as_str()))];
            if let Some(emotion) = non_empty(value.emotion.as_ref()) {
                lines.push(format!("  emotion: {}", quoted(emotion)));
            }
            lines.push(format!("  urgency: {urgency}"));
            if let Some(reason) = non_empty(value.reason.as_ref()) {
                lines.push(format!("  reason: {}", quoted(reason)));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_empty(value: Option<&BoundedText>) -> Option<&str> {
    value.map(BoundedText::as_str).filter(|value| !value.trim().is_empty())
}

fn push_optional(lines: &mut Vec<String>, name: &str, value: Option<&BoundedText>) {
    if let Some(value) = value {
        lines.push(format!("{name}: {}", quoted(value.as_str())));
    }
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

fn render_story_summary(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        value.to_owned()
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
#[path = "tests/character_think_prompt_tests.rs"]
mod tests;
