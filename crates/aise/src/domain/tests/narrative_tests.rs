use super::*;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::TurnId;

fn limits() -> StoryContinuityLimits {
    StoryContinuityLimits {
        max_summary_bytes: 1024,
        max_recent_segments: 8,
        max_recent_segment_bytes: 256,
        max_recent_segment_tokens: 64,
    }
}

fn segment(sequence: u64, text: &str) -> StorySegment {
    StorySegment {
        sequence: StorySequence::try_new(sequence).expect("sequence"),
        origin: StorySegmentOrigin::Turn {
            turn_id: TurnId::try_new(format!("turn-{sequence}")).expect("turn_id"),
        },
        text: BoundedText::try_new(text.to_string(), "segment", 256).expect("text"),
    }
}

fn opening(text: &str) -> StorySegment {
    StorySegment {
        sequence: StorySequence::try_new(1).expect("sequence"),
        origin: StorySegmentOrigin::Opening,
        text: BoundedText::try_new(text.to_string(), "segment", 256).expect("text"),
    }
}

#[test]
fn story_sequence_rejects_zero_and_overflow() {
    assert_eq!(StorySequence::try_new(0), Err(StoryContinuityError::ZeroSequence));
    assert_eq!(
        StorySequence::try_new(u64::MAX).and_then(StorySequence::next),
        Err(StoryContinuityError::SequenceOverflow)
    );
}

#[test]
fn continuity_without_summary_starts_at_one() {
    let summary = StorySummary {
        text: BoundedText::try_new(String::new(), "summary", 1024).unwrap(),
        summarized_through: None,
    };
    let ok = StoryContinuity::try_new(summary.clone(), vec![segment(1, "one"), segment(2, "two")], limits());
    assert!(ok.is_ok());
    let bad = StoryContinuity::try_new(summary, vec![segment(2, "two")], limits());
    assert_eq!(bad, Err(StoryContinuityError::Gap));
}

#[test]
fn continuity_summary_and_recent_are_adjacent() {
    let summary = StorySummary {
        text: BoundedText::try_new(String::from("past"), "summary", 1024).unwrap(),
        summarized_through: Some(StorySequence::try_new(3).unwrap()),
    };
    let ok = StoryContinuity::try_new(summary, vec![segment(4, "four"), segment(5, "five")], limits());
    assert!(ok.is_ok());
}

#[test]
fn continuity_rejects_overlap_gap_duplicate_and_disorder() {
    let with_summary = StorySummary {
        text: BoundedText::try_new(String::from("past"), "summary", 1024).unwrap(),
        summarized_through: Some(StorySequence::try_new(3).unwrap()),
    };
    assert_eq!(
        StoryContinuity::try_new(with_summary.clone(), vec![segment(3, "three")], limits()),
        Err(StoryContinuityError::Overlap)
    );
    assert_eq!(
        StoryContinuity::try_new(with_summary, vec![segment(5, "five")], limits()),
        Err(StoryContinuityError::Gap)
    );
    let empty = StorySummary {
        text: BoundedText::try_new(String::new(), "summary", 1024).unwrap(),
        summarized_through: None,
    };
    assert_eq!(
        StoryContinuity::try_new(empty.clone(), vec![segment(1, "a"), segment(1, "b")], limits()),
        Err(StoryContinuityError::OutOfOrder)
    );
    assert_eq!(
        StoryContinuity::try_new(empty, vec![segment(1, "a"), segment(3, "c")], limits()),
        Err(StoryContinuityError::Gap)
    );
}

#[test]
fn continuity_budget_never_silently_drops_segments() {
    let summary = StorySummary {
        text: BoundedText::try_new(String::new(), "summary", 1024).unwrap(),
        summarized_through: None,
    };
    let tight = StoryContinuityLimits {
        max_summary_bytes: 1024,
        max_recent_segments: 8,
        max_recent_segment_bytes: 256,
        max_recent_segment_tokens: 1,
    };
    let result = StoryContinuity::try_new(summary, vec![segment(1, "abcdefgh")], tight);
    assert_eq!(
        result,
        Err(StoryContinuityError::LimitExceeded {
            limit: "max_recent_segment_tokens",
        })
    );
}

#[test]
fn opening_only_history_is_not_summarizable() {
    let continuity = StoryContinuity::try_new(StorySummary::default(), vec![opening("opening")], limits()).unwrap();

    assert!(!continuity.has_summarizable_history());
}

#[test]
fn generated_segment_makes_history_summarizable() {
    let continuity = StoryContinuity::try_new(
        StorySummary::default(),
        vec![opening("opening"), segment(2, "generated")],
        limits(),
    )
    .unwrap();

    assert!(continuity.has_summarizable_history());
}
