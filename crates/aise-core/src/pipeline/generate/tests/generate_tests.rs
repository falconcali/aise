use super::*;
use crate::core::tests::story_context;
use crate::core::{ActivatedKnowledge, PlayerContribution, TurnCancellation};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, Provider,
};
use crate::pipeline::ValidateConfig;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct GenerateProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl Provider for GenerateProvider {
    fn name(&self) -> &'static str {
        "generate"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletionResponse {
            content: "Generated story.".to_owned(),
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
        name: "generate-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

#[tokio::test]
async fn execute_calls_gateway_once_and_returns_story() {
    let provider = Arc::new(GenerateProvider {
        calls: AtomicUsize::new(0),
    });
    let validate_config = ValidateConfig::default();
    let pipeline = GeneratePipeline::new(
        Arc::new(LlmGateway::new(provider.clone(), LlmConfig::default())),
        Arc::new(prompt()),
        GenerateConfig::default(),
        Arc::new(validate_config.quality_policy()),
    );
    let contribution = PlayerContribution {
        raw: "I walk.".to_owned(),
        processed: "The player walks.".to_owned(),
    };
    let knowledge = ActivatedKnowledge::default();
    let input = GenerateInput {
        story_goal: "Reach the gate.",
        player_contribution: &contribution,
        directions: &[],
        constraints: &[],
        knowledge: &knowledge,
        retrieved_facts: &[],
        retrieved_rumors: &[],
        role_ids: &[],
        character_decisions: &[],
    };
    let story_ctx = story_context();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let output = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .expect("generate succeeds");

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(output.result, "Generated story.");
}
