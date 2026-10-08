use super::*;
use crate::core::{
    IdempotencyKey, PlayerContribution, StorySummary, Turn, TurnEvaluation, TurnNumber, TurnSegment, TurnStatus,
    WorldChange,
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
        name: "summary-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn summary(text: &str) -> StorySummary {
    StorySummary {
        text: text.to_owned(),
        covered_through: TurnNumber::new(1),
    }
}

fn accepted_turn(number: u64, segment: &str) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("key-{number}")).expect("valid idempotency key"),
        player_contribution: PlayerContribution {
            raw: "raw".to_owned(),
            processed: format!("processed-{number}"),
        },
        turn_segment: TurnSegment::new(segment.to_owned()),
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: TurnStatus::Accepted,
    }
}

fn var<'a>(vars: &'a PromptVars, name: &str) -> &'a str {
    vars.get(name).and_then(Value::as_str).expect("string var")
}

async fn runtime_context(story_ctx: &StoryContext, covered_turns: usize) -> String {
    let store = StoryStoreMem::new();
    let trace = test_trace();
    let rendered = summarize_story(&bundled_prompt(), story_ctx, covered_turns, &store, trace.root())
        .await
        .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

#[test]
fn vars_carry_only_the_covered_leading_turns() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  "));
    story_ctx.rencent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.rencent_turns.push_back(accepted_turn(3, "The hall is dark."));
    story_ctx.rencent_turns.push_back(accepted_turn(4, "A candle flickers."));

    let vars = summary_vars(&story_ctx, 2, String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "story_to_summarize"), "The door creaks.\n\nThe hall is dark.");
}

#[tokio::test]
async fn first_summary_context_has_opening_then_story_to_summarize() {
    let mut story_ctx = StoryContext::new();
    story_ctx.rencent_turns.push_back(accepted_turn(1, "The door creaks."));
    let store = StoryStoreMem::new();
    let opening = store.get_pack(&story_ctx.pack_ref.pack_id).await.expect("pack loads").Opening;

    let content = runtime_context(&story_ctx, 1).await;

    assert_eq!(
        content,
        format!("## Story Opening\n\n{opening}\n\n## Story To Summarize\n\nThe door creaks.")
    );
}

#[tokio::test]
async fn merge_context_orders_previous_summary_then_story_to_summarize() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    story_ctx.rencent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.rencent_turns.push_back(accepted_turn(3, "The hall is dark."));

    let content = runtime_context(&story_ctx, 1).await;

    assert_eq!(
        content,
        "## Previous Story Summary\n\nEarlier events.\n\n## Story To Summarize\n\nThe door creaks."
    );
}
