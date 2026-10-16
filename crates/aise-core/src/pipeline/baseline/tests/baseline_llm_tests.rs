use super::*;
use crate::core::TurnCancellation;
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmProvider, LlmReasoningEffort,
};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct CapturingProvider {
    request: Mutex<Option<LlmCompletionRequest>>,
}

#[async_trait]
impl LlmProvider for CapturingProvider {
    fn name(&self) -> &'static str {
        "capturing"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.request.lock().expect("request lock").replace(request);
        Ok(LlmCompletionResponse {
            content: "processed".to_owned(),
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
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
        name: "baseline-llm-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

#[tokio::test]
async fn baseline_uses_high_reasoning_effort() {
    let provider = Arc::new(CapturingProvider {
        request: Mutex::new(None),
    });
    let gateway = LlmGateway::new(provider.clone(), LlmConfig::default());
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    process_player_input(&gateway, "raw", vec![ChatMessage::user("input")], &control, trace.root())
        .await
        .expect("baseline completion");

    let request = provider.request.lock().expect("request lock").take().expect("captured request");
    assert_eq!(request.reasoning_effort, Some(LlmReasoningEffort::High));
}
