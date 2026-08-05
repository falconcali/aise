use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{ContextRequest, ContextSource, StoryGoal, WriterPlan};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::truncate;
use crate::domain::ids::CharacterId;
use crate::error::AiseError;
use crate::llm::gateway::LlmGateway;
use crate::llm::message::CompletionSpec;
use crate::prompt::ContextMerger;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

const MAX_RETRIEVAL_REQUESTS: usize = 4;
const MAX_CHARACTER_REQUESTS: usize = 4;
const MAX_QUERY_CHARS: usize = 200;
const MAX_GOAL_CHARS: usize = 300;
const MAX_PARSE_ERROR_PREVIEW_CHARS: usize = 200;

pub struct WriterPlanner {
    gateway: Arc<LlmGateway>,
    merger: ContextMerger,
}

impl WriterPlanner {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            merger: ContextMerger,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct PlanOutput {
    #[serde(default)]
    retrieval_requests: Vec<PlanRequest>,
    #[serde(default)]
    character_requests: Vec<CharacterId>,
    #[serde(default)]
    story_goal: PlanGoalOutput,
}

#[derive(Debug, Default, Deserialize)]
struct PlanGoalOutput {
    #[serde(default)]
    summary: String,
}

#[derive(Debug, Default, Deserialize)]
struct PlanRequest {
    #[serde(default)]
    query: String,
    #[serde(default)]
    sources: Vec<ContextSource>,
}

#[async_trait]
impl TurnExecutionPipeline for WriterPlanner {
    fn stage(&self) -> TurnStage {
        TurnStage::WriterPlanner
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| AiseError::InvariantViolation("baseline context not set before planning".into()))?;
        let messages = self.merger.plan_messages(baseline, ctx.player_input());
        let max_output = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        let spec = CompletionSpec {
            messages,
            max_output_tokens: max_output,
            purpose: "writer_plan",
        };
        let scope = ctx.llm_call_scope(TurnStage::WriterPlanner);
        let completion = self.gateway.complete(scope, spec).await?;
        let plan = parse_plan(&completion.text)?;
        ctx.set_writer_plan(plan)
    }
}

fn parse_plan(text: &str) -> Result<WriterPlan, AiseError> {
    let output: PlanOutput = serde_json::from_str(text).map_err(|error| {
        AiseError::Internal(format!(
            "writer plan output is not valid JSON: {error}; raw_output={}",
            truncate(text, MAX_PARSE_ERROR_PREVIEW_CHARS)
        ))
    })?;
    let mut retrieval_requests = Vec::new();
    for request in output.retrieval_requests.into_iter().take(MAX_RETRIEVAL_REQUESTS) {
        if request.query.trim().is_empty() {
            continue;
        }
        retrieval_requests.push(ContextRequest {
            query: request.query.chars().take(MAX_QUERY_CHARS).collect(),
            sources: request.sources,
        });
    }
    let mut seen = Vec::new();
    let mut character_requests = Vec::new();
    for character_id in output.character_requests {
        if character_id.as_str().is_empty() {
            continue;
        }
        if seen.contains(&character_id) {
            continue;
        }
        seen.push(character_id.clone());
        character_requests.push(character_id);
        if character_requests.len() >= MAX_CHARACTER_REQUESTS {
            break;
        }
    }
    Ok(WriterPlan {
        retrieval_requests,
        character_requests,
        story_goal: StoryGoal {
            summary: output.story_goal.summary.chars().take(MAX_GOAL_CHARS).collect(),
        },
    })
}

#[cfg(test)]
#[path = "tests/writer_planner_tests.rs"]
mod tests;
