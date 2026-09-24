use super::*;
use crate::config::{
    ActivationConfig, NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig,
};
use crate::domain::ids::TurnKey;
use crate::domain::ids::{StoryId, TurnNumber};
use crate::turn::observability::{ContentCaptureLimits, ContentCapturePolicy};
use crate::turn::turn_budget::TurnBudget;
use crate::turn::turn_contract::{IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest};
use std::time::{Duration, Instant};

fn context() -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
        &ActivationConfig::default(),
    )
    .unwrap();
    let identity = TurnIdentity::new(
        TurnKey::new(StoryId::try_new("story-1").unwrap(), TurnNumber::try_new(1).unwrap()),
        IdempotencyKey::try_new("idem-1").unwrap(),
        0,
    );
    let request = TurnRequest::try_new("go north".to_owned()).unwrap();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control).unwrap();
    ctx.set_observation_encoder(BoundedContentEncoder::new(
        ContentCapturePolicy::RedactedContent,
        ContentCaptureLimits {
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        },
    ));
    ctx
}

#[test]
fn load_story_snapshot_input_comes_from_turn_context() {
    let ctx = context();
    let encoder = ctx.observation_encoder();

    let (content, encoding_failed) = encode_input(
        encoder,
        ObservationStep::LoadStorySnapshot,
        &ctx,
        encoder.unwrap().max_observation_bytes(),
    );

    let value: serde_json::Value = serde_json::from_str(&content.unwrap().json).unwrap();
    assert!(!encoding_failed);
    assert_eq!(value["story_id"], "story-1");
    assert_eq!(value["turn_number"], 1);
}

#[test]
fn activate_world_info_input_includes_player_contribution() {
    let ctx = context();
    let encoder = ctx.observation_encoder();

    let (content, encoding_failed) = encode_input(
        encoder,
        ObservationStep::ActivateWorldInfo,
        &ctx,
        encoder.unwrap().max_observation_bytes(),
    );

    let value: serde_json::Value = serde_json::from_str(&content.unwrap().json).unwrap();
    assert!(!encoding_failed);
    assert_eq!(value["story_id"], "story-1");
    assert_eq!(value["turn_number"], 1);
    assert_eq!(value["player_contribution"], "go north");
}

#[test]
fn load_story_snapshot_failure_produces_output() {
    let ctx = context();
    let encoder = ctx.observation_encoder();
    let outcome: Result<StoryReadSnapshot, StoreError> = Err(StoreError::Unavailable);

    let finish = <StoryReadSnapshot as BaselineObservationOutcome>::finish(
        &ctx,
        &outcome,
        encoder,
        encoder.unwrap().max_observation_bytes(),
    );

    let value: serde_json::Value = serde_json::from_str(&finish.output.unwrap().json).unwrap();
    assert_eq!(value["status"], "error");
    assert_eq!(value["error_code"], "story_snapshot_load_failed");
    assert_eq!(value["failure_kind"], "store");
    assert_eq!(value["stage"], "baseline_ctx_builder");
}
