use crate::config::{AssetLimitsConfig, PlannerConfig, RetrievalConfig};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::domain::asset::validation::BoundedText;
use crate::domain::narrative_graph::director::{NarrativeDirector, NarrativeEvaluation, NarrativeLimits};
use crate::llm::gateway::LlmGateway;
use crate::planning::error::PlanningError;
use crate::planning::planner_output::PlannerOutput;
use crate::planning::retrieval_plan_builder::RetrievalPlanBuilder;
use crate::prompt::{ModelRequest, WriterPlannerContext};
use async_trait::async_trait;
use std::sync::Arc;

pub struct WriterPlanner {
    gateway: Arc<LlmGateway>,
    narrative_director: NarrativeDirector,
    plan_builder: RetrievalPlanBuilder,
    config: PlannerConfig,
}

impl WriterPlanner {
    pub fn new(
        gateway: Arc<LlmGateway>,
        planner: PlannerConfig,
        retrieval: RetrievalConfig,
        assets: AssetLimitsConfig,
    ) -> Self {
        Self {
            gateway,
            narrative_director: NarrativeDirector::new(NarrativeLimits {
                max_nodes: assets.max_graph_nodes,
                max_edges: assets.max_graph_edges,
                max_condition_depth: assets.max_condition_depth,
                max_conditions_per_node: assets.max_conditions_per_node,
                max_effects_per_node: assets.max_effects_per_node,
            }),
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
        let pending = ctx.trace().begin_span("narrative.evaluate", "narrative.evaluate");
        let narrative_result = self.narrative_director.evaluate(NarrativeEvaluation {
            definition: snapshot.narrative_definition(),
            state: snapshot.narrative_state(),
            snapshot: &snapshot,
        });
        let narrative_payload = match &narrative_result {
            Ok(plan) => serde_json::json!({
                "story_id": ctx.story_id(),
                "turn_id": ctx.turn_id(),
                "graph_revision": snapshot.graph_revision(),
                "active_node_count": plan.active_nodes.len(),
                "transition_count": plan.proposed_transitions.len(),
                "intent_count": plan.global_event_intents.len(),
                "status": "ok",
                "error_code": null,
            }),
            Err(_) => serde_json::json!({
                "story_id": ctx.story_id(),
                "turn_id": ctx.turn_id(),
                "graph_revision": snapshot.graph_revision(),
                "active_node_count": 0,
                "transition_count": 0,
                "intent_count": 0,
                "status": "error",
                "error_code": "narrative_evaluation_failed",
            }),
        };
        ctx.trace().end_span_with(pending, &narrative_payload);
        let narrative_plan = narrative_result.map_err(PlanningError::from).map_err(map_planning_error)?;
        let player_input = BoundedText::try_new(
            ctx.player_input().to_owned(),
            "player_input",
            self.config.max_query_bytes.max(4096),
        )
        .map_err(|_| map_planning_error(PlanningError::LimitExceeded { limit: "player_input" }))?;
        let request = ModelRequest::writer_planner(
            WriterPlannerContext {
                baseline: baseline.clone(),
                narrative_plan: narrative_plan.clone(),
                player_input,
            },
            ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32,
        );
        let scope = ctx.llm_call_scope(TurnStage::WriterPlanner);
        let completion = self.gateway.complete_typed(scope, request).await.map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "llm_error",
                Some(TurnStage::WriterPlanner),
                error.to_string(),
            )
        })?;
        let planner_output: PlannerOutput = serde_json::from_str(&completion.text)
            .map_err(|_| map_planning_error(PlanningError::InvalidOutput { code: "invalid_json" }))?;
        let plan = self
            .plan_builder
            .build(&baseline, &narrative_plan, planner_output, &snapshot)
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
