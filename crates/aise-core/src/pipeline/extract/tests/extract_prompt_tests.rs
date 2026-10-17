use super::*;
use crate::core::tests::snake_pack;
use crate::core::{BoundedText, NarrativeConditionId, StorySummary, TurnNumber};
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
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "extract-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn query(id: &str, criterion: &str) -> SemanticQuery {
    SemanticQuery {
        condition_id: NarrativeConditionId::try_new(id).expect("valid condition id"),
        criterion: BoundedText::try_new(criterion, "criterion", 256).expect("valid criterion"),
    }
}

fn render(story_ctx: &StoryContext) -> String {
    let prompt = bundled_prompt();
    let story_pack = snake_pack();
    let queries = [query("gate_open", "The gate is open")];
    let trace = test_trace();
    evaluate_narrative(
        &prompt,
        EvaluateNarrativePrompt {
            story_pack: &story_pack,
            story_ctx,
            proposal: "The gate opens.",
            queries: &queries,
        },
        trace.root(),
    )
    .expect("prompt should render")
    .messages()[1]
        .content
        .replace("\r\n", "\n")
}

#[test]
fn prompt_uses_opening_without_summary_and_serializes_queries() {
    let story_ctx = StoryContext::new();

    let content = render(&story_ctx);

    assert!(content.contains("# Story Opening"));
    assert!(!content.contains("# Story Summary"));
    assert!(content.contains("# Current Proposal\n\nThe gate opens."));
    assert!(content.contains(r#"[{"condition_id":"gate_open","criterion":"The gate is open"}]"#));
}

#[test]
fn prompt_uses_summary_instead_of_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(StorySummary {
        text: "Earlier events.".to_owned(),
        covered_through: TurnNumber::new(2),
    });

    let content = render(&story_ctx);

    assert!(content.contains("# Story Summary\n\nEarlier events."));
    assert!(!content.contains("# Story Opening"));
}
