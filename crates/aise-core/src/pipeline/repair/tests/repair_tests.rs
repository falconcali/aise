use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId, StoryContext,
    TurnCancellation,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmProvider,
};
use crate::pipeline::ValidateConfig;
use crate::pipeline::common::{ValidationContext, ValidationIssue};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct RepairProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl LlmProvider for RepairProvider {
    fn name(&self) -> &'static str {
        "repair"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletionResponse {
            content: "Repaired story.".to_owned(),
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
        name: "repair-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

#[tokio::test]
async fn repair_performs_one_gateway_call() {
    let provider = Arc::new(RepairProvider {
        calls: AtomicUsize::new(0),
    });
    let store = Arc::new(store_with_default_pack().await);
    let quality_policy = Arc::new(ValidateConfig::default().quality_policy());
    let pipeline = RepairPipeline::new(
        Arc::new(LlmGateway::new(provider.clone(), LlmConfig::default())),
        Arc::new(prompt()),
        store,
        RepairConfig::default(),
        quality_policy,
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
    let issues = vec![ValidationIssue::Check {
        name: "internal_logic".to_owned(),
        message: text("Fix the contradiction."),
    }];
    let input = RepairInput {
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
        proposal: "Broken story.",
        proposal_version: 1,
        issues: &issues,
        repair_round: 1,
        repair_rounds_remaining: 1,
    };
    let story_ctx = StoryContext::new();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let output = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .expect("repair succeeds");

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(output.proposal, "Repaired story.");
}

#[tokio::test]
async fn repair_uses_runtime_shared_quality_policy() {
    let provider = Arc::new(RepairProvider {
        calls: AtomicUsize::new(0),
    });
    let store = Arc::new(store_with_default_pack().await);
    let quality_policy = Arc::new(ValidateConfig::default().quality_policy());
    let pipeline = RepairPipeline::new(
        Arc::new(LlmGateway::new(provider, LlmConfig::default())),
        Arc::new(prompt()),
        store,
        RepairConfig::default(),
        Arc::clone(&quality_policy),
    );

    assert!(Arc::ptr_eq(pipeline.quality_policy(), &quality_policy));
}
