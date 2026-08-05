use aise::AiseError;
use aise::core::story_proposal::{ProposedWorldChange, StoryProposal};
use aise::core::turn_budget::{TurnBudget, TurnBudgetLimits};
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, LlmUsageAggregate, StoryRevision, TurnCancellation, TurnControl, TurnIdentity,
    TurnPhase, TurnRequest,
};
use aise::core::turn_data::{BaselineContext, ContextItem, ContextSource, StoryReadSnapshot, WriterPlan};
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::core::turn_validation::{StateChange, ValidatedChangeSet, ValidationDecision, ValidationResult};
use aise::domain::ids::{StoryId, TurnId};
use std::time::{Duration, Instant};

fn budget() -> TurnBudget {
    TurnBudget::new(TurnBudgetLimits {
        max_repair_rounds: 3,
        max_llm_calls: 8,
        max_input_tokens: 8_192,
        max_output_tokens: 2_048,
        max_total_tokens: 10_240,
        max_retrieved_items: 5,
        ..Default::default()
    })
}

fn identity() -> TurnIdentity {
    TurnIdentity::new(
        StoryId::from("story-1"),
        TurnId::from("turn-1"),
        IdempotencyKey::try_new("key-1".to_string()).unwrap(),
        1000,
    )
    .unwrap()
}

fn new_ctx() -> TurnExecutionContext {
    TurnExecutionContext::new(
        identity(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        budget(),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap()
}

fn proposal(text: &str) -> StoryProposal {
    StoryProposal {
        story_text: text.to_string(),
        events: Vec::new(),
        character_changes: Vec::new(),
        world_change: ProposedWorldChange::default(),
        memory_changes: Vec::new(),
        summary_delta: None,
    }
}

fn change_set(text: &str) -> ValidatedChangeSet {
    ValidatedChangeSet::new(
        text.to_string(),
        Vec::new(),
        Vec::new(),
        StateChange::Unchanged,
        Vec::new(),
        None,
    )
}

fn empty_snapshot() -> StoryReadSnapshot {
    StoryReadSnapshot::new(
        StoryId::from("story-1"),
        StoryRevision::new(0),
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn advance_to_proposal(ctx: &mut TurnExecutionContext) {
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    ctx.complete_context_preparation().unwrap();
    ctx.set_story_proposal(proposal("text")).unwrap();
}

fn committed(turn_id: &str) -> CommittedTurnResult {
    CommittedTurnResult {
        turn_id: TurnId::from(turn_id),
        story_revision: StoryRevision::new(1),
        story_text: "text".into(),
        llm_usage: LlmUsageAggregate::default(),
    }
}

#[test]
fn context_rejects_empty_identity() {
    assert!(matches!(
        TurnRequest::try_new("   ".to_string()),
        Err(AiseError::InvalidRequest(_))
    ));
    assert!(
        TurnIdentity::new(
            StoryId::from(""),
            TurnId::from("turn-1"),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            0,
        )
        .is_err()
    );
    assert!(
        TurnIdentity::new(
            StoryId::from("story-1"),
            TurnId::from(""),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            0,
        )
        .is_err()
    );
    assert!(IdempotencyKey::try_new(String::new()).is_err());
}

#[test]
fn request_normalizes_and_digests_stable() {
    let a = TurnRequest::try_new("  开始吧  ".to_string()).unwrap();
    let b = TurnRequest::try_new("开始吧".to_string()).unwrap();
    assert_eq!(a.player_input(), "开始吧");
    assert_eq!(a.request_digest(), b.request_digest());
}

#[test]
fn context_rejects_invalid_phase_transition() {
    let mut ctx = new_ctx();
    assert!(ctx.set_writer_plan(WriterPlan::default()).is_err());
    assert!(ctx.set_story_proposal(proposal("text")).is_err());
    assert!(
        ctx.set_validation_result(ValidationResult::reject("bad", "rejected"), None)
            .is_err()
    );
    assert!(ctx.set_committed_result(committed("turn-1")).is_err());
}

#[tokio::test]
async fn initializer_does_not_access_external_services() {
    let mut ctx = new_ctx();
    let initializer = aise::runtime::TurnInitializer;
    assert!(initializer.execute(&mut ctx).await.is_ok());
    assert_eq!(ctx.phase(), TurnPhase::Initialized);
}

#[test]
fn bounded_outputs_reject_over_limit_values() {
    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();

    let item = |i: usize| ContextItem {
        source: ContextSource::HistoricalStory,
        content: format!("item {i}"),
        score: 1.0,
    };
    let over: Vec<ContextItem> = (0..6).map(item).collect();
    assert!(ctx.set_retrieved_context(over).is_err());

    let within: Vec<ContextItem> = (0..5).map(item).collect();
    assert!(ctx.set_retrieved_context(within).is_ok());
}

#[test]
fn context_advances_through_phases() {
    let mut ctx = new_ctx();
    assert_eq!(ctx.phase(), TurnPhase::Created);

    ctx.complete_initialization().unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Initialized);

    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Prepared);

    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Planned);

    ctx.complete_context_preparation().unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ContextReady);

    ctx.set_story_proposal(proposal("text")).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ProposalReady);

    ctx.set_validation_result(ValidationResult::pass(), Some(change_set("text")))
        .unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);

    ctx.set_committed_result(committed("turn-1")).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Committed);
}

#[test]
fn failed_validation_never_reaches_ready_to_commit() {
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);

    ctx.set_validation_result(ValidationResult::reject("bad", "rejected"), None)
        .unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Failed);
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Reject);
    assert!(ctx.change_set().is_none());
    assert!(ctx.set_committed_result(committed("turn-1")).is_err());
}

#[test]
fn pass_is_the_only_decision_that_carries_change_set() {
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);

    assert!(ctx.set_validation_result(ValidationResult::pass(), None).is_err());
    assert!(
        ctx.set_validation_result(ValidationResult::reject("bad", "rejected"), Some(change_set("text")))
            .is_err()
    );
    assert!(
        ctx.set_validation_result(ValidationResult::repair("fixable", "fix me"), Some(change_set("text")))
            .is_err()
    );

    ctx.set_validation_result(ValidationResult::pass(), Some(change_set("text")))
        .unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    assert_eq!(ctx.change_set().unwrap().story_text(), "text");
}

#[test]
fn repair_invalidates_previous_validation_and_change_set() {
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);

    ctx.set_validation_result(ValidationResult::repair("fixable", "fix me"), None)
        .unwrap();
    assert_eq!(ctx.phase(), TurnPhase::RepairRequired);
    assert_eq!(ctx.proposal_revision(), 0);

    ctx.replace_story_proposal(proposal("v2")).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ProposalReady);
    assert_eq!(ctx.proposal_revision(), 1);
    assert!(ctx.validation().is_none());
    assert!(ctx.change_set().is_none());

    ctx.set_validation_result(ValidationResult::pass(), Some(change_set("v2")))
        .unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);
    assert_eq!(ctx.change_set().unwrap().story_text(), "v2");
}
