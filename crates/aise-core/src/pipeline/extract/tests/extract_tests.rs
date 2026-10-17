use super::*;
use crate::core::tests::snake_pack;
use crate::core::{NarrativeNodeId, NodeStatus, StoryAssetSpec, StoryPack, TurnCancellation};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, Provider,
};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct FixedProvider {
    calls: Arc<AtomicUsize>,
    content: String,
}

#[async_trait]
impl Provider for FixedProvider {
    fn name(&self) -> &'static str {
        "extract-fixed"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletionResponse {
            content: self.content.clone(),
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

async fn pipeline(pack: StoryPack, content: &str, calls: Arc<AtomicUsize>) -> (ExtractPipeline, StoryContext) {
    let story_ctx = StoryContext::new();
    let store = Arc::new(Store::default());
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryAssetSpec {
                pack_ref: story_ctx.pack_ref.clone(),
                pack,
                cards: Vec::new(),
            },
        )
        .await
        .expect("test pack should be retained");
    let gateway = Arc::new(LlmGateway::new(
        Arc::new(FixedProvider {
            calls,
            content: content.to_owned(),
        }),
        LlmConfig::default(),
    ));
    (
        ExtractPipeline::new(gateway, Arc::new(prompt()), store, ExtractConfig::default()),
        story_ctx,
    )
}

async fn run(pipeline: &ExtractPipeline, story_ctx: &StoryContext) -> ExtractOutput {
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();
    pipeline
        .execute(
            ExtractInput {
                proposal: "The gate opens.",
            },
            story_ctx,
            &control,
            trace.root(),
        )
        .await
        .expect("extract should succeed")
}

#[tokio::test]
async fn empty_queries_skip_llm_and_apply_deterministic_transition() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (pipeline, story_ctx) = pipeline(deterministic_pack(), "not json", Arc::clone(&calls)).await;

    let output = run(&pipeline, &story_ctx).await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let Change::Replaced(narrative) = output.narrative else {
        panic!("deterministic condition should activate the node");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("valid node id")),
        NodeStatus::Active
    );
}

#[tokio::test]
async fn semantic_query_calls_llm_once_and_completes_terminal_story() {
    let calls = Arc::new(AtomicUsize::new(0));
    let content = r#"{"evaluations":[{"condition_id":"gate_open","truth":"satisfied"}]}"#;
    let (pipeline, mut story_ctx) = pipeline(semantic_terminal_pack(), content, Arc::clone(&calls)).await;
    story_ctx
        .narrative
        .node_status
        .insert(NarrativeNodeId::try_new("arrival").expect("valid node id"), NodeStatus::Active);

    let output = run(&pipeline, &story_ctx).await;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let Change::Replaced(narrative) = output.narrative else {
        panic!("semantic condition should complete the node");
    };
    assert_eq!(
        narrative.status(&NarrativeNodeId::try_new("arrival").expect("valid node id")),
        NodeStatus::Completed
    );
    assert!(matches!(output.life_cycle, Change::Replaced(StoryLifeCycle::Completed)));
    assert!(output.knowledge_change.is_empty());
}
