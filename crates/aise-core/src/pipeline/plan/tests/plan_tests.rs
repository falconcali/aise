use super::*;
use crate::core::tests::{snake_pack, store_with_default_pack};
use crate::core::{PlayerContribution, PlayerRoleContext, RoleActor, RoleEntry, TurnCancellation};
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct CountingProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Provider for CountingProvider {
    fn name(&self) -> &'static str {
        "counting"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletionResponse {
            content: r#"{"story_goal":"Continue the encounter.","facts_to_retrieve":[],"rumors_to_retrieve":[],"roles_for_generation":[],"character_think_requests":[]}"#.to_owned(),
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
}

async fn pipeline(calls: Arc<AtomicUsize>) -> PlanPipeline {
    let prompt = Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads");
    PlanPipeline::new(
        Arc::new(LlmGateway::new(Arc::new(CountingProvider { calls }), LlmConfig::default())),
        Arc::new(prompt),
        Arc::new(store_with_default_pack().await),
        PlanConfig::default(),
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
        name: "plan-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn plan_output(
    facts_to_retrieve: Vec<FactId>,
    rumors_to_retrieve: Vec<RumorId>,
    roles_for_generation: Vec<RoleId>,
    character_think_requests: Vec<CharacterThinkRequest>,
) -> PlanOutput {
    let pack = snake_pack();
    let role_id = RoleId::try_new("xu_xian").expect("valid role id");
    let role = &pack.roles[&role_id];
    PlanOutput {
        story_goal: BoundedText::default(),
        player_role: PlayerRoleContext {
            role_id,
            name: role.name.clone(),
            aliases: role.alias.clone(),
            backstory: role.backstory.clone(),
            profile: role.default_profile.clone(),
        },
        facts_to_retrieve,
        rumors_to_retrieve,
        roles_for_generation,
        character_think_requests,
    }
}

#[test]
fn empty_knowledge_selection_does_not_require_retrieval() {
    let output = plan_output(Vec::new(), Vec::new(), Vec::new(), Vec::new());

    assert!(!output.requires_retrieval());
}

#[test]
fn fact_selection_requires_retrieval() {
    let output = plan_output(
        vec![FactId::try_new("fact-1").expect("valid fact id")],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    assert!(output.requires_retrieval());
}

#[test]
fn rumor_selection_requires_retrieval() {
    let output = plan_output(
        Vec::new(),
        vec![RumorId::try_new("rumor-1").expect("valid rumor id")],
        Vec::new(),
        Vec::new(),
    );

    assert!(output.requires_retrieval());
}

#[test]
fn generation_role_selection_requires_retrieval() {
    let output = plan_output(
        Vec::new(),
        Vec::new(),
        vec![RoleId::try_new("bai_suzhen").expect("valid role id")],
        Vec::new(),
    );

    assert!(output.requires_retrieval());
}

#[test]
fn character_think_request_requires_retrieval_and_thinking() {
    let output = plan_output(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![CharacterThinkRequest {
            role_id: RoleId::try_new("bai_suzhen").expect("valid role id"),
            thinking_focus: BoundedText::try_new("Choose a response.", "test", 64).expect("valid focus"),
        }],
    );

    assert!(output.requires_retrieval());
    assert!(output.requires_character_thinking());
}

#[tokio::test]
async fn pipeline_calls_llm_once_and_returns_validated_output() {
    let calls = Arc::new(AtomicUsize::new(0));
    let pipeline = pipeline(Arc::clone(&calls)).await;
    let pack = snake_pack();
    let mut story_ctx = StoryContext::new();
    story_ctx.player = RoleId::try_new("xu_xian").expect("valid role id");
    story_ctx.roles.insert(
        story_ctx.player.clone(),
        RoleEntry {
            id: story_ctx.player.clone(),
            actor: RoleActor::Default,
            body: pack.roles[&story_ctx.player].clone(),
            memories: BTreeMap::new(),
        },
    );
    let player_contribution = PlayerContribution {
        raw: "I wave.".to_owned(),
        processed: "I wave.".to_owned(),
    };
    let directions = Vec::new();
    let constraints = Vec::new();
    let knowledge = ActivatedKnowledge::default();
    let input = PlanInput {
        player_contribution: &player_contribution,
        directions: &directions,
        constraints: &constraints,
        knowledge: &knowledge,
    };
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = test_trace();

    let output = pipeline
        .execute(input, &story_ctx, &control, trace.root())
        .await
        .expect("plan succeeds");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(output.story_goal.as_str(), "Continue the encounter.");
    assert_eq!(output.player_role.role_id, story_ctx.player);
    assert!(output.facts_to_retrieve.is_empty());
    assert!(output.rumors_to_retrieve.is_empty());
    assert!(output.roles_for_generation.is_empty());
    assert!(output.character_think_requests.is_empty());
}
