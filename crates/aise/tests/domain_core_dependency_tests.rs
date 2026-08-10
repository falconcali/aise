use aise::domain::error::DomainInputError;
use aise::domain::ids::{ConstraintId, StoryId, StoryRevision, TurnId};
use aise::turn::turn_contract::{
    IdempotencyKey, MAX_IDEMPOTENCY_KEY_CHARS, MAX_PLAYER_INPUT_CHARS, TurnIdentity, TurnRequest, TurnRequestError,
};
use aise::turn::turn_trace::{TraceId, TraceIdError};

#[test]
fn story_id_rejects_empty_and_blank() {
    let empty = StoryId::try_new("").unwrap_err();
    assert_eq!(empty, DomainInputError::EmptyStoryId);
    assert_eq!(empty.to_string(), "story_id must not be empty");
    let blank = StoryId::try_new("   ").unwrap_err();
    assert_eq!(blank, DomainInputError::EmptyStoryId);
    assert_eq!(blank.to_string(), "story_id must not be empty");
}

#[test]
fn turn_id_rejects_empty_and_blank() {
    let empty = TurnId::try_new("").unwrap_err();
    assert_eq!(empty, DomainInputError::EmptyTurnId);
    assert_eq!(empty.to_string(), "turn_id must not be empty");
    let blank = TurnId::try_new("   ").unwrap_err();
    assert_eq!(blank, DomainInputError::EmptyTurnId);
    assert_eq!(blank.to_string(), "turn_id must not be empty");
}

#[test]
fn constraint_id_rejects_empty_and_blank() {
    let empty = ConstraintId::try_new("").unwrap_err();
    assert_eq!(empty, DomainInputError::EmptyConstraintId);
    assert_eq!(empty.to_string(), "constraint_id must not be empty");
    let blank = ConstraintId::try_new("   ").unwrap_err();
    assert_eq!(blank, DomainInputError::EmptyConstraintId);
    assert_eq!(blank.to_string(), "constraint_id must not be empty");
}

#[test]
fn domain_ids_preserve_string_serde_shape() {
    let story_id = StoryId::try_new("story-1").unwrap();
    let turn_id = TurnId::try_new("turn-1").unwrap();
    let constraint_id = ConstraintId::try_new("c-1").unwrap();
    assert_eq!(serde_json::to_string(&story_id).unwrap(), "\"story-1\"");
    assert_eq!(serde_json::to_string(&turn_id).unwrap(), "\"turn-1\"");
    assert_eq!(serde_json::to_string(&constraint_id).unwrap(), "\"c-1\"");
    let story_round_trip: StoryId = serde_json::from_str("\"story-1\"").unwrap();
    let turn_round_trip: TurnId = serde_json::from_str("\"turn-1\"").unwrap();
    let constraint_round_trip: ConstraintId = serde_json::from_str("\"c-1\"").unwrap();
    assert_eq!(story_round_trip, story_id);
    assert_eq!(turn_round_trip, turn_id);
    assert_eq!(constraint_round_trip, constraint_id);
    assert!(serde_json::from_str::<StoryId>("\"\"").is_err());
    assert!(serde_json::from_str::<TurnId>("\"   \"").is_err());
    assert!(serde_json::from_str::<ConstraintId>("\"\"").is_err());
}

#[test]
fn story_revision_preserves_integer_serde_shape() {
    let revision = StoryRevision::new(42);
    assert_eq!(revision.get(), 42);
    assert_eq!(revision.to_string(), "42");
    let copied = revision;
    assert_eq!(copied, revision);
    assert_eq!(serde_json::to_string(&revision).unwrap(), "42");
    let round_trip: StoryRevision = serde_json::from_str("42").unwrap();
    assert_eq!(round_trip, revision);
}

#[test]
fn active_story_constraint_uses_shared_constraint_id() {
    let id = ConstraintId::try_new("shared-constraint").unwrap();
    let constraint = aise::domain::story_instance::constraint::ActiveStoryConstraint {
        id: id.clone(),
        source: aise::domain::story_instance::constraint::StoryConstraintSource {
            pack_id: aise::domain::asset::ids::PackId::from("pack-1"),
            constraint_key: aise::domain::asset::ids::ConstraintKey::from("c1"),
        },
        scope: aise::domain::asset::constraint::StoryConstraintScope::Story,
        requirement: aise::domain::asset::constraint::StoryConstraintRequirement::Require {
            statement: aise::domain::asset::validation::BoundedText::try_new("stay consistent", "constraint", 128)
                .unwrap(),
        },
        lifecycle: aise::domain::asset::constraint::StoryConstraintLifecycle::Persistent,
    };
    assert_eq!(constraint.id, id);
}

#[test]
fn trace_id_rejects_empty_and_blank_with_trace_error() {
    let empty = TraceId::try_new("").unwrap_err();
    assert_eq!(empty, TraceIdError::EmptyTraceId);
    assert_eq!(empty.to_string(), "trace_id must not be empty");
    let blank = TraceId::try_new("   ").unwrap_err();
    assert_eq!(blank, TraceIdError::EmptyTraceId);
    assert_eq!(blank.to_string(), "trace_id must not be empty");
}

#[test]
fn turn_request_errors_are_request_scoped() {
    let key_error = IdempotencyKey::try_new("").unwrap_err();
    assert_eq!(key_error, TurnRequestError::EmptyIdempotencyKey);
    assert_eq!(key_error.to_string(), "idempotency key must not be empty");
    let long_key = "x".repeat(MAX_IDEMPOTENCY_KEY_CHARS + 1);
    let key_error = IdempotencyKey::try_new(long_key).unwrap_err();
    assert_eq!(
        key_error,
        TurnRequestError::IdempotencyKeyTooLong {
            actual: MAX_IDEMPOTENCY_KEY_CHARS + 1,
            maximum: MAX_IDEMPOTENCY_KEY_CHARS,
        }
    );
    let input_error = TurnRequest::try_new(String::new()).unwrap_err();
    assert_eq!(input_error, TurnRequestError::EmptyPlayerInput);
    assert_eq!(input_error.to_string(), "player input must not be empty");
    let long_input = "x".repeat(MAX_PLAYER_INPUT_CHARS + 1);
    let input_error = TurnRequest::try_new(long_input).unwrap_err();
    assert_eq!(
        input_error,
        TurnRequestError::PlayerInputTooLong {
            actual: MAX_PLAYER_INPUT_CHARS + 1,
            maximum: MAX_PLAYER_INPUT_CHARS,
        }
    );
}

#[test]
fn turn_identity_constructor_is_infallible() {
    let identity: TurnIdentity = TurnIdentity::new(
        StoryId::try_new("story-1").unwrap(),
        TurnId::try_new("turn-1").unwrap(),
        IdempotencyKey::try_new("key-1".to_string()).unwrap(),
        1000,
    );
    assert_eq!(identity.story_id().as_str(), "story-1");
    assert_eq!(identity.turn_id().as_str(), "turn-1");
}
