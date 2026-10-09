use super::*;
use crate::core::{
    IdempotencyKey, PlayerContribution, Turn, TurnCancellation, TurnEvaluation, TurnEvent, TurnEventDeliveryError,
    TurnSegment, TurnStatus, WorldChange,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmProvider,
};
use crate::persistence::StoryStoreMem;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::path::Path;
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct FixedProvider {
    content: Result<String, &'static str>,
}

#[async_trait]
impl LlmProvider for FixedProvider {
    fn name(&self) -> &'static str {
        "fixed"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        match &self.content {
            Ok(content) => Ok(LlmCompletionResponse {
                content: content.clone(),
                finish_reason: LlmCompletionFinishReason::Stop,
            }),
            Err(message) => Err(LlmError::Transport {
                message: (*message).to_owned(),
            }),
        }
    }
}

struct NullSink;

impl TurnEventSink for NullSink {
    fn emit(&self, _event: TurnEvent) -> Result<(), TurnEventDeliveryError> {
        Ok(())
    }
}

fn pipeline(content: Result<String, &'static str>, summary_turn_count: usize) -> SummaryPipeline {
    let prompt = Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads");
    SummaryPipeline::new(
        Arc::new(LlmGateway::new(Arc::new(FixedProvider { content }), LlmConfig::default())),
        Arc::new(prompt),
        Arc::new(StoryStoreMem::new()),
        SummaryConfig { summary_turn_count },
    )
}

fn test_trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "summary-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn accepted_turn(number: u64) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("key-{number}")).expect("valid idempotency key"),
        player_contribution: PlayerContribution {
            raw: "raw".to_owned(),
            processed: format!("processed-{number}"),
        },
        turn_segment: TurnSegment::new(format!("Segment {number}.")),
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: TurnStatus::Accepted,
    }
}

fn story_context(turn_count: u64) -> StoryContext {
    let mut story_ctx = StoryContext::new();
    for number in 1..=turn_count {
        story_ctx.recent_turns.push_back(accepted_turn(number));
    }
    story_ctx.turn_number = TurnNumber::new(turn_count);
    story_ctx
}

async fn run(pipeline: &SummaryPipeline, story_ctx: &StoryContext) -> Result<SummaryOutput, PipelineError> {
    let trace = test_trace();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let input = SummaryInput {
        pending_turn_number: story_ctx.turn_number.increment(),
    };
    pipeline.execute(input, story_ctx, &control, &NullSink, trace.root()).await
}

#[test]
fn no_turns_are_covered_while_recent_turns_fit_the_window() {
    let pipeline = pipeline(Ok("unused".to_owned()), 6);

    assert_eq!(pipeline.covered_turn_count(&story_context(0)), 0);
    assert_eq!(pipeline.covered_turn_count(&story_context(5)), 0);
}

#[test]
fn overflow_beyond_the_window_after_commit_is_covered() {
    let pipeline = pipeline(Ok("unused".to_owned()), 6);

    assert_eq!(pipeline.covered_turn_count(&story_context(6)), 1);
    assert_eq!(pipeline.covered_turn_count(&story_context(8)), 3);
}

#[test]
fn zero_turn_count_disables_summary() {
    let pipeline = pipeline(Ok("unused".to_owned()), 0);

    assert_eq!(pipeline.covered_turn_count(&story_context(0)), 0);
    assert_eq!(pipeline.covered_turn_count(&story_context(3)), 0);
}

#[tokio::test]
async fn zero_turn_count_leaves_summary_unchanged_without_calling_llm() {
    let pipeline = pipeline(Err("must not be called"), 0);

    let output = run(&pipeline, &story_context(20)).await.expect("summary stage");

    assert!(matches!(output.summary, Change::Unchanged));
}

#[tokio::test]
async fn below_threshold_leaves_summary_unchanged() {
    let pipeline = pipeline(Ok("unused".to_owned()), 6);

    let output = run(&pipeline, &story_context(5)).await.expect("summary stage");

    assert!(matches!(output.summary, Change::Unchanged));
}

#[tokio::test]
async fn overflow_replaces_summary_and_advances_coverage() {
    let pipeline = pipeline(Ok("  Merged summary.  ".to_owned()), 6);

    let output = run(&pipeline, &story_context(7)).await.expect("summary stage");

    let Change::Replaced(summary) = output.summary else {
        panic!("summary must be replaced");
    };
    assert_eq!(summary.text, "Merged summary.");
    assert_eq!(summary.covered_through.value(), 2);
}

#[tokio::test]
async fn llm_failure_fails_the_stage() {
    let pipeline = pipeline(Err("boom"), 6);

    let error = run(&pipeline, &story_context(6)).await.err().expect("stage fails");

    assert!(error.to_string().contains("summary"));
    assert!(error.to_string().contains("boom"));
}

#[tokio::test]
async fn blank_summary_fails_the_stage() {
    let pipeline = pipeline(Ok("   ".to_owned()), 6);

    let error = run(&pipeline, &story_context(6)).await.err().expect("stage fails");

    assert!(error.to_string().contains("empty"));
}
