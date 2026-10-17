use super::*;
use crate::core::tests::{store_with_default_pack, story_context};
use crate::core::{
    ActivatedKnowledge, BoundedText, PlayerContribution, TurnCancellation, TurnControl, TurnEvent, TurnEventSink,
};
use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmGateway, Provider,
};
use crate::pipeline::common::{
    PipelineRunner, ValidationCheckResult, ValidationContext, ValidationReport, ValidationScoreResult,
};
use crate::pipeline::{PipelineConfig, ValidateConfig};
use crate::prompt::{Prompt, PromptConfig};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct ScriptedValidationProvider {
    responses: Mutex<VecDeque<String>>,
    requests: Arc<Mutex<Vec<LlmCompletionRequest>>>,
}

#[async_trait]
impl Provider for ScriptedValidationProvider {
    fn name(&self) -> &'static str {
        "scripted-validation"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        lock(&self.requests).push(request);
        let content = lock(&self.responses).pop_front().ok_or(LlmError::InvalidResponse {
            reason: "missing scripted response",
        })?;
        Ok(LlmCompletionResponse {
            content,
            finish_reason: LlmCompletionFinishReason::Stop,
            usage: None,
        })
    }
}

#[derive(Default)]
struct CapturingSink {
    stages: Mutex<Vec<String>>,
}

impl TurnEventSink for CapturingSink {
    fn emit(&self, event: TurnEvent) {
        if let TurnEvent::TurnPipeline { stage } = event {
            lock(&self.stages).push(stage);
        }
    }
}

struct ValidationLoopRun {
    result: Result<ValidationLoopResult, PipelineError>,
    requests: Vec<LlmCompletionRequest>,
    stages: Vec<String>,
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().expect("test mutex should not be poisoned")
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
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "runtime-validation-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
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

fn validation_response(checks_pass: bool, score_value: Option<f64>, check_issue: &str, score_issue: &str) -> String {
    let config = ValidateConfig::default();
    let checks = config
        .checks
        .iter()
        .map(|check| {
            json!({
                "name": check.name,
                "passed": checks_pass,
                "issue": (!checks_pass).then_some(check_issue)
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
                "issue": (value < score.target_score).then_some(score_issue)
            })
        })
        .collect::<Vec<_>>();
    json!({"checks": checks, "scores": scores}).to_string()
}

async fn runtime(responses: Vec<String>, max_repair_rounds: u32) -> (Runtime, Arc<Mutex<Vec<LlmCompletionRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedValidationProvider {
        responses: Mutex::new(responses.into()),
        requests: Arc::clone(&requests),
    };
    let mut config = PipelineConfig::default();
    config.validation_loop.max_repair_rounds = max_repair_rounds;
    let runtime = Runtime::new(
        Arc::new(LlmGateway::new(Arc::new(provider), LlmConfig::default())),
        Arc::new(prompt()),
        Arc::new(store_with_default_pack().await),
        config,
    )
    .expect("runtime config should be valid");
    (runtime, requests)
}

async fn run_validation_loop(responses: Vec<String>, max_repair_rounds: u32) -> ValidationLoopRun {
    let (runtime, requests) = runtime(responses, max_repair_rounds).await;
    let story_ctx = story_context();
    let goal = text("Reach the gate.");
    let contribution = PlayerContribution {
        raw: "I follow.".to_owned(),
        processed: "The player follows.".to_owned(),
    };
    let knowledge = ActivatedKnowledge::default();
    let context = ValidationContext {
        story_goal: &goal,
        player_contribution: &contribution,
        directions: &[],
        constraints: &[],
        knowledge: &knowledge,
        retrieved_facts: &[],
        retrieved_rumors: &[],
        role_ids: &[],
        character_decisions: &[],
    };
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let sink = CapturingSink::default();
    let trace = trace();
    let runner = PipelineRunner {
        control: &control,
        sink: &sink,
        trace: &trace,
    };
    let result = runtime
        .validate_repair_loop(&runner, &story_ctx, context, "Initial proposal.".to_owned())
        .await;
    let requests = lock(&requests).clone();
    let stages = lock(&sink.stages).clone();
    ValidationLoopRun {
        result,
        requests,
        stages,
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn score(name: &str, min_score: f64, max_score: f64, target_score: f64, value: f64) -> ValidationScoreResult {
    ValidationScoreResult {
        name: name.to_owned(),
        description: name.to_owned(),
        min_score,
        max_score,
        target_score,
        score: value,
        issue: (value < target_score).then(|| text("improve score")),
    }
}

fn proposal(version: u32, checks_pass: bool, scores: Vec<ValidationScoreResult>) -> ValidatedProposal {
    ValidatedProposal {
        proposal: format!("proposal-{version}"),
        report: ValidationReport {
            proposal_version: version,
            checks: vec![ValidationCheckResult {
                name: "gate".to_owned(),
                description: "gate".to_owned(),
                passed: checks_pass,
                issue: (!checks_pass).then(|| text("fix gate")),
            }],
            scores,
        },
    }
}

#[test]
fn target_gap_mse_normalizes_different_score_ranges() {
    let narrow = proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]);
    let wide = proposal(2, true, vec![score("quality", 0.0, 100.0, 80.0, 70.0)]);

    assert_eq!(target_gap_mse(&narrow).expect("metric"), target_gap_mse(&wide).expect("metric"));
}

#[test]
fn target_gap_mse_ignores_scores_at_or_above_target() {
    let at_target = proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 8.0)]);
    let above_target = proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 10.0)]);

    assert_eq!(target_gap_mse(&at_target).expect("metric"), 0.0);
    assert_eq!(target_gap_mse(&above_target).expect("metric"), 0.0);
}

#[test]
fn choice_excludes_failed_check_candidates() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 6.0)]),
        proposal(2, false, vec![score("quality", 0.0, 10.0, 8.0, 7.9)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 1);
}

#[test]
fn choice_prefers_smaller_target_gap_mse() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 5.0)]),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_breaks_metric_tie_by_fewer_unmet_scores() {
    let history = vec![
        proposal(
            1,
            true,
            vec![
                score("quality", 0.0, 10.0, 8.0, 3.0),
                score("goal", 0.0, 10.0, 8.0, 3.0),
            ],
        ),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 3.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_breaks_full_tie_by_newer_proposal() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_rejects_history_without_eligible_candidate() {
    let history = vec![proposal(3, false, vec![score("quality", 0.0, 10.0, 8.0, 7.0)])];

    assert!(matches!(
        select_choice(&history),
        Err(PipelineError::ValidationFailed {
            code: "validation_choice_unavailable",
            proposal_version: 3,
            ..
        })
    ));
}

#[test]
fn choice_rejects_empty_score_set() {
    let candidate = proposal(4, true, Vec::new());

    assert!(matches!(
        target_gap_mse(&candidate),
        Err(PipelineError::ValidationFailed {
            code: "validation_choice_scores_empty",
            proposal_version: 4,
            ..
        })
    ));
}

#[tokio::test]
async fn runtime_shares_one_policy_across_generate_validate_and_repair() {
    let (runtime, _) = runtime(Vec::new(), 1).await;

    assert!(runtime.stages_share_quality_policy());
}

#[tokio::test]
async fn runtime_repairs_with_current_report_feedback_only() {
    let run = run_validation_loop(
        vec![
            validation_response(false, None, "First report issue.", "Unused score issue."),
            "Repair one.".to_owned(),
            validation_response(false, None, "Current report issue.", "Unused score issue."),
            "Repair two.".to_owned(),
            validation_response(true, None, "Unused check issue.", "Unused score issue."),
        ],
        2,
    )
    .await;
    let repair_prompts = run
        .requests
        .iter()
        .map(request_text)
        .filter(|request| request.contains("# Repair Issues"))
        .collect::<Vec<_>>();

    assert!(run.result.is_ok());
    assert_eq!(repair_prompts.len(), 2);
    assert!(repair_prompts[0].contains("First report issue."));
    assert!(repair_prompts[1].contains("Current report issue."));
    assert!(!repair_prompts[1].contains("First report issue."));
    assert!(!repair_prompts[1].contains("Initial proposal."));
}

#[tokio::test]
async fn runtime_revalidates_complete_policy_after_repair() {
    let run = run_validation_loop(
        vec![
            validation_response(false, None, "Fix the check.", "Unused score issue."),
            "Repaired proposal.".to_owned(),
            validation_response(true, None, "Unused check issue.", "Unused score issue."),
        ],
        1,
    )
    .await;
    let validate_prompts = run
        .requests
        .iter()
        .map(request_text)
        .filter(|request| request.contains("# Check Criteria"))
        .collect::<Vec<_>>();
    let config = ValidateConfig::default();

    assert!(run.result.is_ok());
    assert_eq!(validate_prompts.len(), 2);
    for prompt in validate_prompts {
        for check in &config.checks {
            assert!(prompt.contains(&check.name));
        }
        for score in &config.scores {
            assert!(prompt.contains(&score.name));
        }
    }
}

#[tokio::test]
async fn runtime_score_feedback_uses_validated_current_and_target_values() {
    let current_score = 6.25;
    let run = run_validation_loop(
        vec![
            validation_response(true, Some(current_score), "Unused check issue.", "Improve the score."),
            "Repaired proposal.".to_owned(),
            validation_response(true, None, "Unused check issue.", "Unused score issue."),
        ],
        1,
    )
    .await;
    let repair_prompt = run
        .requests
        .iter()
        .map(request_text)
        .find(|request| request.contains("# Repair Issues"))
        .expect("repair prompt");

    assert!(run.result.is_ok());
    assert!(repair_prompt.contains(&format!("\"current_score\":{current_score}")));
    assert!(repair_prompt.contains("\"target_score\":8.0"));
}

#[tokio::test]
async fn runtime_failed_repair_consumes_reserved_budget() {
    let run = run_validation_loop(
        vec![
            validation_response(true, Some(7.0), "Unused check issue.", "Improve the score."),
            " \n\t".to_owned(),
        ],
        1,
    )
    .await;

    assert!(matches!(
        run.result,
        Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Repair
        })
    ));
    assert_eq!(run.stages.iter().filter(|stage| stage.as_str() == "validate").count(), 1);
    assert_eq!(run.stages.iter().filter(|stage| stage.as_str() == "repair").count(), 1);
}
