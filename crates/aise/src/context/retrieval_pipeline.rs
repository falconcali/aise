use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{ContextItem, ContextSource};
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use async_trait::async_trait;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::time::Instant;

#[derive(Default)]
pub struct ContextRetrievalPipeline;

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::ContextRetrieval
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before retrieval"))?;
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| invariant("story snapshot not set before retrieval"))?;
        let requests = ctx
            .plan()
            .map(|plan| plan.retrieval_requests.clone())
            .ok_or_else(|| invariant("writer plan not set before retrieval"))?;
        let limit = ctx.budget().max_retrieved_items();
        let max_candidates = ctx.budget().max_retrieval_candidates().max(1);
        let mut heap: BinaryHeap<Reverse<RankedItem>> = BinaryHeap::with_capacity(max_candidates.min(64));
        let mut sequence = 0usize;
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
                for item in collect_source(snapshot, baseline, source) {
                    let score = if request.query.trim().is_empty() {
                        item.score
                    } else {
                        keyword_score(&request.query, &item.content)
                    };
                    let ranked = RankedItem {
                        score,
                        tiebreak: sequence,
                        item: ContextItem {
                            source: item.source,
                            content: item.content,
                            score,
                        },
                    };
                    sequence += 1;
                    heap.push(Reverse(ranked));
                    if heap.len() > max_candidates {
                        heap.pop();
                    }
                }
            }
        }
        let mut ranked: Vec<RankedItem> = heap.into_iter().map(|Reverse(ranked)| ranked).collect();
        ranked.sort_by(|left, right| right.cmp(left));
        let mut items: Vec<ContextItem> = Vec::new();
        for candidate in ranked {
            if items.len() >= limit {
                break;
            }
            if !items
                .iter()
                .any(|existing| existing.source == candidate.item.source && existing.content == candidate.item.content)
            {
                items.push(candidate.item);
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

struct RankedItem {
    score: f32,
    tiebreak: usize,
    item: ContextItem,
}

impl PartialEq for RankedItem {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.tiebreak == other.tiebreak
    }
}

impl Eq for RankedItem {}

impl PartialOrd for RankedItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.tiebreak.cmp(&self.tiebreak))
    }
}

fn collect_source(
    snapshot: &crate::domain::story_state::StoryReadSnapshot,
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

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}

#[cfg(test)]
#[path = "tests/retrieval_pipeline_tests.rs"]
mod tests;
