use super::*;
use crate::domain::ids::{StoryRevision, TurnNumber};
use crate::turn::turn_contract::LlmUsageAggregate;
use crate::turn::turn_pipeline::TurnStage;

#[test]
fn committed_root_output_contains_status_and_story_text() {
    let outcome = TurnRunOutcome::Committed {
        result: CommittedTurnResult {
            turn_number: TurnNumber::try_new(1).unwrap(),
            story_revision: StoryRevision::new(1),
            story_text: "The committed story.".into(),
            llm_usage: LlmUsageAggregate::default(),
            llm_calls: Vec::new(),
        },
        replayed: false,
    };

    let output = serde_json::to_value(root_trace_output(&outcome)).unwrap();

    assert_eq!(output["status"], "committed");
    assert_eq!(output["story_text"], "The committed story.");
    assert!(output.get("error_code").is_none());
}

#[test]
fn failed_root_output_contains_bounded_diagnostic_fields() {
    let outcome = TurnRunOutcome::Failed(TurnExecutionError::new(
        TurnFailureKind::DeadlineExceeded,
        "deadline_exceeded",
        Some(TurnStage::StoryGenerator),
        "turn deadline exceeded",
    ));

    let output = serde_json::to_value(root_trace_output(&outcome)).unwrap();

    assert_eq!(output["status"], "deadline_exceeded");
    assert_eq!(output["error_code"], "deadline_exceeded");
    assert_eq!(output["failure_kind"], "deadline_exceeded");
    assert_eq!(output["stage"], "story_generator");
    assert!(output.get("story_text").is_none());
}
