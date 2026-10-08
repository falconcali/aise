use super::*;
use crate::core::{
    IdempotencyKey, PlayerContribution, StorySummary, Turn, TurnEvaluation, TurnNumber, TurnSegment, WorldChange,
};
use crate::persistence::StoryStoreMem;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use std::path::Path;

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

fn bundled_prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads")
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
        name: "baseline-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn turn(number: u64, segment: &str, status: TurnStatus) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("key-{number}")).expect("valid idempotency key"),
        player_contribution: PlayerContribution {
            raw: format!("raw-{number}"),
            processed: format!("processed-{number}"),
        },
        turn_segment: TurnSegment::new(segment.to_owned()),
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: status,
    }
}

fn summary(text: &str, covered_through: u64) -> StorySummary {
    StorySummary {
        text: text.to_owned(),
        covered_through: TurnNumber::new(covered_through),
    }
}

fn var<'a>(vars: &'a PromptVars, name: &str) -> &'a str {
    vars.get(name).and_then(Value::as_str).expect("string var")
}

async fn runtime_context(story_ctx: &StoryContext, input: &str) -> String {
    let store = StoryStoreMem::new();
    let trace = test_trace();
    let rendered = process_player_input(&bundled_prompt(), story_ctx, input, &store, trace.root())
        .await
        .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

#[test]
fn vars_for_new_story_carry_opening_and_empty_summary_and_recent_story() {
    let story_ctx = StoryContext::new();
    let vars = player_input_vars(&story_ctx, "I wave.", "Rain falls.".to_owned());

    assert_eq!(var(&vars, "player_input"), "I wave.");
    assert_eq!(var(&vars, "story_opening"), "Rain falls.");
    assert_eq!(var(&vars, "story_summary"), "");
    assert_eq!(var(&vars, "recent_story"), "");
}

#[test]
fn vars_with_summary_carry_trimmed_summary_and_segments_only() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  ", 1));
    story_ctx
        .rencent_turns
        .push_back(turn(2, "The door creaks.", TurnStatus::Accepted));
    story_ctx
        .rencent_turns
        .push_back(turn(3, "The hall is dark.", TurnStatus::Accepted));
    let vars = player_input_vars(&story_ctx, "I look around.", String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "recent_story"), "The door creaks.\n\nThe hall is dark.");
}

#[test]
fn blank_summary_is_treated_as_absent() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  \n ", 1));

    assert!(story_summary(&story_ctx).is_none());
}

#[test]
fn recent_story_skips_rejected_turns_and_blank_segments() {
    let mut story_ctx = StoryContext::new();
    story_ctx
        .rencent_turns
        .push_back(turn(1, "The door creaks.", TurnStatus::Rejected));
    story_ctx
        .rencent_turns
        .push_back(turn(2, "  The hall is dark.  ", TurnStatus::Accepted));
    story_ctx.rencent_turns.push_back(turn(3, "  ", TurnStatus::Accepted));

    assert_eq!(recent_story(&story_ctx), "The hall is dark.");
}

#[tokio::test]
async fn opening_is_loaded_only_when_summary_is_absent() {
    let store = StoryStoreMem::new();
    let mut story_ctx = StoryContext::new();

    let opening = load_story_opening(&story_ctx, &store).await.expect("opening loads");
    assert!(!opening.is_empty());

    story_ctx.summary = Some(summary("Earlier events.", 1));
    let opening = load_story_opening(&story_ctx, &store).await.expect("opening skipped");
    assert!(opening.is_empty());
}

#[tokio::test]
async fn new_story_runtime_context_has_opening_then_input() {
    let story_ctx = StoryContext::new();
    let store = StoryStoreMem::new();
    let opening = store.get_pack(&story_ctx.pack_ref.pack_id).await.expect("pack loads").Opening;

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        format!("# Runtime Context\n\n## Story Opening\n\n{opening}\n\n## Raw Player Input\n\nI wave.")
    );
}

#[tokio::test]
async fn new_story_runtime_context_orders_opening_recent_story_and_input() {
    let store = StoryStoreMem::new();
    let mut story_ctx = StoryContext::new();
    story_ctx
        .rencent_turns
        .push_back(turn(1, "The door creaks.", TurnStatus::Accepted));
    let opening = store.get_pack(&story_ctx.pack_ref.pack_id).await.expect("pack loads").Opening;

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        format!(
            "# Runtime Context\n\n## Story Opening\n\n{opening}\n\n## Recent Story\n\nThe door creaks.\n\n## Raw Player Input\n\nI wave."
        )
    );
}

#[tokio::test]
async fn summarized_story_runtime_context_omits_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events.", 1));
    story_ctx
        .rencent_turns
        .push_back(turn(2, "The door creaks.", TurnStatus::Accepted));

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        "# Runtime Context\n\n## Story Summary\n\nEarlier events.\n\n## Recent Story\n\nThe door creaks.\n\n## Raw Player Input\n\nI wave."
    );
}
