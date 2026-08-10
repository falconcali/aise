use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{EventId, TurnId};
use serde::{Deserialize, Serialize};

pub use crate::domain::story_sequence::{StoryContinuityError, StorySequence};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorySegment {
    pub sequence: StorySequence,
    pub turn_id: TurnId,
    pub text: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: BoundedText,
    pub summarized_through: Option<StorySequence>,
}

impl Default for StorySummary {
    fn default() -> Self {
        Self {
            text: BoundedText::try_new(String::new(), "summary", usize::MAX).expect("empty summary fits"),
            summarized_through: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryContinuity {
    summary: StorySummary,
    recent_segments: Vec<StorySegment>,
}

#[derive(Debug, Clone, Copy)]
pub struct StoryContinuityLimits {
    pub max_summary_bytes: usize,
    pub max_recent_segments: usize,
    pub max_recent_segment_bytes: usize,
    pub max_recent_segment_tokens: u64,
}

impl StoryContinuity {
    pub fn try_new(
        summary: StorySummary,
        recent_segments: Vec<StorySegment>,
        limits: StoryContinuityLimits,
    ) -> Result<Self, StoryContinuityError> {
        if summary.text.as_str().len() > limits.max_summary_bytes {
            return Err(StoryContinuityError::LimitExceeded {
                limit: "max_summary_bytes",
            });
        }
        if recent_segments.len() > limits.max_recent_segments {
            return Err(StoryContinuityError::LimitExceeded {
                limit: "max_recent_segments",
            });
        }
        for segment in &recent_segments {
            if segment.text.as_str().len() > limits.max_recent_segment_bytes {
                return Err(StoryContinuityError::LimitExceeded {
                    limit: "max_recent_segment_bytes",
                });
            }
        }

        let summary_text = summary.text.as_str().trim();
        let summary_empty = summary_text.is_empty();
        match (summary_empty, summary.summarized_through) {
            (true, Some(_)) | (false, None) => {
                return Err(StoryContinuityError::InvalidSummaryBoundary);
            }
            (true, None) | (false, Some(_)) => {}
        }

        if let Some(boundary) = summary.summarized_through {
            if let Some(first) = recent_segments.first() {
                let expected = boundary.next().map_err(|_| StoryContinuityError::SequenceOverflow)?;
                if first.sequence.get() < expected.get() {
                    return Err(StoryContinuityError::Overlap);
                }
                if first.sequence.get() > expected.get() {
                    return Err(StoryContinuityError::Gap);
                }
            }
        } else if let Some(first) = recent_segments.first() {
            if first.sequence.get() != 1 {
                return Err(StoryContinuityError::Gap);
            }
        }

        for window in recent_segments.windows(2) {
            let prev = window[0].sequence.get();
            let next = window[1].sequence.get();
            if next <= prev {
                return Err(StoryContinuityError::OutOfOrder);
            }
            if next != prev + 1 {
                return Err(StoryContinuityError::Gap);
            }
        }

        let mut token_sum = 0u64;
        for segment in &recent_segments {
            let tokens = crate::domain::text::estimate_text_tokens(segment.text.as_str());
            token_sum = token_sum.checked_add(tokens).ok_or(StoryContinuityError::LimitExceeded {
                limit: "max_recent_segment_tokens",
            })?;
        }
        if token_sum > limits.max_recent_segment_tokens {
            return Err(StoryContinuityError::LimitExceeded {
                limit: "max_recent_segment_tokens",
            });
        }

        Ok(Self {
            summary,
            recent_segments,
        })
    }

    pub fn summary(&self) -> &StorySummary {
        &self.summary
    }

    pub fn recent_segments(&self) -> &[StorySegment] {
        &self.recent_segments
    }

    pub fn latest_sequence(&self) -> Option<StorySequence> {
        self.recent_segments
            .last()
            .map(|segment| segment.sequence)
            .or(self.summary.summarized_through)
    }

    pub fn next_sequence(&self) -> Result<StorySequence, StoryContinuityError> {
        match self.latest_sequence() {
            Some(latest) => latest.next(),
            None => StorySequence::try_new(1),
        }
    }

    pub fn estimate_tokens(&self) -> u64 {
        let mut total = 0u64;
        if !self.summary.text.as_str().trim().is_empty() {
            total = total.saturating_add(crate::domain::text::estimate_text_tokens(self.summary.text.as_str()));
        }
        for segment in &self.recent_segments {
            total = total.saturating_add(crate::domain::text::estimate_text_tokens(segment.text.as_str()));
        }
        total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryTurn {
    pub id: TurnId,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryEvent {
    pub id: EventId,
    pub turn_id: TurnId,
    pub seq: u32,
    pub kind: EventKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Dialogue,
    Action,
    WorldChange,
    Chapter,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Dialogue => "dialogue",
            EventKind::Action => "action",
            EventKind::WorldChange => "world_change",
            EventKind::Chapter => "chapter",
        }
    }
}

#[cfg(test)]
#[path = "tests/narrative_tests.rs"]
mod tests;
