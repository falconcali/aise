use aise_core::core::{
    IdempotencyKey, StoryId, TurnCancellation, TurnControl, TurnEvent, TurnEventDeliveryError, TurnEventSink,
    TurnRequest,
};
use aise_core::engine::{AiseEngine, Engine};
use aise_core::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, ObservationStatus,
    SessionOutcome, SessionSpec, Trace, TraceOutcome, TraceSpec,
};
use anyhow::Context;
use std::time::{Duration, Instant};

const DEFAULT_STORY_ID: &str = "default-story";
const DEFAULT_IDEMPOTENCY_KEY: &str = "default-turn";
const DEFAULT_PLAYER_INPUT: &str = "继续这个故事。";
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(30);

struct ConsoleTurnEventSink;

impl TurnEventSink for ConsoleTurnEventSink {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError> {
        println!("{event:?}");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let request = TurnRequest {
        story_id: StoryId::try_new(DEFAULT_STORY_ID).context("invalid default story id")?,
        idempotency_key: IdempotencyKey::try_new(DEFAULT_IDEMPOTENCY_KEY).context("invalid default idempotency key")?,
        player_input: DEFAULT_PLAYER_INPUT.to_owned(),
    };
    let control = TurnControl::new(Instant::now() + DEFAULT_TURN_TIMEOUT, TurnCancellation::new());
    let (session, trace) = begin_default_trace();
    let result = AiseEngine::new()
        .run_turn(request, control, &ConsoleTurnEventSink, &trace)
        .await;
    let status = if result.is_ok() {
        ObservationStatus::Ok
    } else {
        ObservationStatus::Error
    };
    trace.finish(TraceOutcome {
        status,
        ..TraceOutcome::default()
    });
    session.finish(SessionOutcome {
        status,
        metadata: Vec::new(),
    });
    let result = result.context("default turn failed")?;

    println!("{}", result.result.story_text);
    Ok(())
}

fn begin_default_trace() -> (ObservationSession, Trace) {
    let content = ContentCapture::new(ObservabilityContentConfig {
        policy: ContentCapturePolicy::MetadataOnly,
        max_field_bytes: 0,
        max_observation_bytes: 0,
        detector_overlap_bytes: 0,
    });
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        content,
    );
    let trace = session.begin_trace(TraceSpec {
        name: "default-turn",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    });
    (session, trace)
}
