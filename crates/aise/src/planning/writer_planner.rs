use crate::config::{NarrativeConfig, PlannerConfig, RetrievalConfig};
use crate::domain::asset::validation::BoundedText;
use crate::llm::gateway::LlmGateway;
use crate::planning::error::PlanningError;
use crate::planning::planner_output::writer_planner_contract;
use crate::planning::retrieval_plan_builder::RetrievalPlanBuilder;
use crate::planning::writer_planner_prompt::WriterPlannerPromptContextProjector;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;

pub struct WriterPlanner {
    gateway: Arc<LlmGateway>,
    plan_builder: RetrievalPlanBuilder,
    config: PlannerConfig,
}

impl WriterPlanner {
    pub fn new(
        gateway: Arc<LlmGateway>,
        planner: PlannerConfig,
        retrieval: RetrievalConfig,
        _narrative: &NarrativeConfig,
    ) -> Self {
        Self {
            gateway,
            plan_builder: RetrievalPlanBuilder::new(retrieval, planner.clone()),
            config: planner,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for WriterPlanner {
    fn stage(&self) -> TurnStage {
        TurnStage::WriterPlanner
    }

    fn observation_input(&self, ctx: &TurnExecutionContext) -> serde_json::Value {
        serde_json::json!({
            "phase": format!("{:?}", ctx.phase()).to_lowercase(),
            "has_baseline": ctx.baseline().is_some(),
            "has_narrative_projection": ctx.narrative_projection().is_some(),
            "remaining_output_tokens": ctx.budget().remaining_output_tokens()
        })
    }

    fn observation_output(&self, ctx: &TurnExecutionContext, succeeded: bool) -> serde_json::Value {
        let plan = ctx.plan();
        let serialized = plan.and_then(|value| serde_json::to_vec(value).ok());
        serde_json::json!({
            "completed": succeeded,
            "phase": format!("{:?}", ctx.phase()).to_lowercase(),
            "plan_bytes": serialized.as_ref().map_or(0, Vec::len),
            "plan_sha256": serialized.as_ref().map(|value| crate::turn::observability::sha256_hex(value)),
            "character_request_count": plan.map_or(0, |value| value.character_think_requests.len()),
            "knowledge_request_count": plan.map_or(0, |value| value.retrieval_plan.knowledge_requests.len()),
            "requires_retrieval": ctx.requires_retrieval().unwrap_or(false),
            "requires_character_thinking": ctx.requires_character_thinking().unwrap_or(false)
        })
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| {
                map_planning_error(PlanningError::InvalidOutput {
                    code: "missing_baseline",
                })
            })?
            .clone();
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| {
                map_planning_error(PlanningError::InvalidOutput {
                    code: "missing_snapshot",
                })
            })?
            .clone();
        let narrative_projection = ctx
            .narrative_projection()
            .ok_or_else(|| {
                map_planning_error(PlanningError::InvalidOutput {
                    code: "missing_narrative_projection",
                })
            })?
            .clone();
        let narrative_plan = narrative_projection.plan.clone();
        let player_contribution = BoundedText::try_new(
            ctx.player_contribution().to_owned(),
            "player_contribution",
            crate::turn::turn_contract::MAX_PLAYER_CONTRIBUTION_CHARS,
        )
        .map_err(|_| {
            map_planning_error(PlanningError::LimitExceeded {
                limit: "player_contribution",
            })
        })?;
        let prompt_projection = WriterPlannerPromptContextProjector
            .project(
                &baseline,
                &narrative_plan,
                &player_contribution,
                ctx.budget().max_context_tokens(),
            )
            .map_err(|error| {
                let code = match &error {
                    crate::planning::WriterPlannerProjectionError::UnknownRoleTarget { .. } => "unknown_role_target",
                    crate::planning::WriterPlannerProjectionError::PlayerRoleTarget { .. } => "player_role_target",
                    crate::planning::WriterPlannerProjectionError::DuplicateRoleTarget { .. } => {
                        "duplicate_role_target"
                    }
                    crate::planning::WriterPlannerProjectionError::RetrievalTargetCollision { .. } => {
                        "retrieval_target_collision"
                    }
                    crate::planning::WriterPlannerProjectionError::RequiredPromptDataExceedsBudget => {
                        "required_prompt_data_exceeds_budget"
                    }
                };
                TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    code,
                    Some(TurnStage::WriterPlanner),
                    error.to_string(),
                )
            })?;
        let request = PromptCompositionInput {
            profile: PromptProfile::WriterPlanner,
            rc_vars: prompt_projection.rc_vars,
            fti_vars: prompt_projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        let scope = ctx.llm_call_scope(TurnStage::WriterPlanner);
        let structured = self
            .gateway
            .complete_structured_composed(
                scope,
                request,
                max_output_tokens,
                crate::turn::turn_contract::LlmCallPurpose::WriterPlan,
                writer_planner_contract(&self.config),
            )
            .await
            .map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "llm_error",
                    Some(TurnStage::WriterPlanner),
                    error.to_string(),
                )
            })?;
        let planner_output = structured.value;
        let plan = self
            .plan_builder
            .build(
                &baseline,
                &narrative_plan,
                planner_output,
                &snapshot,
                &prompt_projection.context,
            )
            .map_err(map_planning_error)?;
        ctx.set_writer_plan(plan)
    }
}

fn map_planning_error(error: PlanningError) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        error.turn_code(),
        Some(TurnStage::WriterPlanner),
        error.to_string(),
    )
}

#[cfg(test)]
#[path = "tests/writer_planner_tests.rs"]
mod tests;
