use super::*;
use crate::core::{
    BoundedText, Change, KnowledgeChange, PlayerId, StoryInstanceSpec, TurnCancellation, TurnCheckEvaluation,
    TurnChoiceEvaluation, TurnScoreEvaluation,
};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, TraceSpec,
};
use std::time::{Duration, Instant};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn trace() -> crate::trace::Trace {
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
        name: "commit-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn evaluation(choice: Option<TurnChoiceEvaluation>) -> TurnEvaluation {
    TurnEvaluation {
        proposal_version: 2,
        repair_rounds: 1,
        checks: vec![TurnCheckEvaluation {
            name: "logic".to_owned(),
            passed: true,
            issue: None,
        }],
        scores: vec![TurnScoreEvaluation {
            name: "quality".to_owned(),
            min_score: 0.0,
            max_score: 10.0,
            target_score: 8.0,
            score: 7.0,
            issue: Some(text("improve quality")),
        }],
        choice,
    }
}

async fn commit(status: TurnStatus, evaluation: TurnEvaluation) -> StoryCommit {
    let store = Arc::new(Store::default());
    let story_ctx = StoryContext::new();
    store
        .story_store()
        .create(StoryInstanceSpec {
            story_id: story_ctx.story_id.clone(),
            pack_ref: story_ctx.pack_ref.clone(),
            roles: story_ctx.roles.clone(),
            facts: story_ctx.facts.clone(),
            rumors: story_ctx.rumors.clone(),
            player_id: PlayerId::try_new("player").expect("valid player id"),
            player_role: story_ctx.player.clone(),
            narrative: story_ctx.narrative.clone(),
        })
        .await
        .expect("story creation succeeds");
    let pipeline = CommitPipeline::new(Arc::clone(&store));
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = trace();
    let expected = evaluation.clone();
    let committed = pipeline
        .execute(
            CommitInput {
                player_contribution: PlayerContribution {
                    raw: "raw".to_owned(),
                    processed: "processed".to_owned(),
                },
                idempotency_key: IdempotencyKey::try_new("turn-1").expect("valid idempotency key"),
                turn_segment: TurnSegment::new("story".to_owned()),
                knowledge_change: KnowledgeChange::default(),
                turn_status: status,
                turn_evaluation: evaluation,
                summary: Change::Unchanged,
                narrative: Change::Unchanged,
                life_cycle: Change::Unchanged,
            },
            &story_ctx,
            &control,
            trace.root(),
        )
        .await
        .expect("commit succeeds");
    let loaded = store.story_store().load(&story_ctx.story_id).await.expect("story loads");
    let persisted = loaded.recent_turns.back().expect("turn persisted");

    assert_eq!(persisted.turn_status, status);
    assert_eq!(persisted.turn_evaluation, expected);
    committed
}

#[tokio::test]
async fn commit_persists_pass_status_and_evaluation() {
    let evaluation = evaluation(None);

    let committed = commit(TurnStatus::Pass, evaluation.clone()).await;

    assert_eq!(committed.turn.turn_status, TurnStatus::Pass);
    assert_eq!(committed.turn.turn_evaluation, evaluation);
}

#[tokio::test]
async fn commit_persists_choice_status_and_evaluation() {
    let evaluation = evaluation(Some(TurnChoiceEvaluation {
        selected_proposal_version: 1,
        target_gap_mse: 0.01,
    }));

    let committed = commit(TurnStatus::Choice, evaluation.clone()).await;

    assert_eq!(committed.turn.turn_status, TurnStatus::Choice);
    assert_eq!(committed.turn.turn_evaluation, evaluation);
}
