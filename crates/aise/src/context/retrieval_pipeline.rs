use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{ContextItem, ContextSource};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use crate::error::AiseError;
use async_trait::async_trait;
use std::time::Instant;

#[derive(Default)]
pub struct ContextRetrievalPipeline;

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::ContextRetrieval
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| AiseError::InvariantViolation("baseline context not set before retrieval".into()))?;
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| AiseError::InvariantViolation("story snapshot not set before retrieval".into()))?;
        let requests = ctx
            .plan()
            .map(|plan| plan.retrieval_requests.clone())
            .ok_or_else(|| AiseError::InvariantViolation("writer plan not set before retrieval".into()))?;
        let limit = ctx.budget().max_retrieved_items();
        let mut candidates: Vec<ContextItem> = Vec::new();
        for request in requests {
            let sources = if request.sources.is_empty() {
                vec![
                    ContextSource::HistoricalStory,
                    ContextSource::WorldKnowledge,
                    ContextSource::CharacterMemory,
                ]
            } else {
                request.sources
            };
            for source in sources {
                let mut items = collect_source(snapshot, baseline, source);
                for item in items.drain(..) {
                    let score = if request.query.trim().is_empty() {
                        item.score
                    } else {
                        keyword_score(&request.query, &item.content)
                    };
                    candidates.push(ContextItem {
                        source: item.source,
                        content: item.content,
                        score,
                    });
                }
            }
        }
        candidates.sort_by(|left, right| right.score.partial_cmp(&left.score).unwrap_or(std::cmp::Ordering::Equal));
        let mut items: Vec<ContextItem> = Vec::new();
        for candidate in candidates {
            if items.len() >= limit {
                break;
            }
            if !items
                .iter()
                .any(|existing| existing.source == candidate.source && existing.content == candidate.content)
            {
                items.push(candidate);
            }
        }
        let pending = ctx.trace().begin_span("aise.tool_call", "context.retrieval");
        let started = Instant::now();
        let latency_ms = started.elapsed().as_millis() as u64;
        ctx.trace().end_span_with(
            pending,
            &SpanPayload::ToolCall(ToolCallData {
                tool: "context.retrieval".into(),
                args: serde_json::json!({ "limit": limit }),
                result: serde_json::json!({ "items": items.len() }),
                ok: true,
                latency_ms,
            }),
        );
        ctx.set_retrieved_context(items)
    }
}

fn collect_source(
    snapshot: &crate::core::turn_data::StoryReadSnapshot,
    baseline: &crate::core::turn_data::BaselineContext,
    source: ContextSource,
) -> Vec<ContextItem> {
    match source {
        ContextSource::HistoricalStory => baseline
            .recent_story
            .iter()
            .map(|text| ContextItem {
                source,
                content: text.clone(),
                score: 1.0,
            })
            .collect(),
        ContextSource::WorldKnowledge => snapshot
            .world()
            .map(|world| {
                world
                    .facts
                    .iter()
                    .map(|fact| ContextItem {
                        source,
                        content: fact.text.clone(),
                        score: 1.0,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ContextSource::CharacterMemory => snapshot
            .player_memories()
            .iter()
            .map(|memory| ContextItem {
                source,
                content: memory.content.clone(),
                score: 1.0,
            })
            .collect(),
        ContextSource::NarrativeGraph | ContextSource::LoreBook => Vec::new(),
    }
}

fn keyword_score(query: &str, content: &str) -> f32 {
    let query_terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if query_terms.is_empty() {
        return 1.0;
    }
    let lowered = content.to_lowercase();
    let matched = query_terms.iter().filter(|term| lowered.contains(term.as_str())).count();
    matched as f32 / query_terms.len() as f32
}

#[cfg(test)]
#[path = "tests/retrieval_pipeline_tests.rs"]
mod tests;
