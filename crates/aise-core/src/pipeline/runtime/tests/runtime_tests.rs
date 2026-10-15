use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    ActivationRule, FactBody, FactId, IdempotencyKey, NarrativeState, PlayerId, RetrievalHint, RumorBody, RumorId,
    StoryAssetSpec, StoryCommit, StoryInstanceSpec, StoryPack, TurnCancellation, TurnEvent,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmProvider,
};
use crate::pipeline::common::PipelineStage;
use crate::pipeline::validate::ValidateScoreResult;
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
impl LlmProvider for ScriptedProvider {
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
    let store = Arc::new(Store::default());
    let story_ctx = StoryContext::new();
    let pack = knowledge_pack();
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
            cast: BTreeMap::new(),
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
    let provider = ScriptedProvider {
        calls: AtomicUsize::new(0),
        responses: Mutex::new(VecDeque::from([
            "activation-key".to_owned(),
            plan_response,
            "Generated story.".to_owned(),
        ])),
        requests: Arc::clone(&requests),
        store: Arc::clone(&store),
        replacement_after_plan: Mutex::new(replacement_after_plan),
    };
    let runtime = Runtime::new(
        Arc::new(LlmGateway::new(Arc::new(provider), LlmConfig::default())),
        Arc::new(bundled_prompt()),
        Arc::clone(&store),
        PipelineConfig::default(),
    );
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
    json!({
        "story_goal": "Continue the encounter.",
        "facts_to_retrieve": facts,
        "rumors_to_retrieve": rumors,
        "roles_for_generation": [],
        "character_think_requests": []
    })
    .to_string()
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
}

fn validation(decision: ValidationDecision, scores: &[f32]) -> ValidateOutput {
    ValidateOutput {
        scores: scores
            .iter()
            .map(|score| ValidateScoreResult {
                name: "score".to_owned(),
                description: "score".to_owned(),
                score: *score,
                target_score: 10.0,
                issue: String::new(),
            })
            .collect(),
        decision,
    }
}

#[test]
fn accepted_final_validation_selects_last_proposal() {
    let validations = vec![
        validation(ValidationDecision::Repair, &[9.0]),
        validation(ValidationDecision::Accept, &[1.0]),
    ];

    assert_eq!(select_final_index(&validations), 1);
}

#[test]
fn rejected_final_validation_selects_best_scored_proposal() {
    let validations = vec![
        validation(ValidationDecision::Repair, &[3.0, 4.0]),
        validation(ValidationDecision::Repair, &[9.0, 4.0]),
        validation(ValidationDecision::Reject, &[1.0, 1.0]),
    ];

    assert_eq!(select_final_index(&validations), 1);
}

#[test]
fn proposal_version_counts_proposals() {
    let proposals = vec!["a".to_owned(), "b".to_owned()];

    assert_eq!(proposal_version(&proposals), 2);
}

#[tokio::test]
async fn runtime_skips_retrieval_for_empty_plan_selection() {
    let run = run_runtime(plan_response(&[], &[]), false).await;

    assert!(run.result.is_ok());
    assert_eq!(stage_count(&run.stages, "retrieval"), 0);
    assert_eq!(run.requests.len(), 3);
    assert_plan_prompt_contract(&run.requests);
    let generate_prompt = request_text(&run.requests[2]);
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
    assert_eq!(run.requests.len(), 3);
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
    assert_eq!(run.requests.len(), 3);
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
    assert_eq!(run.requests.len(), 3);
    assert_plan_prompt_contract(&run.requests);
    let generate_prompt = request_text(&run.requests[2]);
    assert!(generate_prompt.contains(
        "# Known Facts\n\n- Activated fact body.\n- Retrieved fact body.\n\n# Rumors\n\n- Activated rumor body.\n- Retrieved rumor body."
    ));
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
