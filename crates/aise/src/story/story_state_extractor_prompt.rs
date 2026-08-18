use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{AttributeKey, LocationKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{MemoryId, RoleId, allocate_dynamic_role_candidates};
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::text::estimate_text_tokens;
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_validation::{ValidationDecision, ValidationIssueCode, ValidationLocation};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const STORY_STATE_EXTRACTOR_CSI_SLOT: &str = "context.story_state_extractor.csi";
pub const STORY_STATE_EXTRACTOR_RC_SLOT: &str = "context.story_state_extractor.rc";
pub const STORY_STATE_EXTRACTOR_FTI_SLOT: &str = "context.story_state_extractor.fti";

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorPromptContext {
    pub story_text: BoundedText,
    pub roles: Vec<StoryStateExtractorRolePromptView>,
    pub relationships: Vec<StoryStateExtractorRelationshipPromptView>,
    pub modifiable_knowledge: ModifiableWorldKnowledgePromptView,
    pub condition_queries: Vec<StoryStateExtractorConditionQueryPromptView>,
    pub previous_extraction: Option<BoundedText>,
    pub validation_issues: Vec<StoryStateExtractorValidationIssuePromptView>,
    pub cast_policy: CastPolicy,
    pub available_locations: Vec<LocationKey>,
    pub new_role_candidates: Vec<RoleId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorConditionQueryPromptView {
    pub condition_key: crate::domain::asset::ids::NarrativeConditionKey,
    pub criterion: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
    pub memories: Vec<ModifiableMemoryPromptView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorRelationshipPromptView {
    pub source_role_id: RoleId,
    pub target_role_id: RoleId,
    pub kind: BoundedText,
    pub trust: i16,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModifiableKnowledgePromptItem {
    pub id: KnowledgeSourceId,
    pub content: BoundedText,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModifiableWorldKnowledgePromptView {
    pub facts: Vec<ModifiableKnowledgePromptItem>,
    pub rumors: Vec<ModifiableKnowledgePromptItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModifiableMemoryPromptView {
    pub id: MemoryId,
    pub content: BoundedText,
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

        let roles = snapshot
            .roles()
            .values()
            .map(|role| {
                Ok(StoryStateExtractorRolePromptView {
                    role_id: role.role_id.clone(),
                    name: role.effective_profile.name.clone(),
                    role_label: role.role_label.clone(),
                    location: role.state.location.clone(),
                    goals: role.state.goals.clone(),
                    attributes: role.state.attributes.clone(),
                    memories: role_memories(ctx, &role.role_id)?,
                })
            })
            .collect::<Result<Vec<_>, StoryStateExtractorProjectionError>>()?;
        if roles.is_empty() {
            return Err(StoryStateExtractorProjectionError::Invariant { code: "roles_empty" });
        }

        let relationships = snapshot
            .relationships()
            .iter()
            .map(|relationship| {
                Ok(StoryStateExtractorRelationshipPromptView {
                    source_role_id: relationship.source_role_id.clone(),
                    target_role_id: relationship.target_role_id.clone(),
                    kind: bounded_key(relationship.kind.as_str(), "relationship_kind", ctx.budget().max_item_bytes())?,
                    trust: relationship.trust,
                })
            })
            .collect::<Result<Vec<_>, StoryStateExtractorProjectionError>>()?;

        let modifiable_knowledge = modifiable_knowledge_view(ctx);

        let condition_queries = ctx
            .narrative_projection()
            .map(|projection| {
                projection
                    .condition_queries
                    .iter()
                    .map(|query| StoryStateExtractorConditionQueryPromptView {
                        condition_key: query.condition_key.clone(),
                        criterion: query.criterion.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

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

        let cast_policy = snapshot.instance_settings().cast_policy;
        let available_locations = available_location_keys(snapshot);
        let new_role_candidates = if cast_policy == CastPolicy::Open {
            allocate_dynamic_role_candidates(
                snapshot.role_id_high_water(),
                ctx.budget().state_extraction_limits().max_new_roles,
            )
            .map(|pool| pool.candidates)
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        let context = StoryStateExtractorPromptContext {
            story_text,
            roles,
            relationships,
            modifiable_knowledge,
            condition_queries,
            previous_extraction,
            validation_issues,
            cast_policy,
            available_locations,
            new_role_candidates,
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
        let fti_vars = TrustedPromptVars::new(HashMap::new());
        Ok(StoryStateExtractorPromptProjection {
            context,
            rc_vars,
            fti_vars,
        })
    }
}

fn available_location_keys(snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot) -> Vec<LocationKey> {
    let mut keys: BTreeSet<LocationKey> = snapshot
        .entity_catalog()
        .iter()
        .filter_map(|entity| match entity {
            KnowledgeEntity::Location(key) => Some(key.clone()),
            _ => None,
        })
        .collect();
    for role in snapshot.roles().values() {
        keys.insert(role.state.location.clone());
    }
    keys.into_iter().collect()
}

fn modifiable_knowledge_view(ctx: &TurnExecutionContext) -> ModifiableWorldKnowledgePromptView {
    let mut facts: BTreeMap<KnowledgeSourceId, BoundedText> = BTreeMap::new();
    let mut fact_order = Vec::new();
    let mut rumors: BTreeMap<KnowledgeSourceId, BoundedText> = BTreeMap::new();
    let mut rumor_order = Vec::new();
    if let Some(baseline) = ctx.baseline() {
        for entry in &baseline.relevant_world_knowledge.facts {
            insert_modifiable(&mut facts, &mut fact_order, entry.source_id.clone(), entry.content.clone());
        }
        for entry in &baseline.relevant_world_knowledge.rumors {
            insert_modifiable(&mut rumors, &mut rumor_order, entry.source_id.clone(), entry.content.clone());
        }
    }
    for item in &ctx.retrieved().world().facts {
        insert_modifiable(&mut facts, &mut fact_order, item.source_id.clone(), item.content.clone());
    }
    for item in &ctx.retrieved().world().rumors {
        insert_modifiable(&mut rumors, &mut rumor_order, item.source_id.clone(), item.content.clone());
    }
    for character in ctx.retrieved().characters().values() {
        for item in &character.known_rumors {
            insert_modifiable(&mut rumors, &mut rumor_order, item.source_id.clone(), item.content.clone());
        }
    }
    ModifiableWorldKnowledgePromptView {
        facts: fact_order
            .into_iter()
            .map(|id| ModifiableKnowledgePromptItem {
                content: facts.remove(&id).expect("tracked fact id present"),
                id,
            })
            .collect(),
        rumors: rumor_order
            .into_iter()
            .map(|id| ModifiableKnowledgePromptItem {
                content: rumors.remove(&id).expect("tracked rumor id present"),
                id,
            })
            .collect(),
    }
}

fn insert_modifiable(
    map: &mut BTreeMap<KnowledgeSourceId, BoundedText>,
    order: &mut Vec<KnowledgeSourceId>,
    id: KnowledgeSourceId,
    content: BoundedText,
) {
    if !map.contains_key(&id) {
        order.push(id.clone());
    }
    map.insert(id, content);
}

fn role_memories(
    ctx: &TurnExecutionContext,
    role_id: &RoleId,
) -> Result<Vec<ModifiableMemoryPromptView>, StoryStateExtractorProjectionError> {
    let Some(character) = ctx.retrieved().character(role_id) else {
        return Ok(Vec::new());
    };
    character
        .memories
        .iter()
        .map(|item| match &item.source_id {
            KnowledgeSourceId::Memory(memory_id) => Ok(ModifiableMemoryPromptView {
                id: memory_id.clone(),
                content: item.content.clone(),
            }),
            _ => Err(StoryStateExtractorProjectionError::Invariant {
                code: "modifiable_knowledge_owner_invalid",
            }),
        })
        .collect()
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
        ("roles".into(), Value::String(render_roles(&context.roles))),
        (
            "relationships".into(),
            Value::String(render_relationships(&context.relationships)),
        ),
        (
            "modifiable_knowledge".into(),
            Value::String(render_modifiable_knowledge(&context.modifiable_knowledge)),
        ),
        (
            "condition_queries".into(),
            Value::String(render_condition_queries(&context.condition_queries)),
        ),
        (
            "previous_extraction".into(),
            Value::String(
                context
                    .previous_extraction
                    .as_ref()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default(),
            ),
        ),
        (
            "validation_issues".into(),
            Value::String(render_validation_issues(&context.validation_issues)),
        ),
        (
            "cast_policy".into(),
            Value::String(cast_policy_label(context.cast_policy).to_owned()),
        ),
        (
            "available_locations".into(),
            Value::String(render_location_keys(&context.available_locations)),
        ),
        (
            "new_role_candidates".into(),
            Value::String(render_role_ids(&context.new_role_candidates)),
        ),
    ]))
}

fn cast_policy_label(policy: CastPolicy) -> &'static str {
    match policy {
        CastPolicy::Open => "open",
        CastPolicy::IncidentalOnly => "incidental_only",
        CastPolicy::Closed => "closed",
    }
}

fn render_location_keys(values: &[LocationKey]) -> String {
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn render_role_ids(values: &[RoleId]) -> String {
    format!(
        "[{}]",
        values.iter().map(|value| quoted(value.as_str())).collect::<Vec<_>>().join(", ")
    )
}

fn render_roles(values: &[StoryStateExtractorRolePromptView]) -> String {
    values
        .iter()
        .map(|value| {
            let mut lines = vec![
                format!("- role_id: {}", quoted(value.role_id.as_str())),
                format!("  name: {}", quoted(value.name.as_str())),
            ];
            if value.role_label != value.name {
                lines.push(format!("  role: {}", quoted(value.role_label.as_str())));
            }
            lines.push(format!("  location: {}", quoted(value.location.as_str())));
            if !value.goals.is_empty() {
                lines.push(format!("  goals: {}", quoted_list(&value.goals)));
            }
            if !value.attributes.is_empty() {
                let attributes = value
                    .attributes
                    .iter()
                    .map(|(key, value)| format!("{}: {}", quoted(key.as_str()), render_scalar(value)))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("  attributes: {{{attributes}}}"));
            }
            if !value.memories.is_empty() {
                let memories = value
                    .memories
                    .iter()
                    .map(|memory| {
                        format!(
                            "    - id: {}\n      content: {}",
                            quoted(memory.id.as_str()),
                            quoted(memory.content.as_str())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                lines.push(format!("  memories:\n{memories}"));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_relationships(values: &[StoryStateExtractorRelationshipPromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| {
            format!(
                "- source_role_id: {}\n  target_role_id: {}\n  kind: {}\n  trust: {}",
                quoted(value.source_role_id.as_str()),
                quoted(value.target_role_id.as_str()),
                quoted(value.kind.as_str()),
                value.trust
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_modifiable_knowledge(value: &ModifiableWorldKnowledgePromptView) -> String {
    let mut sections = Vec::new();
    if !value.facts.is_empty() {
        let items = value
            .facts
            .iter()
            .map(|item| {
                format!(
                    "- id: {}\n  content: {}",
                    quoted(item.id.as_str()),
                    quoted(item.content.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Facts\n\n{items}"));
    }
    if !value.rumors.is_empty() {
        let items = value
            .rumors
            .iter()
            .map(|item| {
                format!(
                    "- id: {}\n  content: {}",
                    quoted(item.id.as_str()),
                    quoted(item.content.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("### Rumors\n\n{items}"));
    }
    sections.join("\n\n")
}

fn render_condition_queries(values: &[StoryStateExtractorConditionQueryPromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .map(|value| {
            format!(
                "- condition_key: {}\n  criterion: {}",
                quoted(value.condition_key.as_str()),
                quoted(value.criterion.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_validation_issues(values: &[StoryStateExtractorValidationIssuePromptView]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut lines = vec![format!("{}. Code: {}", index + 1, value.code)];
            if let Some(location) = value.location.as_ref() {
                let path = quoted(location.path.as_str());
                match location.item_index {
                    Some(item_index) => lines.push(format!("   Location: {path}\n   Item Index: {item_index}")),
                    None => lines.push(format!("   Location: {path}")),
                }
            }
            lines.push(format!("   Message: {}", quoted(value.message.as_str())));
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
#[path = "tests/story_state_extractor_prompt_tests.rs"]
mod tests;
