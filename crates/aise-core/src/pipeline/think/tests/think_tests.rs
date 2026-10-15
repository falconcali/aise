use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    BoundedText, CharacterProfile, CharacterThinkRequest, PlayerContribution, ResolvedRole, RetrievalHint, RoleId,
    StoryContext, TurnCancellation,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, Provider,
};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct ControlledProvider {
    active: AtomicUsize,
    calls: AtomicUsize,
    maximum_active: AtomicUsize,
    fail_first: AtomicBool,
}

#[async_trait]
impl Provider for ControlledProvider {
    fn name(&self) -> &'static str {
        "controlled"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        let request_text = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let role_id = (0..8)
            .map(|index| format!("role-{index}"))
            .find(|candidate| request_text.contains(candidate))
            .ok_or(LlmError::InvalidResponse {
                reason: "target role missing",
            })?;
        if role_id == "role-0" {
            tokio::time::sleep(Duration::from_millis(40)).await;
        } else {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        let content = if call == 0 && self.fail_first.load(Ordering::SeqCst) {
            r#"{"decision":""}"#.to_owned()
        } else {
            serde_json::json!({
                "decision": format!("decision-{role_id}"),
                "suggested_utterance": null
            })
            .to_string()
        };
        Ok(LlmCompletionResponse {
            content,
            finish_reason: LlmCompletionFinishReason::Stop,
        })
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn role(index: usize) -> ResolvedRole {
    ResolvedRole {
        role_id: RoleId::try_new(format!("role-{index}")).expect("valid role id"),
        name: text(&format!("Role {index}")),
        aliases: Vec::new(),
        backstory: text(&format!("Backstory {index}")),
        retrieval_hint: RetrievalHint::try_new(format!("Hint {index}")).expect("valid hint"),
        profile: CharacterProfile {
            appearance: None,
            personality: Some(text(&format!("Personality {index}"))),
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        seed_memories: BTreeMap::new(),
    }
}

fn request(role: &ResolvedRole) -> CharacterThinkRequest {
    CharacterThinkRequest {
        role_id: role.role_id.clone(),
        thinking_focus: text("Choose an immediate response."),
    }
}

fn prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt should load")
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
        name: "think-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

async fn pipeline(provider: Arc<ControlledProvider>, max_concurrency: usize) -> ThinkPipeline {
    let store = Arc::new(store_with_default_pack().await);
    let config = ThinkConfig {
        max_concurrency: std::num::NonZeroUsize::new(max_concurrency).expect("nonzero concurrency"),
        ..ThinkConfig::default()
    };
    ThinkPipeline::new(
        Arc::new(LlmGateway::new(provider, LlmConfig::default())),
        Arc::new(prompt()),
        store,
        config,
    )
}

#[tokio::test]
async fn decisions_preserve_request_order_with_bounded_concurrency() {
    let provider = Arc::new(ControlledProvider {
        active: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        maximum_active: AtomicUsize::new(0),
        fail_first: AtomicBool::new(false),
    });
    let pipeline = pipeline(Arc::clone(&provider), 2).await;
    let roles = (0..4).map(role).collect::<Vec<_>>();
    let requests = roles.iter().map(request).collect::<Vec<_>>();
    let player_contribution = PlayerContribution {
        raw: "raw".to_owned(),
        processed: "processed".to_owned(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        directions: &[],
    };
    let story_ctx = StoryContext::new();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let output = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .expect("think should succeed");

    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    assert_eq!(provider.maximum_active.load(Ordering::SeqCst), 2);
    assert_eq!(
        output
            .decisions
            .iter()
            .map(|decision| decision.role_id.as_str())
            .collect::<Vec<_>>(),
        vec!["role-0", "role-1", "role-2", "role-3"]
    );
}

#[tokio::test]
async fn any_invalid_role_output_fails_the_whole_stage() {
    let provider = Arc::new(ControlledProvider {
        active: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        maximum_active: AtomicUsize::new(0),
        fail_first: AtomicBool::new(true),
    });
    let pipeline = pipeline(Arc::clone(&provider), 2).await;
    let roles = (0..3).map(role).collect::<Vec<_>>();
    let requests = roles.iter().map(request).collect::<Vec<_>>();
    let player_contribution = PlayerContribution {
        raw: "raw".to_owned(),
        processed: "processed".to_owned(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        directions: &[],
    };
    let story_ctx = StoryContext::new();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let error = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .err()
        .expect("invalid decision should fail");

    assert!(matches!(
        error,
        PipelineError::InvalidOutput {
            code: "think_decision_empty",
            ..
        }
    ));
}

#[test]
fn duplicate_request_role_is_rejected() {
    let roles = [role(0)];
    let requests = [request(&roles[0]), request(&roles[0])];
    let player_contribution = PlayerContribution {
        raw: String::new(),
        processed: String::new(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        directions: &[],
    };

    let error = validate_input(&input, &RoleId::try_new("player").expect("valid player")).expect_err("duplicate fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            code: "think_duplicate_request_role_id",
            ..
        }
    ));
}

#[test]
fn player_role_request_is_rejected() {
    let roles = [role(0)];
    let requests = [request(&roles[0])];
    let player_contribution = PlayerContribution {
        raw: String::new(),
        processed: String::new(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        directions: &[],
    };

    let error = validate_input(&input, &roles[0].role_id).expect_err("player target fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            code: "think_player_role_forbidden",
            ..
        }
    ));
}

#[test]
fn missing_or_duplicate_resolved_role_is_rejected() {
    let roles = [role(0)];
    let requests = [request(&roles[0])];
    let player_contribution = PlayerContribution {
        raw: String::new(),
        processed: String::new(),
    };
    let missing_input = ThinkInput {
        requests: &requests,
        resolved_roles: &[],
        player_contribution: &player_contribution,
        directions: &[],
    };
    let duplicate_roles = [roles[0].clone(), roles[0].clone()];
    let duplicate_input = ThinkInput {
        requests: &requests,
        resolved_roles: &duplicate_roles,
        player_contribution: &player_contribution,
        directions: &[],
    };
    let player = RoleId::try_new("player").expect("valid player");

    let missing = validate_input(&missing_input, &player).expect_err("missing role fails");
    let duplicate = validate_input(&duplicate_input, &player).expect_err("duplicate role fails");

    assert!(matches!(
        missing,
        PipelineError::InconsistentData {
            code: "think_resolved_role_missing",
            ..
        }
    ));
    assert!(matches!(
        duplicate,
        PipelineError::InconsistentData {
            code: "think_resolved_role_missing",
            ..
        }
    ));
}
