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
        name: "generate-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn generate_input(goal: &str, processed: &str) -> GenerateInput {
    GenerateInput {
        story_goal: goal.to_owned(),
        player_contribution: PlayerContribution {
            raw: "raw".to_owned(),
            processed: processed.to_owned(),
        },
    }
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
            raw: format!("raw-{number}"),
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

async fn runtime_context(story_ctx: &StoryContext, input: &GenerateInput) -> String {
    let store = StoryStoreMem::new();
    let trace = test_trace();
    let rendered = generate_story(&bundled_prompt(), story_ctx, input, &store, trace.root())
        .await
        .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

#[test]
fn vars_for_new_story_carry_opening_and_empty_summary() {
    let story_ctx = StoryContext::new();
    let vars = generate_vars(&story_ctx, &generate_input("Goal.", "I wave."), "Rain falls.".to_owned());

    assert_eq!(var(&vars, "story_opening"), "Rain falls.");
    assert_eq!(var(&vars, "story_summary"), "");
    assert_eq!(var(&vars, "recent_story"), "");
    assert_eq!(var(&vars, "story_goal"), "Goal.");
    assert_eq!(var(&vars, "player_contribution"), "I wave.");
}

#[test]
fn vars_with_summary_and_recent_story_carry_both() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  "));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.recent_turns.push_back(accepted_turn(3, "The hall is dark."));
    let vars = generate_vars(&story_ctx, &generate_input("Goal.", "I wave."), String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "recent_story"), "The door creaks.\n\nThe hall is dark.");
}

#[tokio::test]
async fn new_story_context_has_opening_then_goal_and_contribution() {
    let story_ctx = StoryContext::new();
    let store = StoryStoreMem::new();
    let opening = store.get_pack(&story_ctx.pack_ref.pack_id).await.expect("pack loads").Opening;

    let content = runtime_context(&story_ctx, &generate_input("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Opening\n\n{opening}\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave."
        )
    );
}

#[tokio::test]
async fn recent_story_without_summary_keeps_opening_before_recent_story() {
    let mut story_ctx = StoryContext::new();
    story_ctx.recent_turns.push_back(accepted_turn(1, "The door creaks."));
    let store = StoryStoreMem::new();
    let opening = store.get_pack(&story_ctx.pack_ref.pack_id).await.expect("pack loads").Opening;

    let content = runtime_context(&story_ctx, &generate_input("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Opening\n\n{opening}\n\n# Recent Story\n\nThe door creaks.\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave."
        )
    );
}

#[tokio::test]
async fn summarized_story_context_orders_summary_recent_story_goal_and_contribution() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));

    let content = runtime_context(&story_ctx, &generate_input("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        "# Story Summary\n\nEarlier events.\n\n# Recent Story\n\nThe door creaks.\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave."
    );
}

#[tokio::test]
async fn summary_without_recent_story_omits_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));

    let content = runtime_context(&story_ctx, &generate_input("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        "# Story Summary\n\nEarlier events.\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave."
    );
}
