use crate::config::{NarrativeConfig, PlannerConfig, RetrievalConfig};
use crate::domain::asset::validation::BoundedText;
use crate::domain::narrative_graph::projector::{NarrativeProjectionInput, NarrativeProjector};
use crate::domain::narrative_graph::state_view::CommittedNarrativeStateView;
use crate::llm::gateway::LlmGateway;
use crate::planning::error::PlanningError;
use crate::planning::planner_output::WriterPlannerOutputDto;
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
    narrative_projector: NarrativeProjector,
    plan_builder: RetrievalPlanBuilder,
    config: PlannerConfig,
}

impl WriterPlanner {
    pub fn new(
        gateway: Arc<LlmGateway>,
        planner: PlannerConfig,
        retrieval: RetrievalConfig,
        narrative: &NarrativeConfig,
    ) -> Self {
        Self {
            gateway,
            narrative_projector: NarrativeProjector::new(narrative.as_limits()),
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
        let pending = ctx.trace().begin_span("narrative.project", "narrative.project");
        let committed_view = CommittedNarrativeStateView::new(&snapshot);
        let current_turn = snapshot.base_revision().get().saturating_add(1);
        let projection_result = self.narrative_projector.project(NarrativeProjectionInput {
            definition: snapshot.narrative_definition(),
            state: snapshot.narrative_state(),
            committed_view: &committed_view,
            current_turn,
        });
        let narrative_payload = match &projection_result {
            Ok(projection) => serde_json::json!({
                "story_id": ctx.story_id(),
                "turn_number": ctx.turn_number().get(),
                "graph_revision": snapshot.graph_revision(),
                "active_node_count": projection.plan.active_nodes.len(),
                "condition_query_count": projection.condition_queries.len(),
                "intent_count": projection.plan.world_event_intents.len(),
                "status": "ok",
                "error_code": null,
            }),
            Err(_) => serde_json::json!({
                "story_id": ctx.story_id(),
                "turn_number": ctx.turn_number().get(),
                "graph_revision": snapshot.graph_revision(),
                "active_node_count": 0,
                "condition_query_count": 0,
                "intent_count": 0,
                "status": "error",
                "error_code": "narrative_projection_failed",
            }),
        };
        ctx.trace().end_span_with(pending, &narrative_payload);
        let projection = projection_result.map_err(PlanningError::from).map_err(map_planning_error)?;
        let narrative_plan = projection.plan.clone();
        ctx.set_narrative_projection(projection)?;
        let player_input = BoundedText::try_new(
            ctx.player_input().to_owned(),
            "player_input",
            crate::turn::turn_contract::MAX_PLAYER_INPUT_CHARS,
        )
        .map_err(|_| map_planning_error(PlanningError::LimitExceeded { limit: "player_input" }))?;
        let projection = WriterPlannerPromptContextProjector
            .project(
                &baseline,
                &narrative_plan,
                &player_input,
                &self.config,
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
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        let scope = ctx.llm_call_scope(TurnStage::WriterPlanner);
        let completion = self
            .gateway
            .complete_composed(
                scope,
                request,
                max_output_tokens,
                crate::turn::turn_contract::LlmCallPurpose::WriterPlan,
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
        let planner_output: WriterPlannerOutputDto = serde_json::from_str(&completion.text)
            .map_err(|_| map_planning_error(PlanningError::InvalidOutput { code: "invalid_json" }))?;
        let plan = self
            .plan_builder
            .build(&baseline, &narrative_plan, planner_output, &snapshot, &projection.context)
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
