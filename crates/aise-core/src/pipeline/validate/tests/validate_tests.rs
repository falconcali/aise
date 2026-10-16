use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId, StoryContext,
    TurnCancellation,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmProvider,
};
use crate::pipeline::common::ValidationContext;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct ValidateProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl LlmProvider for ValidateProvider {
    fn name(&self) -> &'static str {
        "validate"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let config = ValidateConfig::default();
        let checks = config
            .checks
            .iter()
            .map(|check| serde_json::json!({"name": check.name, "passed": true, "issue": null}))
            .collect::<Vec<_>>();
        let scores = config
            .scores
            .iter()
            .map(|score| serde_json::json!({"name": score.name, "score": score.target_score, "issue": null}))
            .collect::<Vec<_>>();
        Ok(LlmCompletionResponse {
            content: serde_json::json!({"checks": checks, "scores": scores}).to_string(),
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
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
        name: "validate-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

#[tokio::test]
async fn execute_calls_gateway_once_and_returns_report() {
    let provider = Arc::new(ValidateProvider {
        calls: AtomicUsize::new(0),
    });
    let store = Arc::new(store_with_default_pack().await);
    let pipeline = ValidatePipeline::new(
        Arc::new(LlmGateway::new(provider.clone(), LlmConfig::default())),
        Arc::new(prompt()),
        store,
        ValidateConfig::default(),
    );
    let goal = text("Reach the gate.");
    let contribution = PlayerContribution {
        raw: "I walk.".to_owned(),
        processed: "The player walks.".to_owned(),
    };
    let knowledge = ActivatedKnowledge::default();
    let player_role = PlayerRoleContext {
        role_id: RoleId::try_new("player").expect("valid player role"),
        name: text("Player"),
        aliases: Vec::new(),
        backstory: text("Player backstory."),
        profile: CharacterProfile {
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
    };
    let input = ValidateInput {
        context: ValidationContext {
            story_goal: &goal,
            player_contribution: &contribution,
            directions: &[],
            constraints: &[],
            knowledge: &knowledge,
            retrieved_facts: &[],
            retrieved_rumors: &[],
            player_role: &player_role,
            roles: &[],
            character_decisions: &[],
        },
        proposal: "The player reaches the gate.",
        proposal_version: 2,
        repair_rounds_used: 0,
        repair_rounds_remaining: 2,
    };
    let story_ctx = StoryContext::new();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let output = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .expect("validate succeeds");

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(output.report.proposal_version, 2);
    assert!(output.report.all_checks_pass());
    assert!(output.report.all_scores_meet_target());
}
