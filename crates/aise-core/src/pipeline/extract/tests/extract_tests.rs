use super::*;
use crate::core::tests::{snake_pack, story_context};
use crate::core::{
    EntryOrigin, FactChangeOp, FactEntry, MemoryEntry, NarrativeNodeId, NodeStatus, RoleEntry, RoleMemoryChangeOp,
    RumorEntry, StoryPack, StoryPackSnapshot, TurnCancellation,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, Provider,
};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct RoutingProvider {
    calls: Arc<AtomicUsize>,
    narrative_response: String,
    world_response: String,
    role_responses: BTreeMap<String, String>,
    in_flight_memory: Arc<AtomicUsize>,
    max_in_flight_memory: Arc<AtomicUsize>,
    recorded_prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Provider for RoutingProvider {
    fn name(&self) -> &'static str {
        "extract-routing"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = request
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.recorded_prompts.lock().expect("lock").push(text.clone());

        let content = if text.contains("# Semantic Queries") {
            self.narrative_response.clone()
        } else if text.contains("# Existing Facts") {
            self.world_response.clone()
        } else if text.contains("# Target Role") {
            let current = self.in_flight_memory.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight_memory.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(15)).await;
            self.in_flight_memory.fetch_sub(1, Ordering::SeqCst);
            self.role_responses
                .iter()
                .find(|(role_id, _)| text.contains(&format!("- role_id: {role_id}")))
                .map(|(_, response)| response.clone())
                .unwrap_or_else(|| r#"{"memories":[]}"#.to_owned())
        } else {
            return Err(LlmError::InvalidResponse {
                reason: "unrecognized extract prompt",
            });
        };

        Ok(LlmCompletionResponse {
            content,
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
}

fn prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads")
}

fn trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::MetadataOnly,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "extract-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn deterministic_pack() -> StoryPack {
    let mut pack = snake_pack();
    pack.narrative = Some(
        serde_json::from_value(json!({ "nodes": {
            "arrival": {
                "title": "Arrival",
                "activate_when": { "type": "story_started" },
                "complete_when": { "type": "turn_reaches", "turn": 10 }
            }
        }}))
        .expect("test graph should deserialize"),
    );
    pack
}

fn semantic_terminal_pack() -> StoryPack {
    let mut pack = snake_pack();
    pack.narrative = Some(
        serde_json::from_value(json!({ "nodes": {
            "arrival": {
                "title": "Arrival",
                "activate_when": { "type": "story_started" },
                "complete_when": {
                    "type": "semantic",
                    "condition_id": "gate_open",
                    "criterion": "The gate is open"
                },
                "terminal": true
            }
        }}))
        .expect("test graph should deserialize"),
    );
    pack
}

fn seeded_context(pack: &StoryPack) -> StoryContext {
    let mut story_ctx = story_context();
    story_ctx.pack_ref = pack.pack_ref().expect("pack ref computes");
    story_ctx.opening = Arc::new(pack.opening.clone());
    story_ctx.facts = pack
        .seed_facts
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                FactEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    story_ctx.rumors = pack
        .seed_rumors
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RumorEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    story_ctx.roles = pack
        .roles
        .iter()
        .map(|(id, body)| {
            let memories = body
                .seed_memories
                .iter()
                .map(|(mem_id, mem_body)| {
                    (
                        mem_id.clone(),
                        MemoryEntry {
                            id: mem_id.clone(),
                            origin: EntryOrigin::Seed,
                            body: mem_body.clone(),
                        },
                    )
                })
                .collect();
            (
                id.clone(),
                RoleEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: body.spec.clone(),
                    profile: body.default_profile.clone(),
                    memories,
                },
            )
        })
        .collect();
    story_ctx
}

async fn setup_pipeline(
    pack: StoryPack,
    config: ExtractConfig,
    provider: RoutingProvider,
) -> (ExtractPipeline, StoryContext) {
    let story_ctx = seeded_context(&pack);
    let store = Arc::new(Store::default());
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryPackSnapshot::from_pack(pack).expect("snapshot builds"),
        )
        .await
        .expect("test pack should be retained");
    let gateway = Arc::new(LlmGateway::new(Arc::new(provider), LlmConfig::default()));
    (ExtractPipeline::new(gateway, Arc::new(prompt()), store, config), story_ctx)
}

async fn run(pipeline: &ExtractPipeline, story_ctx: &StoryContext, proposal: &str) -> ExtractOutput {
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();
    pipeline
        .execute(ExtractInput { proposal }, story_ctx, &control, trace.root())
        .await
        .expect("extract should succeed")
}

#[tokio::test]
async fn empty_queries_and_empty_roles_for_memory_call_world_knowledge_only() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = RoutingProvider {
        calls: Arc::clone(&calls),
        narrative_response: "unused".to_owned(),
        world_response: r#"{"facts":[],"rumors":[],"roles_for_memory":[]}"#.to_owned(),
        role_responses: BTreeMap::new(),
        in_flight_memory: Arc::new(AtomicUsize::new(0)),
        max_in_flight_memory: Arc::new(AtomicUsize::new(0)),
        recorded_prompts: Arc::new(Mutex::new(Vec::new())),
    };
    let (pipeline, story_ctx) = setup_pipeline(deterministic_pack(), ExtractConfig::default(), provider).await;

    let output = run(&pipeline, &story_ctx, "The gate opens.").await;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let Change::Replaced(narrative) = output.narrative else {
        panic!("deterministic condition should activate the node");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("valid node id")),
        NodeStatus::Active
    );
    assert!(output.knowledge_change.is_empty());
}

#[tokio::test]
async fn concurrent_narrative_and_world_extraction_with_bounded_role_memories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let recorded_prompts = Arc::new(Mutex::new(Vec::new()));
    let world_response = json!({
        "facts": [{
            "op": "add",
            "body": {
                "content": "Bai Suzhen lent her oil-paper umbrella to Xu Xian at Broken Bridge.",
                "retrieval": "Umbrella lent at Broken Bridge",
                "activation": { "match": { "keys": ["umbrella", "Broken Bridge"] } }
            }
        }],
        "rumors": [],
        "roles_for_memory": ["bai_suzhen", "xu_xian", "xiaoqing"]
    })
    .to_string();
    let role_responses = BTreeMap::from([
        (
            "bai_suzhen".to_owned(),
            json!({
                "memories": [{
                    "op": "add",
                    "body": {
                        "content": "I handed my umbrella to Xu Xian in the rain.",
                        "retrieval": "Handing umbrella to Xu Xian",
                        "activation": { "match": { "keys": ["umbrella", "Xu Xian"] } }
                    }
                }]
            })
            .to_string(),
        ),
        (
            "xu_xian".to_owned(),
            json!({
                "memories": [{
                    "op": "add",
                    "body": {
                        "content": "Lady Bai kindly lent me her umbrella by West Lake.",
                        "retrieval": "Borrowing Lady Bai's umbrella",
                        "activation": { "match": { "keys": ["umbrella", "Lady Bai"] } }
                    }
                }]
            })
            .to_string(),
        ),
        ("xiaoqing".to_owned(), r#"{"memories":[]}"#.to_owned()),
    ]);
    let provider = RoutingProvider {
        calls: Arc::clone(&calls),
        narrative_response: r#"{"evaluations":[{"condition_id":"gate_open","truth":"satisfied"}]}"#.to_owned(),
        world_response,
        role_responses,
        in_flight_memory: Arc::new(AtomicUsize::new(0)),
        max_in_flight_memory: Arc::clone(&max_in_flight),
        recorded_prompts: Arc::clone(&recorded_prompts),
    };
    let config = ExtractConfig {
        max_memory_concurrency: NonZeroUsize::new(2).expect("non-zero"),
        ..ExtractConfig::default()
    };
    let (pipeline, mut story_ctx) = setup_pipeline(semantic_terminal_pack(), config, provider).await;
    story_ctx
        .narrative
        .node_status
        .insert(NarrativeNodeId::try_new("arrival").expect("valid node id"), NodeStatus::Active);

    let output = run(
        &pipeline,
        &story_ctx,
        "While Fahai was absent, Bai Suzhen whispered to Xiaoqing and lent her umbrella to Xu Xian.",
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 5);
    assert!(max_in_flight.load(Ordering::SeqCst) <= 2);
    let Change::Replaced(narrative) = output.narrative else {
        panic!("semantic condition should complete the node");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("valid node id")),
        NodeStatus::Completed
    );
    assert!(matches!(output.life_cycle, Change::Replaced(StoryLifeCycle::Completed)));
    assert_eq!(output.knowledge_change.facts.len(), 1);
    assert!(matches!(
        &output.knowledge_change.facts[0],
        FactChangeOp::Add { id, .. } if id.as_str() == "fact_t1_1"
    ));
    assert_eq!(output.knowledge_change.memories.len(), 2);
    assert!(matches!(
        &output.knowledge_change.memories[0],
        RoleMemoryChangeOp::Add { role_id, id, .. } if role_id.as_str() == "bai_suzhen" && id.as_str() == "mem_t1_1"
    ));
    assert!(matches!(
        &output.knowledge_change.memories[1],
        RoleMemoryChangeOp::Add { role_id, id, .. } if role_id.as_str() == "xu_xian" && id.as_str() == "mem_t1_1"
    ));
}
