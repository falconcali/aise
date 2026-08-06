use aise::config::TurnConfig;
use aise::core::story_proposal::{ProposedEvent, ProposedWorldChange, StoryProposal};
use aise::core::turn_budget::TurnBudget;
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, LlmUsageAggregate, StoryRevision, TurnCancellation, TurnControl, TurnIdentity,
    TurnPhase, TurnRequest,
};
use aise::core::turn_data::{BaselineContext, CharacterThought, ContextItem, ContextSource, StoryGoal, WriterPlan};
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::core::turn_validation::{
    BoundedValidationIssues, Repairability, ValidationDecision, ValidationIssue, ValidationIssueCode, ValidationResult,
};
use aise::domain::ids::{CharacterId, StoryId, TurnId};
use aise::domain::narrative::EventKind;
use aise::domain::story_state::{AuthoritativeStoryState, PlayerStoryState, StoryReadSnapshot};
use aise::validation::ValidationPipeline;
use std::time::{Duration, Instant};

fn budget() -> TurnBudget {
    let config = TurnConfig {
        max_repair_rounds: 3,
        max_llm_calls: 8,
        max_input_tokens: 8_192,
        max_output_tokens: 2_048,
        max_total_tokens: 10_240,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    TurnBudget::from_config(&config, &aise::config::TurnContentLimitsConfig::default()).unwrap()
}

fn identity() -> TurnIdentity {
    TurnIdentity::new(
        StoryId::try_new("story-1").unwrap(),
        TurnId::try_new("turn-1").unwrap(),
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

fn valid_proposal(text: &str) -> StoryProposal {
    StoryProposal {
        story_text: text.to_string(),
        events: vec![ProposedEvent {
            kind: EventKind::Action,
            summary: text.to_string(),
        }],
        character_changes: Vec::new(),
        world_change: ProposedWorldChange::default(),
        memory_changes: Vec::new(),
        scene_change: None,
        constraint_changes: Vec::new(),
        summary_change: None,
    }
}

fn empty_snapshot() -> StoryReadSnapshot {
    StoryReadSnapshot::new(
        StoryId::try_new("story-1").unwrap(),
        StoryRevision::new(0),
        AuthoritativeStoryState::default(),
        PlayerStoryState::default(),
        None,
        Vec::new(),
        Vec::new(),
    )
}

fn advance_to_proposal(ctx: &mut TurnExecutionContext) {
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    ctx.complete_context_preparation().unwrap();
}

fn committed(turn_id: &str) -> CommittedTurnResult {
    CommittedTurnResult {
        turn_id: TurnId::try_new(turn_id).unwrap(),
        story_revision: StoryRevision::new(1),
        story_text: "text".into(),
        llm_usage: LlmUsageAggregate::default(),
        llm_calls: Vec::new(),
    }
}

fn repair_issue() -> BoundedValidationIssues {
    BoundedValidationIssues::try_new(
        vec![ValidationIssue {
            code: ValidationIssueCode::NarrativeInconsistent,
            message: "fix me".into(),
            repairability: Repairability::Repairable,
            location: None,
        }],
        10,
    )
    .unwrap()
}

fn reject_issue() -> BoundedValidationIssues {
    BoundedValidationIssues::try_new(
        vec![ValidationIssue {
            code: ValidationIssueCode::NarrativeInconsistent,
            message: "rejected".into(),
            repairability: Repairability::Fatal,
            location: None,
        }],
        10,
    )
    .unwrap()
}

#[test]
fn context_rejects_empty_identity() {
    assert!(matches!(
        TurnRequest::try_new("   ".to_string()),
        Err(error) if error.to_string().contains("player input")
    ));
    assert!(StoryId::try_new("").is_err());
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
    assert!(ctx.set_story_proposal(valid_proposal("text")).is_err());
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
fn bounded_outputs_reject_plan_snapshot_retrieval_thought_proposal_and_validation_limits() {
    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    let oversized_plan = WriterPlan {
        retrieval_requests: (0..64)
            .map(|index| aise::core::turn_data::ContextRequest {
                query: format!("the hidden treasure of the ancient port city number {index} ").repeat(8),
                sources: vec![ContextSource::HistoricalStory],
            })
            .collect(),
        character_requests: Vec::new(),
        story_goal: StoryGoal::default(),
    };
    assert!(
        ctx.set_writer_plan(oversized_plan).is_err(),
        "writer plan over the byte limit must be rejected"
    );

    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    let oversized_baseline = BaselineContext {
        recent_story: vec!["x".repeat(2000); 100],
        ..BaselineContext::default()
    };
    assert!(
        ctx.set_prepared_context(empty_snapshot(), oversized_baseline).is_err(),
        "prepared context over the token budget must be rejected"
    );

    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    let over_bytes: Vec<ContextItem> = (0..5)
        .map(|index| ContextItem {
            source: ContextSource::HistoricalStory,
            content: format!("fact {index} ").repeat(200),
            score: 1.0,
        })
        .collect();
    assert!(
        ctx.set_retrieved_context(over_bytes).is_err(),
        "retrieved context over the byte limit must be rejected"
    );

    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    let over_thoughts: Vec<CharacterThought> = (0..9)
        .map(|index| CharacterThought {
            character_id: CharacterId::from("c-1"),
            perception: format!("perception {index}"),
            emotion: "calm".into(),
            goal: String::new(),
            possible_action: String::new(),
        })
        .collect();
    assert!(
        ctx.set_character_thoughts(over_thoughts).is_err(),
        "character thought count over the limit must be rejected"
    );
    let oversized_thought = vec![CharacterThought {
        character_id: CharacterId::from("c-1"),
        perception: "x".repeat(2000),
        emotion: String::new(),
        goal: String::new(),
        possible_action: String::new(),
    }];
    assert!(
        ctx.set_character_thoughts(oversized_thought).is_err(),
        "character thought bytes over the limit must be rejected"
    );

    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    let mut oversized_proposal = valid_proposal("text");
    oversized_proposal.story_text = "x".repeat(40_000);
    assert!(
        ctx.set_story_proposal(oversized_proposal).is_err(),
        "story proposal over the byte limit must be rejected"
    );

    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(valid_proposal("text")).unwrap();
    let long_message = BoundedValidationIssues::try_new(
        vec![ValidationIssue {
            code: ValidationIssueCode::NarrativeInconsistent,
            message: "中".repeat(400),
            repairability: Repairability::Fatal,
            location: None,
        }],
        10,
    )
    .unwrap();
    assert!(
        ctx.set_validation_result(ValidationResult::Reject(long_message)).is_err(),
        "validation issue message over the byte limit must be rejected"
    );

    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(valid_proposal("text")).unwrap();
    let too_many: Vec<ValidationIssue> = (0..33)
        .map(|index| ValidationIssue {
            code: ValidationIssueCode::NarrativeInconsistent,
            message: format!("issue {index}"),
            repairability: Repairability::Repairable,
            location: None,
        })
        .collect();
    assert!(
        BoundedValidationIssues::try_new(too_many, budget().max_validation_issues()).is_err(),
        "validation issue count over the limit must be rejected"
    );
}

#[test]
fn retrieval_token_limit_rejects_over_tokens_with_lax_byte_limit() {
    let mut config = TurnConfig {
        max_retrieved_items: 100,
        ..TurnConfig::default()
    };
    config.max_retrieval_candidates = 100;
    let content = aise::config::TurnContentLimitsConfig {
        max_retrieved_item_bytes: 100_000,
        max_retrieved_tokens: 1000,
        ..aise::config::TurnContentLimitsConfig::default()
    };
    let budget = TurnBudget::from_config(&config, &content).unwrap();
    let mut ctx = TurnExecutionContext::new(
        identity(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        budget,
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    let over_tokens: Vec<ContextItem> = (0..100)
        .map(|index| ContextItem {
            source: ContextSource::HistoricalStory,
            content: format!("the ancient port city fact number {index} says a lot"),
            score: 1.0,
        })
        .collect();
    assert!(
        ctx.set_retrieved_context(over_tokens).is_err(),
        "retrieved token total over the limit must be rejected"
    );
}

#[tokio::test]
async fn context_advances_through_phases() {
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

    ctx.set_story_proposal(valid_proposal("text")).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ProposalReady);

    let pipeline = ValidationPipeline::default();
    pipeline.execute(&mut ctx).await.unwrap();
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);

    ctx.set_committed_result(committed("turn-1")).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Committed);
}

#[test]
fn failed_validation_never_reaches_ready_to_commit() {
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(valid_proposal("text")).unwrap();

    ctx.set_validation_result(ValidationResult::Reject(reject_issue())).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::Failed);
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Reject);
    assert!(ctx.change_set().is_none());
    assert!(ctx.set_committed_result(committed("turn-1")).is_err());
}

#[tokio::test]
async fn pass_is_the_only_decision_that_carries_change_set() {
    let pipeline = ValidationPipeline::default();
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(valid_proposal("text")).unwrap();

    ctx.set_validation_result(ValidationResult::Repair(repair_issue())).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::RepairRequired);
    assert!(ctx.change_set().is_none(), "repair must not carry a change set");
    ctx.replace_story_proposal(valid_proposal("text")).unwrap();

    pipeline.execute(&mut ctx).await.unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    assert_eq!(ctx.change_set().unwrap().story_text(), "text");
}

#[tokio::test]
async fn repair_invalidates_previous_validation_and_change_set() {
    let pipeline = ValidationPipeline::default();
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(valid_proposal("text")).unwrap();
    pipeline.execute(&mut ctx).await.unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);
    assert!(ctx.change_set().is_some());

    let mut ctx2 = new_ctx();
    advance_to_proposal(&mut ctx2);
    ctx2.set_story_proposal(valid_proposal("text")).unwrap();
    ctx2.set_validation_result(ValidationResult::Repair(repair_issue())).unwrap();
    assert_eq!(ctx2.phase(), TurnPhase::RepairRequired);
    assert_eq!(ctx2.proposal_revision(), 0);

    ctx2.replace_story_proposal(valid_proposal("v2")).unwrap();
    assert_eq!(ctx2.phase(), TurnPhase::ProposalReady);
    assert_eq!(ctx2.proposal_revision(), 1);
    assert!(ctx2.validation().is_none());
    assert!(ctx2.change_set().is_none());

    pipeline.execute(&mut ctx2).await.unwrap();
    assert_eq!(ctx2.phase(), TurnPhase::ReadyToCommit);
    assert_eq!(ctx2.change_set().unwrap().story_text(), "v2");
}
