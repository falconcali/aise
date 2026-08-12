use crate::config::PlannerConfig;
use crate::domain::asset::constraint::StoryConstraintRequirement;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::narrative_graph::director::NarrativePlan;
use crate::domain::story_instance::binding::RoleController;
use crate::domain::story_instance::state::CastPolicy;
use crate::domain::turn::{BaselineContext, CharacterView, RetrievalTargetId};
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub const WRITER_PLANNER_CSI_SLOT: &str = "context.writer_planner.csi";
pub const WRITER_PLANNER_RC_SLOT: &str = "context.writer_planner.rc";
pub const WRITER_PLANNER_FTI_SLOT: &str = "context.writer_planner.fti";

#[derive(Debug, Clone)]
pub struct WriterPlannerPromptContext {
    pub character_targets: BTreeMap<RetrievalTargetId, crate::domain::ids::CharacterId>,
    pub knowledge_targets: BTreeMap<RetrievalTargetId, crate::domain::knowledge::KnowledgeSourceId>,
    pub provided_character_ids: Vec<crate::domain::ids::CharacterId>,
    pub provided_knowledge_ids: Vec<crate::domain::knowledge::KnowledgeSourceId>,
}

pub struct WriterPlannerPromptProjection {
    pub context: WriterPlannerPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

pub struct WriterPlannerPromptContextProjector;

impl WriterPlannerPromptContextProjector {
    pub fn project(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        player_input: &BoundedText,
        config: &PlannerConfig,
    ) -> WriterPlannerPromptProjection {
        let mut character_targets = BTreeMap::new();
        let character_index = render_character_index(baseline, &mut character_targets);
        let mut knowledge_targets = BTreeMap::new();
        let knowledge_entry_index = render_knowledge_index(baseline, &mut knowledge_targets);
        let provided_character_ids = std::iter::once(baseline.player_character.character_id.clone())
            .chain(baseline.scene_characters.iter().map(|character| character.character_id.clone()))
            .chain(
                baseline
                    .referenced_characters
                    .iter()
                    .map(|character| character.character_id.clone()),
            )
            .collect();
        let provided_knowledge_ids = baseline.relevant_knowledge.iter().map(|entry| entry.entry_id.clone()).collect();
        let continuity = &baseline.story_continuity;
        let rc_vars = HashMap::from([
            ("story_profile".into(), Value::String(render_story_profile(baseline))),
            (
                "instance_settings".into(),
                Value::String(render_instance_settings(baseline.instance_settings.cast_policy)),
            ),
            (
                "story_summary".into(),
                Value::String(render_optional_text(continuity.summary().text.as_str())),
            ),
            ("recent_story".into(), Value::String(render_recent_story(baseline))),
            ("current_scene".into(), Value::String(render_current_scene(baseline))),
            (
                "player_character".into(),
                Value::String(render_character(&baseline.player_character, "player", None)),
            ),
            (
                "scene_characters".into(),
                Value::String(render_characters(&baseline.scene_characters, "ai")),
            ),
            (
                "referenced_characters".into(),
                Value::String(render_referenced_characters(baseline)),
            ),
            ("relevant_knowledge".into(), Value::String(render_relevant_knowledge(baseline))),
            ("character_index".into(), Value::String(character_index)),
            ("knowledge_entry_index".into(), Value::String(knowledge_entry_index)),
            ("narrative_plan".into(), Value::String(render_narrative_plan(narrative_plan))),
            ("active_story_constraints".into(), Value::String(render_constraints(baseline))),
            ("player_input".into(), Value::String(render_data(player_input.as_str()))),
        ]);
        let fti_vars = HashMap::from([(
            "output_schema".into(),
            Value::String(writer_planner_output_schema(config).to_string()),
        )]);
        WriterPlannerPromptProjection {
            context: WriterPlannerPromptContext {
                character_targets,
                knowledge_targets,
                provided_character_ids,
                provided_knowledge_ids,
            },
            rc_vars: RuntimePromptVars::new(rc_vars),
            fti_vars: TrustedPromptVars::new(fti_vars),
        }
    }
}

pub fn writer_planner_output_schema(config: &PlannerConfig) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["story_goal", "context_gaps", "character_think_requests"],
        "properties": {
            "story_goal": {"type": "string", "minLength": 1, "maxLength": config.max_goal_bytes},
            "context_gaps": {
                "type": "array",
                "maxItems": config.max_context_gaps,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["audience", "target_id", "query_text", "reason"],
                    "properties": {
                        "audience": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["kind"],
                                    "properties": {"kind": {"const": "global_writer"}}
                                },
                                {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["kind", "character_id"],
                                    "properties": {
                                        "kind": {"const": "character"},
                                        "character_id": {"type": "string", "minLength": 1}
                                    }
                                }
                            ]
                        },
                        "target_id": {"type": ["string", "null"], "maxLength": 256},
                        "query_text": {"type": ["string", "null"], "maxLength": config.max_query_bytes},
                        "reason": {"type": "string", "minLength": 1, "maxLength": config.max_reason_bytes}
                    },
                    "oneOf": [
                        {
                            "properties": {
                                "target_id": {"type": "string", "minLength": 1},
                                "query_text": {"type": "null"}
                            }
                        },
                        {
                            "properties": {
                                "target_id": {"type": "null"},
                                "query_text": {"type": "string", "minLength": 1}
                            }
                        }
                    ]
                }
            },
            "character_think_requests": {
                "type": "array",
                "maxItems": config.max_character_think_requests,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["character_id", "reason"],
                    "properties": {
                        "character_id": {"type": "string", "minLength": 1},
                        "reason": {"type": "string", "minLength": 1, "maxLength": config.max_reason_bytes}
                    }
                }
            }
        }
    })
}

fn render_story_profile(baseline: &BaselineContext) -> String {
    let profile = &baseline.story_profile;
    [
        field("premise", profile.premise.as_str()),
        field("language", profile.language.as_str()),
        list_field("genre", &profile.genre),
        list_field("themes", &profile.themes),
        list_field("tone", &profile.style.tone),
        field("point_of_view", profile.style.point_of_view.as_str()),
        field("tense", profile.style.tense.as_str()),
    ]
    .join("\n")
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
    let segments = baseline.story_continuity.recent_segments();
    if segments.is_empty() {
        return "None.".into();
    }
    segments
        .iter()
        .map(|segment| {
            format!(
                "- sequence: {}\n  text: {}",
                segment.sequence.get(),
                quoted(segment.text.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_current_scene(baseline: &BaselineContext) -> String {
    let scene = &baseline.current_scene;
    [
        format!("location: {}", quoted(scene.location_key.as_str())),
        field("time", scene.time.as_str()),
        field("immediate_situation", scene.description.as_str()),
    ]
    .join("\n")
}

fn render_characters(characters: &[CharacterView], control: &str) -> String {
    if characters.is_empty() {
        return "None.".into();
    }
    characters
        .iter()
        .map(|character| render_character(character, control, Some("- ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_referenced_characters(baseline: &BaselineContext) -> String {
    if baseline.referenced_characters.is_empty() {
        return "None.".into();
    }
    baseline
        .referenced_characters
        .iter()
        .map(|character| {
            format!(
                "{}\n  presence: referenced_off_scene",
                render_character(character, "ai", Some("- "))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_relevant_knowledge(baseline: &BaselineContext) -> String {
    if baseline.relevant_knowledge.is_empty() {
        return "None.".into();
    }
    baseline
        .relevant_knowledge
        .iter()
        .map(|entry| {
            let (kind, scope) = match entry.kind {
                crate::domain::knowledge::KnowledgeKind::Fact => ("fact", "objective_world"),
                crate::domain::knowledge::KnowledgeKind::Rumor => ("rumor", "public_claim"),
                crate::domain::knowledge::KnowledgeKind::Memory => ("memory", "character_limited"),
            };
            format!(
                "- entry_id: {}\n  title: {}\n  kind: {kind}\n  scope: {scope}\n  content: {}",
                quoted(entry.entry_id.as_str()),
                quoted(entry.entry_id.as_str()),
                quoted(entry.content.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_character(character: &CharacterView, control: &str, prefix: Option<&str>) -> String {
    let first = prefix.unwrap_or_default();
    let profile = &character.card.profile;
    [
        format!("{first}character_id: {}", quoted(character.character_id.as_str())),
        format!("  name: {}", quoted(character.card.meta.name.as_str())),
        format!("  story_role: {}", quoted(character.role.role_label.as_str())),
        format!("  control: {control}"),
        format!("  description: {}", quoted(profile.description.as_str())),
        format!("  personality: {}", quoted_list(&profile.personality)),
        format!("  values: {}", quoted_list(&profile.values)),
        format!("  location: {}", quoted(character.state.location.as_str())),
        format!("  goals: {}", quoted_list(&character.state.goals)),
        format!("  attributes: {}", render_attributes(&character.state.attributes)),
    ]
    .join("\n")
    .trim_start()
    .to_owned()
}

fn render_character_index(
    baseline: &BaselineContext,
    targets: &mut BTreeMap<RetrievalTargetId, crate::domain::ids::CharacterId>,
) -> String {
    let scope = match baseline.character_index_scope {
        crate::domain::turn::RetrievalIndexScope::Complete => "complete",
        crate::domain::turn::RetrievalIndexScope::Prefiltered => "prefiltered",
    };
    if baseline.character_index.is_empty() {
        return format!("scope: {scope}\nentries: None.");
    }
    let entries = baseline
        .character_index
        .iter()
        .filter(|entry| !entry.player_controlled)
        .map(|entry| {
            let target_id = RetrievalTargetId::for_character(&entry.character_id);
            targets.insert(target_id.clone(), entry.character_id.clone());
            format!(
                "- target_id: {}\n  character_id: {}\n  name: {}\n  role: {}\n  control: ai\n  retrieval_hint: {}",
                quoted(target_id.as_str()),
                quoted(entry.character_id.as_str()),
                quoted(entry.name.as_str()),
                quoted(entry.role_key.as_str()),
                quoted(entry.narrative_function.as_str())
            )
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        format!("scope: {scope}\nentries: None.")
    } else {
        format!("scope: {scope}\nentries:\n{}", entries.join("\n"))
    }
}

fn render_knowledge_index(
    baseline: &BaselineContext,
    targets: &mut BTreeMap<RetrievalTargetId, crate::domain::knowledge::KnowledgeSourceId>,
) -> String {
    let scope = match baseline.knowledge_entry_index_scope {
        crate::domain::turn::RetrievalIndexScope::Complete => "complete",
        crate::domain::turn::RetrievalIndexScope::Prefiltered => "prefiltered",
    };
    if baseline.knowledge_entry_index.is_empty() {
        return format!("scope: {scope}\nentries: None.");
    }
    let entries = baseline
        .knowledge_entry_index
        .iter()
        .map(|entry| {
            targets.insert(entry.target_id.clone(), entry.entry_id.clone());
            let kind = match entry.kind {
                crate::domain::knowledge::KnowledgeKind::Fact => "fact",
                crate::domain::knowledge::KnowledgeKind::Rumor => "rumor",
                crate::domain::knowledge::KnowledgeKind::Memory => "memory",
            };
            format!(
                "- target_id: {}\n  title: {}\n  kind: {kind}\n  retrieval_hint: {}",
                quoted(entry.target_id.as_str()),
                quoted(entry.entry_id.as_str()),
                quoted(entry.retrieval_hint.as_str())
            )
        })
        .collect::<Vec<_>>();
    format!("scope: {scope}\nentries:\n{}", entries.join("\n"))
}

fn render_narrative_plan(plan: &NarrativePlan) -> String {
    if plan.active_goals.is_empty()
        && plan.global_event_intents.is_empty()
        && plan.character_impulses.is_empty()
        && plan.proposed_transitions.is_empty()
    {
        return "None.".into();
    }
    let goals = plan
        .active_goals
        .iter()
        .map(|goal| quoted(goal.summary.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let impulses = plan
        .character_impulses
        .iter()
        .map(|impulse| {
            format!(
                "{}: {}",
                quoted(impulse.target_character_id.as_str()),
                quoted(impulse.goal.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "active_goals: [{}]\ncharacter_impulses: [{}]\nevent_intent_count: {}\ncandidate_transition_count: {}",
        goals,
        impulses,
        plan.global_event_intents.len(),
        plan.proposed_transitions.len()
    )
}

fn render_constraints(baseline: &BaselineContext) -> String {
    if baseline.active_story_constraints.is_empty() {
        return "None.".into();
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

fn render_optional_text(value: &str) -> String {
    if value.trim().is_empty() {
        "None.".into()
    } else {
        render_data(value)
    }
}

fn render_data(value: &str) -> String {
    quoted(value)
}

fn field(name: &str, value: &str) -> String {
    format!("{name}: {}", quoted(value))
}

fn list_field(name: &str, values: &[BoundedText]) -> String {
    format!("{name}: {}", quoted_list(values))
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

pub fn is_ai_character(baseline: &BaselineContext, character_id: &crate::domain::ids::CharacterId) -> bool {
    baseline
        .scene_characters
        .iter()
        .find(|character| &character.character_id == character_id)
        .is_some_and(|character| {
            matches!(character.binding.controller, RoleController::Ai)
                && baseline.current_scene.present_character_ids.contains(character_id)
        })
}
