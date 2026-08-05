use aise::AiseError;
use aise::core::story_proposal::{ProposedEvent, ProposedWorldChange, StoryProposal};
use aise::core::turn_budget::{TurnBudget, TurnBudgetLimits};
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, LlmUsageAggregate, RequestDigest, StoryRevision, TurnCancellation,
    TurnControl, TurnIdentity, TurnPhase, TurnRequest,
};
use aise::core::turn_data::{BaselineContext, CharacterThought, SnapshotLimits, StoryReadSnapshot, WriterPlan};
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::core::turn_validation::{StateChange, ValidatedChangeSet, ValidationDecision};
use aise::domain::ids::{CharacterId, FactId, MemoryId, StoryId, TurnId};
use aise::domain::memory::{MemoryEntry, MemoryKind};
use aise::domain::narrative::{EventKind, StoryTurn};
use aise::domain::world::{FactSource, WorldFact, WorldState};
use aise::persistence::{SqliteStore, Store, StoreError, StoredTurnOutcome, TurnCommit, TurnCommitter};
use aise::validation::ValidationPipeline;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct NoCommitStore;

#[async_trait]
impl Store for NoCommitStore {
    async fn load_story_snapshot(
        &self,
        _story_id: &StoryId,
        _limits: SnapshotLimits,
    ) -> Result<Option<StoryReadSnapshot>, StoreError> {
        Ok(None)
    }

    async fn create_story(
        &self,
        _story_id: &StoryId,
        _player_character_id: Option<&CharacterId>,
        _created_at: i64,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    async fn find_committed_turn(
        &self,
        _story_id: &StoryId,
        _idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoredTurnOutcome>, StoreError> {
        Ok(None)
    }

    async fn commit_turn(&self, _commit: &TurnCommit) -> Result<CommittedTurnResult, StoreError> {
        panic!("commit_turn must not be called")
    }
}

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

fn new_ctx() -> TurnExecutionContext {
    TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::from("story-1"),
            TurnId::from("turn-1"),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            1000,
        )
        .unwrap(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        budget(),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap()
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

fn proposal_with(text: &str, events: Vec<ProposedEvent>) -> StoryProposal {
    StoryProposal {
        story_text: text.to_string(),
        events,
        character_changes: Vec::new(),
        world_change: ProposedWorldChange::default(),
        memory_changes: Vec::new(),
        summary_delta: None,
    }
}

fn advance_to_proposal(ctx: &mut TurnExecutionContext) {
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    ctx.complete_context_preparation().unwrap();
}

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_vc_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn commit(
    story_id: &StoryId,
    base: StoryRevision,
    key: &str,
    turn_id: &str,
    text: &str,
    world: StateChange<WorldState>,
) -> TurnCommit {
    TurnCommit {
        story_id: story_id.clone(),
        turn: StoryTurn {
            id: TurnId::from(turn_id),
            player_input: "input".into(),
            story_text: text.into(),
            summary_delta: None,
            created_at: 1000,
        },
        events: Vec::new(),
        characters: Vec::new(),
        world,
        memory: Vec::new(),
        base_revision: base,
        idempotency_key: IdempotencyKey::try_new(key.to_string()).unwrap(),
        request_digest: RequestDigest::from_stored(format!("digest-{key}")),
        player_character_id: None,
        outbox: Vec::new(),
        llm_usage: LlmUsageAggregate::default(),
    }
}

fn limits() -> SnapshotLimits {
    SnapshotLimits {
        max_recent_turns: 20,
        max_memories: 20,
    }
}

#[tokio::test]
async fn proposal_cannot_be_committed_directly() {
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(proposal_with("text", Vec::new())).unwrap();
    assert_eq!(ctx.phase(), TurnPhase::ProposalReady);

    let committer = TurnCommitter::new(Arc::new(NoCommitStore));
    let error = committer
        .execute(&mut ctx)
        .await
        .expect_err("unvalidated proposal must be rejected");
    assert!(matches!(error, AiseError::InvariantViolation(_)));
    assert_eq!(ctx.phase(), TurnPhase::ProposalReady);
}

#[tokio::test]
async fn committer_rejects_non_ready_context() {
    let mut ctx = new_ctx();
    let committer = TurnCommitter::new(Arc::new(NoCommitStore));
    let error = committer
        .execute(&mut ctx)
        .await
        .expect_err("non-ready context must be rejected");
    assert!(matches!(error, AiseError::InvariantViolation(_)));
}

#[tokio::test]
async fn pass_is_the_only_decision_that_produces_change_set() {
    let pipeline = ValidationPipeline::default();

    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(proposal_with(
        "story text",
        vec![ProposedEvent {
            kind: EventKind::Action,
            summary: "story text".into(),
        }],
    ))
    .unwrap();
    pipeline.execute(&mut ctx).await.expect("validation must run");
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    assert_eq!(ctx.phase(), TurnPhase::ReadyToCommit);
    assert!(ctx.change_set().is_some());

    let mut rejected = new_ctx();
    advance_to_proposal(&mut rejected);
    rejected.set_story_proposal(proposal_with("   ", Vec::new())).unwrap();
    pipeline.execute(&mut rejected).await.expect("validation must run");
    assert_eq!(rejected.validation_decision().unwrap(), ValidationDecision::Reject);
    assert_eq!(rejected.phase(), TurnPhase::Failed);
    assert!(rejected.change_set().is_none());
}

#[tokio::test]
async fn deterministic_failure_cannot_be_overridden_by_narrative_validator() {
    let pipeline = ValidationPipeline::default();
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    ctx.set_story_proposal(proposal_with("", Vec::new())).unwrap();
    pipeline.execute(&mut ctx).await.expect("validation must run");
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Reject);
    assert!(ctx.change_set().is_none());
}

#[tokio::test]
async fn first_turn_world_facts_create_world_from_missing_state() {
    let pipeline = ValidationPipeline::default();
    let mut ctx = new_ctx();
    advance_to_proposal(&mut ctx);
    let mut proposal = proposal_with(
        "story text",
        vec![ProposedEvent {
            kind: EventKind::Action,
            summary: "story text".into(),
        }],
    );
    proposal.world_change = ProposedWorldChange {
        add_facts: vec![
            "the inn is near the port".into(),
            "the king rules from the capital".into(),
        ],
    };
    ctx.set_story_proposal(proposal).unwrap();
    pipeline.execute(&mut ctx).await.expect("validation must run");
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    match ctx.change_set().unwrap().world_change() {
        StateChange::Replace(world) => {
            assert_eq!(world.id, StoryId::from("story-1"));
            assert_eq!(world.facts.len(), 2);
            assert_eq!(world.facts[0].text, "the inn is near the port");
            assert_eq!(world.facts[1].text, "the king rules from the capital");
            assert_eq!(world.facts[0].source, FactSource::CommittedTurn);
            assert_eq!(world.facts[0].id.as_str(), "story-1-fact-1");
            assert_eq!(world.facts[1].id.as_str(), "story-1-fact-2");
        }
        other => panic!("expected world Replace, got {other:?}"),
    }
}

#[tokio::test]
async fn character_thought_cannot_become_world_fact() {
    let pipeline = ValidationPipeline::default();
    let mut ctx = new_ctx();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(empty_snapshot(), BaselineContext::default()).unwrap();
    ctx.set_writer_plan(WriterPlan::default()).unwrap();
    ctx.set_character_thoughts(vec![CharacterThought {
        character_id: CharacterId::from("c-1"),
        perception: "the sky is green".into(),
        emotion: "curious".into(),
        goal: String::new(),
        possible_action: String::new(),
    }])
    .unwrap();
    ctx.complete_context_preparation().unwrap();
    ctx.set_story_proposal(proposal_with(
        "story text",
        vec![ProposedEvent {
            kind: EventKind::Action,
            summary: "story text".into(),
        }],
    ))
    .unwrap();
    pipeline.execute(&mut ctx).await.expect("validation must run");
    assert_eq!(ctx.validation_decision().unwrap(), ValidationDecision::Pass);
    assert!(matches!(ctx.change_set().unwrap().world_change(), StateChange::Unchanged));
    assert!(ctx.change_set().unwrap().character_changes().is_empty());
}

#[tokio::test]
async fn world_unchanged_does_not_overwrite_existing_world() {
    let db = temp_db_path("world");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-w");
    store.create_story(&story_id, None, 1000).await.expect("create story");
    let world = WorldState {
        id: story_id.clone(),
        name: "the realm".into(),
        facts: vec![WorldFact {
            id: FactId::from("fact-1"),
            text: "the capital is airon".into(),
            source: FactSource::Seed,
        }],
    };

    let first = store
        .commit_turn(&commit(
            &story_id,
            StoryRevision::new(0),
            "key-1",
            "turn-1",
            "first",
            StateChange::Replace(world.clone()),
        ))
        .await
        .expect("commit with world");
    assert_eq!(first.story_revision, StoryRevision::new(1));

    let second = store
        .commit_turn(&commit(
            &story_id,
            StoryRevision::new(1),
            "key-2",
            "turn-2",
            "second",
            StateChange::Unchanged,
        ))
        .await
        .expect("commit without world change");
    assert_eq!(second.story_revision, StoryRevision::new(2));

    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let loaded = snapshot.world().expect("world exists");
    assert_eq!(loaded.facts.len(), 1);
    assert_eq!(loaded.facts[0].text, "the capital is airon");
    assert_eq!(loaded.name, "the realm");
    assert_eq!(snapshot.recent_turns().len(), 2);

    let _ = std::fs::remove_file(&db);
}

#[test]
fn change_set_is_constructible_only_through_validation_types() {
    let change_set = ValidatedChangeSet::new(
        "text".into(),
        Vec::new(),
        Vec::new(),
        StateChange::Unchanged,
        vec![MemoryEntry {
            id: MemoryId::from("m-1"),
            owner: CharacterId::from("c-1"),
            kind: MemoryKind::Observed,
            content: "note".into(),
            created_at: 1,
        }],
        None,
    );
    assert_eq!(change_set.story_text(), "text");
    assert_eq!(change_set.memory_changes().len(), 1);
}
