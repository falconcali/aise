use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    ActivationRule, BoundedText, CharacterThinkRequest, FactBody, FactId, IdempotencyKey, NarrativeState, PlayerId,
    PlayerRoleContext, RetrievalHint, RoleActor, RumorBody, RumorId, StoryAssetSpec, StoryCommit, StoryInstanceSpec,
    StoryPack, TurnCancellation, TurnEvent,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, Provider,
};
use crate::pipeline::ValidateConfig;
use crate::pipeline::common::PipelineStage;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, TraceSpec,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const ACTIVATED_FACT_ID: &str = "activated_fact";
const RETRIEVED_FACT_ID: &str = "retrieved_fact";
const ACTIVATED_RUMOR_ID: &str = "activated_rumor";
const RETRIEVED_RUMOR_ID: &str = "retrieved_rumor";
const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct ScriptedProvider {
    calls: AtomicUsize,
    responses: Mutex<VecDeque<String>>,
    requests: Arc<Mutex<Vec<LlmCompletionRequest>>>,
    store: Arc<Store>,
    replacement_after_plan: Mutex<Option<StoryPack>>,
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn name(&self) -> &'static str {
        "scripted"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        lock(&self.requests).push(request);
        let content = lock(&self.responses).pop_front().ok_or(LlmError::InvalidResponse {
            reason: "missing scripted response",
        })?;
        let replacement = (call == 1).then(|| lock(&self.replacement_after_plan).take()).flatten();
        if let Some(pack) = replacement {
            replace_default_pack(self.store.as_ref(), pack).await?;
        }
        Ok(LlmCompletionResponse {
            content,
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
}

#[derive(Default)]
struct CapturingSink {
    stages: Arc<Mutex<Vec<String>>>,
}

impl TurnEventSink for CapturingSink {
    fn emit(&self, event: TurnEvent) {
        if let TurnEvent::TurnPipeline { stage } = event {
            lock(&self.stages).push(stage);
        }
    }
}

struct RuntimeRun {
    result: Result<StoryCommit, PipelineError>,
    requests: Vec<LlmCompletionRequest>,
    stages: Vec<String>,
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().expect("test mutex should not be poisoned")
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn activation_rule() -> ActivationRule {
    serde_json::from_value(json!({ "match": { "keys": ["activation-key"] } }))
        .expect("activation rule should deserialize")
}

fn fact_body(content: &str, hint: &str, activated: bool) -> FactBody {
    FactBody {
        content: text(content),
        activation: activated.then(activation_rule).unwrap_or_default(),
        retrieval: RetrievalHint::try_new(hint).expect("retrieval hint should be valid"),
    }
}

fn rumor_body(content: &str, hint: &str, activated: bool) -> RumorBody {
    RumorBody {
        content: text(content),
        activation: activated.then(activation_rule).unwrap_or_default(),
        retrieval: RetrievalHint::try_new(hint).expect("retrieval hint should be valid"),
    }
}

fn knowledge_pack() -> StoryPack {
    let mut pack = snake_pack();
    pack.seed_facts = BTreeMap::from([
        (
            FactId::try_new(ACTIVATED_FACT_ID).expect("fact id should be valid"),
            fact_body("Activated fact body.", "activated fact hint", true),
        ),
        (
            FactId::try_new(RETRIEVED_FACT_ID).expect("fact id should be valid"),
            fact_body("Retrieved fact body.", "retrieved fact hint", false),
        ),
    ]);
    pack.seed_rumors = BTreeMap::from([
        (
            RumorId::try_new(ACTIVATED_RUMOR_ID).expect("rumor id should be valid"),
            rumor_body("Activated rumor body.", "activated rumor hint", true),
        ),
        (
            RumorId::try_new(RETRIEVED_RUMOR_ID).expect("rumor id should be valid"),
            rumor_body("Retrieved rumor body.", "retrieved rumor hint", false),
        ),
    ]);
    pack
}

async fn replace_default_pack(store: &Store, pack: StoryPack) -> Result<(), LlmError> {
    let story_ctx = StoryContext::new();
    store
        .pack_store()
        .release(&story_ctx.story_id)
        .await
        .map_err(|source| LlmError::Transport {
            source: Box::new(source),
        })?;
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryAssetSpec {
                pack_ref: story_ctx.pack_ref,
                pack,
                cards: Vec::new(),
            },
        )
        .await
        .map_err(|source| LlmError::Transport {
            source: Box::new(source),
        })
}

fn bundled_prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt should load")
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
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "runtime-retrieval-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

async fn run_runtime(plan_response: String, remove_selected_fact_after_plan: bool) -> RuntimeRun {
    run_runtime_with_think_responses(plan_response, remove_selected_fact_after_plan, None).await
}

async fn run_runtime_with_think_responses(
    plan_response: String,
    remove_selected_fact_after_plan: bool,
    think_responses: Option<Vec<String>>,
) -> RuntimeRun {
    run_runtime_scenario(
        plan_response,
        remove_selected_fact_after_plan,
        think_responses,
        "Generated story.".to_owned(),
        vec![validation_response()],
        PipelineConfig::default(),
    )
    .await
}

async fn run_runtime_scenario(
    plan_response: String,
    remove_selected_fact_after_plan: bool,
    think_responses: Option<Vec<String>>,
    generated_proposal: String,
    validation_responses: Vec<String>,
    pipeline_config: PipelineConfig,
) -> RuntimeRun {
    let store = Arc::new(Store::default());
    let mut story_ctx = StoryContext::new();
    let pack = knowledge_pack();
    story_ctx.player = RoleId::try_new("xu_xian").expect("player role id should be valid");
    story_ctx.cast = pack.roles.keys().map(|role_id| (role_id.clone(), RoleActor::Default)).collect();
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryAssetSpec {
                pack_ref: story_ctx.pack_ref.clone(),
                pack: pack.clone(),
                cards: Vec::new(),
            },
        )
        .await
        .expect("pack should be retained");
    store
        .story_store()
        .create(StoryInstanceSpec {
            story_id: story_ctx.story_id.clone(),
            pack_ref: story_ctx.pack_ref.clone(),
            cast: story_ctx.cast.clone(),
            player_id: PlayerId::try_new("player").expect("player id should be valid"),
            player_role: story_ctx.player.clone(),
            narrative: NarrativeState::default(),
        })
        .await
        .expect("story should be created");
    let replacement_after_plan = remove_selected_fact_after_plan.then(|| {
        let mut replacement = pack;
        replacement
            .seed_facts
            .remove(&FactId::try_new(RETRIEVED_FACT_ID).expect("fact id should be valid"));
        replacement
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let think_request_count = serde_json::from_str::<serde_json::Value>(&plan_response)
        .ok()
        .and_then(|value| value["character_think_requests"].as_array().map(Vec::len))
        .unwrap_or_default();
    let mut responses = VecDeque::from(["activation-key".to_owned(), plan_response]);
    responses.extend(think_responses.unwrap_or_else(|| {
        (0..think_request_count)
            .map(|index| json!({"decision": format!("decision-{index}"), "suggested_utterance": null}).to_string())
            .collect()
    }));
    responses.push_back(generated_proposal);
    responses.extend(validation_responses);
    let provider = ScriptedProvider {
        calls: AtomicUsize::new(0),
        responses: Mutex::new(responses),
        requests: Arc::clone(&requests),
        store: Arc::clone(&store),
        replacement_after_plan: Mutex::new(replacement_after_plan),
    };
    let runtime = Runtime::new(
        Arc::new(LlmGateway::new(Arc::new(provider), LlmConfig::default())),
        Arc::new(bundled_prompt()),
        Arc::clone(&store),
        pipeline_config,
    )
    .expect("runtime config should be valid");
    let sink = CapturingSink::default();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = test_trace();
    let result = runtime
        .run_turn(
            &story_ctx,
            TurnRequest {
                story_id: story_ctx.story_id.clone(),
                idempotency_key: IdempotencyKey::try_new("turn-1").expect("idempotency key should be valid"),
                player_input: "raw input".to_owned(),
            },
            &control,
            &sink,
            &trace,
        )
        .await;
    let requests = lock(&requests).clone();
    let stages = lock(&sink.stages).clone();

    RuntimeRun {
        result,
        requests,
        stages,
    }
}

fn plan_response(facts: &[&str], rumors: &[&str]) -> String {
    plan_response_with_roles(facts, rumors, &[], &[])
}

fn plan_response_with_roles(facts: &[&str], rumors: &[&str], roles: &[&str], think_roles: &[&str]) -> String {
    json!({
        "story_goal": "Continue the encounter.",
        "facts_to_retrieve": facts,
        "rumors_to_retrieve": rumors,
        "roles_for_generation": roles,
        "character_think_requests": think_roles
            .iter()
            .map(|role_id| json!({
                "role_id": role_id,
                "thinking_focus": "Choose a response."
            }))
            .collect::<Vec<_>>()
    })
    .to_string()
}

fn validation_response() -> String {
    validation_response_with(true, None)
}

fn validation_response_with(checks_pass: bool, score_value: Option<f64>) -> String {
    let config = ValidateConfig::default();
    let checks = config
        .checks
        .iter()
        .map(|check| {
            json!({
                "name": check.name,
                "passed": checks_pass,
                "issue": (!checks_pass).then_some("Fix the hard-gate conflict.")
            })
        })
        .collect::<Vec<_>>();
    let scores = config
        .scores
        .iter()
        .map(|score| {
            let value = score_value.unwrap_or(score.target_score);
            json!({
                "name": score.name,
                "score": value,
                "issue": (value < score.target_score).then_some("Improve this score.")
            })
        })
        .collect::<Vec<_>>();
    json!({"checks": checks, "scores": scores}).to_string()
}

fn request_text(request: &LlmCompletionRequest) -> String {
    request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\r\n", "\n")
}

fn stage_count(stages: &[String], stage: &str) -> usize {
    stages.iter().filter(|candidate| candidate.as_str() == stage).count()
}

fn assert_plan_prompt_contract(requests: &[LlmCompletionRequest]) {
    assert_eq!(
        requests
            .iter()
            .filter(|request| request_text(request).contains("\"facts_to_retrieve\""))
            .count(),
        1
    );
    let prompt = request_text(&requests[1]);
    assert!(prompt.contains("## Known Facts\n\n- Activated fact body."));
    assert!(prompt.contains("## Rumors\n\n- Activated rumor body."));
    assert!(prompt.contains(&format!("- id: \"{RETRIEVED_FACT_ID}\"")));
    assert!(prompt.contains(&format!("- id: \"{RETRIEVED_RUMOR_ID}\"")));
    assert!(!prompt.contains(&format!("- id: \"{ACTIVATED_FACT_ID}\"")));
    assert!(!prompt.contains(&format!("- id: \"{ACTIVATED_RUMOR_ID}\"")));
    assert!(prompt.contains("## Player Character"));
    assert!(prompt.contains("role_id: \"xu_xian\""));
    assert!(prompt.contains("appearance: \"清瘦温和的年轻书生"));
    assert!(!prompt.contains("profile.speaking_style"));
    let ai_catalog = prompt.split("## AI Role Catalog").nth(1).expect("AI role catalog");
    assert!(!ai_catalog.contains("role_id: \"xu_xian\""));
}

#[test]
fn runtime_new_rejects_invalid_pipeline_config() {
    let store = Arc::new(Store::default());
    let provider = ScriptedProvider {
        calls: AtomicUsize::new(0),
        responses: Mutex::new(VecDeque::new()),
        requests: Arc::new(Mutex::new(Vec::new())),
        store: Arc::clone(&store),
        replacement_after_plan: Mutex::new(None),
    };
    let mut config = PipelineConfig::default();
    config.validate.checks.clear();
    config.validate.scores.clear();

    let result = Runtime::new(
        Arc::new(LlmGateway::new(Arc::new(provider), LlmConfig::default())),
        Arc::new(bundled_prompt()),
        store,
        config,
    );

    assert!(matches!(
        result,
        Err(PipelineError::InvalidConfig {
            stage: PipelineStage::Validate,
            code: "validate_criteria_empty",
            ..
        })
    ));
}

#[test]
fn effective_roles_preserve_generation_order_and_append_unique_think_targets() {
    let pack = snake_pack();
    let player_role_id = RoleId::try_new("xu_xian").expect("valid role id");
    let player_role = &pack.roles[&player_role_id];
    let plan_output = PlanOutput {
        story_goal: text("Continue the encounter."),
        player_role: PlayerRoleContext {
            role_id: player_role_id,
            name: player_role.name.clone(),
            aliases: player_role.alias.clone(),
            backstory: player_role.backstory.clone(),
            profile: player_role.default_profile.clone(),
        },
        facts_to_retrieve: Vec::new(),
        rumors_to_retrieve: Vec::new(),
        roles_for_generation: vec![
            RoleId::try_new("bai_suzhen").expect("valid role id"),
            RoleId::try_new("xiaoqing").expect("valid role id"),
        ],
        character_think_requests: vec![
            CharacterThinkRequest {
                role_id: RoleId::try_new("bai_suzhen").expect("valid role id"),
                thinking_focus: text("Choose a response."),
            },
            CharacterThinkRequest {
                role_id: RoleId::try_new("fahai").expect("valid role id"),
                thinking_focus: text("Choose a response."),
            },
        ],
    };

    let role_ids = effective_role_ids(&plan_output);

    assert_eq!(
        role_ids.iter().map(RoleId::as_str).collect::<Vec<_>>(),
        vec!["bai_suzhen", "xiaoqing", "fahai"]
    );
}

#[tokio::test]
async fn runtime_skips_role_retrieval_and_think_when_plan_selects_no_roles() {
    let run = run_runtime(plan_response(&[], &[]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 0);
    assert_eq!(stage_count(&run.stages, "think"), 0);
    assert_eq!(run.requests.len(), 4);
    assert_plan_prompt_contract(&run.requests);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains("# Player Character"));
    assert!(generate_prompt.contains("role_id: \"xu_xian\""));
    assert!(generate_prompt.contains("profile.speaking_style: \"温和克制"));
    assert!(!generate_prompt.contains("xu_memory_west_lake_meeting"));
    assert!(!generate_prompt.contains("清明时节西湖骤雨"));
    assert!(generate_prompt.contains("# Known Facts\n\n- Activated fact body."));
    assert!(generate_prompt.contains("# Rumors\n\n- Activated rumor body."));
    assert!(!generate_prompt.contains("Retrieved fact body."));
    assert!(!generate_prompt.contains("Retrieved rumor body."));
}

#[tokio::test]
async fn runtime_runs_retrieval_for_fact_selection() {
    let run = run_runtime(plan_response(&[RETRIEVED_FACT_ID], &[]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(run.requests.len(), 4);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains("# Known Facts\n\n- Activated fact body.\n- Retrieved fact body."));
    assert!(generate_prompt.contains("# Rumors\n\n- Activated rumor body."));
    assert!(!generate_prompt.contains("Retrieved rumor body."));
}

#[tokio::test]
async fn runtime_runs_retrieval_for_rumor_selection() {
    let run = run_runtime(plan_response(&[], &[RETRIEVED_RUMOR_ID]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(run.requests.len(), 4);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains("# Known Facts\n\n- Activated fact body."));
    assert!(generate_prompt.contains("# Rumors\n\n- Activated rumor body.\n- Retrieved rumor body."));
    assert!(!generate_prompt.contains("Retrieved fact body."));
}

#[tokio::test]
async fn runtime_delivers_retrieved_knowledge_to_generate() {
    let run = run_runtime(plan_response(&[RETRIEVED_FACT_ID], &[RETRIEVED_RUMOR_ID]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(run.requests.len(), 4);
    assert_plan_prompt_contract(&run.requests);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains(
        "# Known Facts\n\n- Activated fact body.\n- Retrieved fact body.\n\n# Rumors\n\n- Activated rumor body.\n- Retrieved rumor body."
    ));
}

#[tokio::test]
async fn runtime_retrieves_generation_only_role_without_think() {
    let run = run_runtime(plan_response_with_roles(&[], &[], &["bai_suzhen"], &[]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(stage_count(&run.stages, "think"), 0);
    assert_eq!(run.requests.len(), 4);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains("# Player Character"));
    assert!(generate_prompt.contains("# Relevant AI Roles"));
    assert!(generate_prompt.contains("role_id: \"bai_suzhen\""));
    assert!(!generate_prompt.contains("# Character Decisions"));
}

#[tokio::test]
async fn runtime_runs_one_character_think_and_delivers_decision_to_generate() {
    let run = run_runtime(plan_response_with_roles(&[], &[], &[], &["bai_suzhen"]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(stage_count(&run.stages, "think"), 1);
    assert_eq!(run.requests.len(), 5);
    let generate_prompt = request_text(&run.requests[3]);
    assert!(generate_prompt.contains("# Player Character"));
    assert!(generate_prompt.contains("# Relevant AI Roles"));
    assert!(generate_prompt.contains("# Character Decisions"));
    assert!(generate_prompt.contains("role_id: \"bai_suzhen\""));
    assert!(generate_prompt.contains("decision: \"decision-0\""));
}

#[tokio::test]
async fn runtime_delivers_relevant_rumors_but_not_narrative_direction_to_character_think() {
    let run = run_runtime(
        plan_response_with_roles(&[], &[RETRIEVED_RUMOR_ID], &[], &["bai_suzhen"]),
        false,
    )
    .await;

    assert!(run.result.is_ok());
    assert_eq!(run.requests.len(), 5);
    let think_prompt = request_text(&run.requests[2]);
    assert!(think_prompt.contains("## Rumors\n\n- \"Activated rumor body.\"\n- \"Retrieved rumor body.\""));
    assert!(!think_prompt.contains(ACTIVATED_RUMOR_ID));
    assert!(!think_prompt.contains(RETRIEVED_RUMOR_ID));
    assert!(!think_prompt.contains("activated rumor hint"));
    assert!(!think_prompt.contains("retrieved rumor hint"));
    assert!(!think_prompt.contains("Narrative Direction"));
}

#[tokio::test]
async fn runtime_stable_union_resolves_cross_list_role_once() {
    let run = run_runtime(
        plan_response_with_roles(
            &[RETRIEVED_FACT_ID],
            &[RETRIEVED_RUMOR_ID],
            &["bai_suzhen"],
            &["bai_suzhen", "xiaoqing"],
        ),
        false,
    )
    .await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(stage_count(&run.stages, "think"), 1);
    assert_eq!(run.requests.len(), 6);
    let generate_prompt = request_text(&run.requests[4]);
    let relevant_roles = generate_prompt
        .split("# Relevant AI Roles")
        .nth(1)
        .expect("roles section")
        .split("# Character Decisions")
        .next()
        .expect("roles section body");
    assert_eq!(relevant_roles.matches("role_id: \"bai_suzhen\"").count(), 1);
    assert_eq!(relevant_roles.matches("role_id: \"xiaoqing\"").count(), 1);
}

#[tokio::test]
async fn runtime_preserves_multi_role_think_request_order() {
    let run = run_runtime(plan_response_with_roles(&[], &[], &[], &["xiaoqing", "bai_suzhen"]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "think"), 1);
    assert_eq!(run.requests.len(), 6);
    let generate_prompt = request_text(&run.requests[4]);
    let decisions = generate_prompt
        .split("# Character Decisions")
        .nth(1)
        .expect("decisions section");
    let xiaoqing = decisions.find("role_id: \"xiaoqing\"").expect("xiaoqing decision");
    let bai_suzhen = decisions.find("role_id: \"bai_suzhen\"").expect("bai suzhen decision");
    let first_decision = decisions.find("decision: \"decision-0\"").expect("first decision");
    let second_decision = decisions.find("decision: \"decision-1\"").expect("second decision");

    assert!(xiaoqing < bai_suzhen);
    assert!(first_decision < second_decision);
}

#[tokio::test]
async fn runtime_fails_before_generate_when_one_character_think_fails() {
    let valid = json!({"decision": "valid", "suggested_utterance": null}).to_string();
    let run = run_runtime_with_think_responses(
        plan_response_with_roles(&[], &[], &[], &["bai_suzhen", "xiaoqing"]),
        false,
        Some(vec![valid, "not json".to_owned()]),
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::InvalidOutput {
            stage: PipelineStage::Think,
            code: "think_output_invalid_json",
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "think"), 1);
    assert_eq!(stage_count(&run.stages, "generate"), 0);
    assert_eq!(run.requests.len(), 4);
}

#[tokio::test]
async fn runtime_never_places_character_decisions_in_commit() {
    let run = run_runtime(plan_response_with_roles(&[], &[], &[], &["bai_suzhen"]), false).await;

    let commit = run.result.expect("turn succeeds");
    let serialized = serde_json::to_string(&commit).expect("commit serializes");

    assert!(!serialized.contains("character_decisions"));
    assert!(!serialized.contains("decision-0"));
    assert_eq!(commit.turn.turn_segment.text(), "Generated story.");
}

#[tokio::test]
async fn runtime_rejects_plain_text_plan_before_retrieval_and_generate() {
    let run = run_runtime("Continue the encounter.".to_owned(), false).await;

    assert!(matches!(
        run.result,
        Err(PipelineError::InvalidOutput {
            stage: PipelineStage::Plan,
            code: "plan_output_invalid_json",
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "retrieval"), 0);
    assert_eq!(stage_count(&run.stages, "generate"), 0);
    assert_eq!(run.requests.len(), 2);
}

#[tokio::test]
async fn runtime_stops_before_generate_when_selected_fact_disappears() {
    let run = run_runtime(plan_response(&[RETRIEVED_FACT_ID], &[]), true).await;

    assert!(matches!(
        run.result,
        Err(PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_fact_missing",
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "retrieval"), 1);
    assert_eq!(stage_count(&run.stages, "generate"), 0);
    assert_eq!(run.requests.len(), 2);
}

#[tokio::test]
async fn runtime_passes_initial_proposal_without_repair() {
    let run = run_runtime(plan_response(&[], &[]), false).await;
    let commit = run.result.expect("turn succeeds");

    assert_eq!(stage_count(&run.stages, "validate"), 1);
    assert_eq!(stage_count(&run.stages, "repair"), 0);
    assert_eq!(commit.turn.turn_segment.text(), "Generated story.");
    assert_eq!(commit.turn.turn_status, TurnStatus::Pass);
    assert_eq!(commit.turn.turn_evaluation.proposal_version, 1);
    assert_eq!(commit.turn.turn_evaluation.repair_rounds, 0);
}

#[tokio::test]
async fn runtime_repairs_then_fully_revalidates_new_version() {
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        "Generated story.".to_owned(),
        vec![
            validation_response_with(false, Some(7.0)),
            "Repaired story.".to_owned(),
            validation_response(),
        ],
        PipelineConfig::default(),
    )
    .await;
    let commit = run.result.expect("turn succeeds");
    let validate_requests = run
        .requests
        .iter()
        .filter(|request| request_text(request).contains("# [UNTRUSTED CONTENT] Check Criteria"))
        .collect::<Vec<_>>();

    assert_eq!(stage_count(&run.stages, "validate"), 2);
    assert_eq!(stage_count(&run.stages, "repair"), 1);
    assert_eq!(validate_requests.len(), 2);
    for request in validate_requests {
        let prompt = request_text(request);
        for check in &ValidateConfig::default().checks {
            assert!(prompt.contains(&check.name));
        }
        for score in &ValidateConfig::default().scores {
            assert!(prompt.contains(&score.name));
        }
    }
    assert_eq!(commit.turn.turn_segment.text(), "Repaired story.");
    assert_eq!(commit.turn.turn_evaluation.proposal_version, 2);
    assert_eq!(commit.turn.turn_evaluation.repair_rounds, 1);
}

#[tokio::test]
async fn runtime_zero_budget_validates_once_and_never_repairs() {
    let mut config = PipelineConfig::default();
    config.validation_loop.max_repair_rounds = 0;
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        "Generated story.".to_owned(),
        vec![validation_response_with(true, Some(7.0))],
        config,
    )
    .await;
    let commit = run.result.expect("turn succeeds");

    assert_eq!(stage_count(&run.stages, "validate"), 1);
    assert_eq!(stage_count(&run.stages, "repair"), 0);
    assert_eq!(commit.turn.turn_status, TurnStatus::Choice);
    assert_eq!(commit.turn.turn_evaluation.repair_rounds, 0);
}

#[tokio::test]
async fn runtime_hard_gate_exhaustion_skips_extract_summary_and_commit() {
    let mut config = PipelineConfig::default();
    config.validation_loop.max_repair_rounds = 1;
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        "Generated story.".to_owned(),
        vec![
            validation_response_with(false, None),
            "Still broken.".to_owned(),
            validation_response_with(false, None),
        ],
        config,
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::ValidationFailed {
            code: "validation_hard_gate_exhausted",
            proposal_version: 2,
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "validate"), 2);
    assert_eq!(stage_count(&run.stages, "repair"), 1);
    assert_eq!(stage_count(&run.stages, "extract"), 0);
    assert_eq!(stage_count(&run.stages, "summary"), 0);
    assert_eq!(stage_count(&run.stages, "commit"), 0);
}

#[tokio::test]
async fn runtime_validate_contract_error_does_not_consume_repair_budget() {
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        "Generated story.".to_owned(),
        vec!["not json".to_owned()],
        PipelineConfig::default(),
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::InvalidOutput {
            stage: PipelineStage::Validate,
            code: "validate_output_invalid_json",
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "validate"), 1);
    assert_eq!(stage_count(&run.stages, "repair"), 0);
    assert_eq!(stage_count(&run.stages, "commit"), 0);
}

#[tokio::test]
async fn runtime_history_is_bounded_by_repair_budget_plus_one() {
    let mut config = PipelineConfig::default();
    config.validation_loop.max_repair_rounds = 2;
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        "Generated story.".to_owned(),
        vec![
            validation_response_with(false, None),
            "Repair one.".to_owned(),
            validation_response_with(false, None),
            "Repair two.".to_owned(),
            validation_response_with(false, None),
        ],
        config,
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::ValidationFailed {
            code: "validation_hard_gate_exhausted",
            proposal_version: 3,
            ..
        })
    ));
    assert_eq!(stage_count(&run.stages, "validate"), 3);
    assert_eq!(stage_count(&run.stages, "repair"), 2);
}

#[tokio::test]
async fn runtime_rejects_invalid_generated_proposal_before_validate() {
    let run = run_runtime_scenario(
        plan_response(&[], &[]),
        false,
        None,
        " \n\t".to_owned(),
        Vec::new(),
        PipelineConfig::default(),
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Generate
        })
    ));
    assert_eq!(stage_count(&run.stages, "validate"), 0);
    assert_eq!(stage_count(&run.stages, "repair"), 0);
    assert_eq!(stage_count(&run.stages, "commit"), 0);
}
