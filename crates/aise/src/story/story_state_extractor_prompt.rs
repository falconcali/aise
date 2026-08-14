use crate::domain::asset::ids::SceneKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::text::estimate_text_tokens;
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_validation::{ValidationDecision, ValidationIssueCode, ValidationLocation};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub const STORY_STATE_EXTRACTOR_CSI_SLOT: &str = "context.story_state_extractor.csi";
pub const STORY_STATE_EXTRACTOR_RC_SLOT: &str = "context.story_state_extractor.rc";
pub const STORY_STATE_EXTRACTOR_FTI_SLOT: &str = "context.story_state_extractor.fti";

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorPromptContext {
    pub story_text: BoundedText,
    pub current_scene: StoryStateExtractorScenePromptView,
    pub characters: Vec<StoryStateExtractorCharacterPromptView>,
    pub relationships: Vec<StoryStateExtractorRelationshipPromptView>,
    pub modifiable_knowledge: Vec<StoryStateExtractorKnowledgePromptView>,
    pub previous_extraction: Option<BoundedText>,
    pub validation_issues: Vec<StoryStateExtractorValidationIssuePromptView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorScenePromptView {
    pub scene_key: SceneKey,
    pub location: BoundedText,
    pub time: BoundedText,
    pub description: BoundedText,
    pub present_character_ids: Vec<CharacterId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorCharacterPromptView {
    pub character_id: CharacterId,
    pub location: BoundedText,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<String, ScalarValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorRelationshipPromptView {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: BoundedText,
    pub trust: i16,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorKnowledgePromptView {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub memory_owner: Option<CharacterId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorValidationIssuePromptView {
    pub code: ValidationIssueCode,
    pub location: Option<StoryStateExtractorValidationLocationPromptView>,
    pub message: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorValidationLocationPromptView {
    pub path: BoundedText,
    pub item_index: Option<u32>,
}

pub struct StoryStateExtractorPromptProjection {
    pub context: StoryStateExtractorPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryStateExtractorProjectionError {
    #[error("story state extractor candidate story is missing")]
    MissingStory,
    #[error("story state extractor snapshot is missing")]
    MissingSnapshot,
    #[error("story state extractor re-extraction requires ValidationDecision::ReextractState")]
    ValidationDoesNotRequireReextraction,
    #[error("story state extractor validation issues are empty for a re-extraction attempt")]
    EmptyValidationIssues,
    #[error("story state extractor prompt invariant violated: {code}")]
    Invariant { code: &'static str },
    #[error("story state extractor required prompt data exceeds budget")]
    RequiredPromptDataExceedsBudget,
}

pub trait StoryStateExtractorPromptContextProjector: Send + Sync {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryStateExtractorPromptProjection, StoryStateExtractorProjectionError>;
}

pub struct DefaultStoryStateExtractorPromptContextProjector;

impl StoryStateExtractorPromptContextProjector for DefaultStoryStateExtractorPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryStateExtractorPromptProjection, StoryStateExtractorProjectionError> {
        let story = ctx.story().ok_or(StoryStateExtractorProjectionError::MissingStory)?;
        let snapshot = ctx.snapshot().ok_or(StoryStateExtractorProjectionError::MissingSnapshot)?;

        let story_text = story.story_text.clone();

        let current_scene = StoryStateExtractorScenePromptView {
            scene_key: snapshot.current_scene().scene_key.clone(),
            location: bounded_key(
                snapshot.current_scene().location_key.as_str(),
                "scene_location",
                ctx.budget().max_item_bytes(),
            )?,
            time: snapshot.current_scene().time.clone(),
            description: snapshot.current_scene().description.clone(),
            present_character_ids: snapshot.current_scene().present_character_ids.clone(),
        };

        let characters = snapshot
            .character_states()
            .values()
            .map(|state| {
                Ok(StoryStateExtractorCharacterPromptView {
                    character_id: state.character_id.clone(),
                    location: bounded_key(
                        state.location.as_str(),
                        "character_location",
                        ctx.budget().max_item_bytes(),
                    )?,
                    goals: state.goals.clone(),
                    attributes: state
                        .attributes
                        .iter()
                        .map(|(key, value)| (key.as_str().to_owned(), value.clone()))
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, StoryStateExtractorProjectionError>>()?;

        let relationships = snapshot
            .relationships()
            .iter()
            .map(|relationship| {
                Ok(StoryStateExtractorRelationshipPromptView {
                    source_character_id: relationship.source_character_id.clone(),
                    target_character_id: relationship.target_character_id.clone(),
                    kind: bounded_key(relationship.kind.as_str(), "relationship_kind", ctx.budget().max_item_bytes())?,
                    trust: relationship.trust,
                })
            })
            .collect::<Result<Vec<_>, StoryStateExtractorProjectionError>>()?;

        let modifiable_knowledge = modifiable_knowledge_view(ctx);

        let (previous_extraction, validation_issues) = if ctx.phase() == TurnPhase::StateReextractionRequired {
            let validation = ctx.validation().ok_or(StoryStateExtractorProjectionError::Invariant {
                code: "missing_validation_for_reextraction",
            })?;
            if validation.decision() != ValidationDecision::ReextractState {
                return Err(StoryStateExtractorProjectionError::ValidationDoesNotRequireReextraction);
            }
            if validation.issues().is_empty() {
                return Err(StoryStateExtractorProjectionError::EmptyValidationIssues);
            }
            let issues = validation
                .issues()
                .iter()
                .map(|issue| {
                    let message = BoundedText::try_new(
                        issue.message.clone(),
                        "story_state_extractor_validation_message",
                        ctx.budget().max_validation_issue_bytes(),
                    )
                    .map_err(|_| StoryStateExtractorProjectionError::Invariant {
                        code: "validation_issue_message_invalid",
                    })?;
                    let location = issue
                        .location
                        .as_ref()
                        .map(|location| project_location(location, ctx.budget().max_item_bytes()))
                        .transpose()?;
                    Ok(StoryStateExtractorValidationIssuePromptView {
                        code: issue.code,
                        location,
                        message,
                    })
                })
                .collect::<Result<Vec<_>, StoryStateExtractorProjectionError>>()?;
            let previous = ctx.extraction().map(|extraction| {
                let rendered = serde_json::to_string_pretty(extraction).unwrap_or_default();
                BoundedText::try_new(rendered, "previous_extraction", ctx.budget().max_state_extraction_bytes())
            });
            let previous = previous
                .transpose()
                .map_err(|_| StoryStateExtractorProjectionError::Invariant {
                    code: "previous_extraction_serialization_failed",
                })?;
            (previous, issues)
        } else {
            (None, Vec::new())
        };

        let context = StoryStateExtractorPromptContext {
            story_text,
            current_scene,
            characters,
            relationships,
            modifiable_knowledge,
            previous_extraction,
            validation_issues,
        };
        let rc_vars = render_runtime_vars(&context);
        let input_tokens = rc_vars
            .as_map()
            .values()
            .filter_map(Value::as_str)
            .map(estimate_text_tokens)
            .fold(0u64, u64::saturating_add);
        if input_tokens > ctx.budget().state_extractor_max_context_tokens() {
            return Err(StoryStateExtractorProjectionError::RequiredPromptDataExceedsBudget);
        }
        let output_schema =
            crate::domain::turn::StoryStateExtractorOutput::json_schema(ctx.budget().state_extraction_limits());
        let fti_vars = TrustedPromptVars::new(HashMap::from([(
            "output_schema".into(),
            Value::String(output_schema.to_string()),
        )]));
        Ok(StoryStateExtractorPromptProjection {
            context,
            rc_vars,
            fti_vars,
        })
    }
}

fn modifiable_knowledge_view(ctx: &TurnExecutionContext) -> Vec<StoryStateExtractorKnowledgePromptView> {
    let mut index = BTreeMap::new();
    if let Some(baseline) = ctx.baseline() {
        for entry in &baseline.relevant_knowledge {
            index.insert(
                entry.entry_id.clone(),
                StoryStateExtractorKnowledgePromptView {
                    source_id: entry.entry_id.clone(),
                    kind: entry.kind,
                    content: entry.content.clone(),
                    memory_owner: None,
                },
            );
        }
    }
    for item in ctx.retrieved().writer() {
        index.insert(
            item.provenance.source_id.clone(),
            StoryStateExtractorKnowledgePromptView {
                source_id: item.provenance.source_id.clone(),
                kind: item.provenance.knowledge_kind,
                content: item.content.clone(),
                memory_owner: item.provenance.memory_owner.clone(),
            },
        );
    }
    for items in ctx.retrieved().characters().values() {
        for item in items {
            index.insert(
                item.provenance.source_id.clone(),
                StoryStateExtractorKnowledgePromptView {
                    source_id: item.provenance.source_id.clone(),
                    kind: item.provenance.knowledge_kind,
                    content: item.content.clone(),
                    memory_owner: item.provenance.memory_owner.clone(),
                },
            );
        }
    }
    index.into_values().collect()
}

fn bounded_key(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<BoundedText, StoryStateExtractorProjectionError> {
    BoundedText::try_new(value.to_owned(), field, max_bytes).map_err(|_| {
        StoryStateExtractorProjectionError::Invariant {
            code: "invalid_key_text",
        }
    })
}

fn project_location(
    location: &ValidationLocation,
    max_item_bytes: usize,
) -> Result<StoryStateExtractorValidationLocationPromptView, StoryStateExtractorProjectionError> {
    let path = BoundedText::try_new(
        location.path.clone(),
        "story_state_extractor_validation_location",
        max_item_bytes,
    )
    .map_err(|_| StoryStateExtractorProjectionError::Invariant {
        code: "validation_issue_location_invalid",
    })?;
    Ok(StoryStateExtractorValidationLocationPromptView {
        path,
        item_index: location.item_index,
    })
}

fn render_runtime_vars(context: &StoryStateExtractorPromptContext) -> RuntimePromptVars {
    RuntimePromptVars::new(HashMap::from([
        ("story_text".into(), Value::String(quoted(context.story_text.as_str()))),
        ("current_scene".into(), Value::String(render_scene(&context.current_scene))),
        ("characters".into(), Value::String(render_characters(&context.characters))),
        (
            "relationships".into(),
            Value::String(render_relationships(&context.relationships)),
        ),
        (
            "modifiable_knowledge".into(),
            Value::String(render_knowledge(&context.modifiable_knowledge)),
        ),
        (
            "previous_extraction".into(),
            Value::String(
                context
                    .previous_extraction
                    .as_ref()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_else(|| "None.".into()),
            ),
        ),
        (
            "validation_issues".into(),
            Value::String(render_validation_issues(&context.validation_issues)),
        ),
    ]))
}

fn render_scene(value: &StoryStateExtractorScenePromptView) -> String {
    format!(
        "scene_key: {}\nlocation: {}\ntime: {}\ndescription: {}\npresent_character_ids: {}",
        quoted(value.scene_key.as_str()),
        quoted(value.location.as_str()),
        quoted(value.time.as_str()),
        quoted(value.description.as_str()),
        id_list(&value.present_character_ids)
    )
}

fn render_characters(values: &[StoryStateExtractorCharacterPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            let attributes = if value.attributes.is_empty() {
                "None.".into()
            } else {
                format!(
                    "{{{}}}",
                    value
                        .attributes
                        .iter()
                        .map(|(key, value)| format!("{}: {}", quoted(key), render_scalar(value)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!(
                "- character_id: {}\n  location: {}\n  goals: {}\n  attributes: {attributes}",
                quoted(value.character_id.as_str()),
                quoted(value.location.as_str()),
                quoted_list(&value.goals)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_relationships(values: &[StoryStateExtractorRelationshipPromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .map(|value| {
            format!(
                "- source_character_id: {}\n  target_character_id: {}\n  kind: {}\n  trust: {}",
                quoted(value.source_character_id.as_str()),
                quoted(value.target_character_id.as_str()),
                quoted(value.kind.as_str()),
                value.trust
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_knowledge(values: &[StoryStateExtractorKnowledgePromptView]) -> String {
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
            let owner = value
                .memory_owner
                .as_ref()
                .map(|owner| quoted(owner.as_str()))
                .unwrap_or_else(|| "None.".into());
            format!(
                "- source_id: {}\n  kind: {kind}\n  memory_owner: {owner}\n  content: {}",
                quoted(value.source_id.as_str()),
                quoted(value.content.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_validation_issues(values: &[StoryStateExtractorValidationIssuePromptView]) -> String {
    if values.is_empty() {
        return "None.".into();
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let location = value
                .location
                .as_ref()
                .map(|location| {
                    let path = quoted(location.path.as_str());
                    match location.item_index {
                        Some(item_index) => format!("{path}\n   Item Index: {item_index}"),
                        None => path,
                    }
                })
                .unwrap_or_else(|| "None.".into());
            format!(
                "{}. Code: {}\n   Location: {}\n   Message: {}",
                index + 1,
                value.code,
                location,
                quoted(value.message.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
#[path = "tests/story_state_extractor_prompt_tests.rs"]
mod tests;
