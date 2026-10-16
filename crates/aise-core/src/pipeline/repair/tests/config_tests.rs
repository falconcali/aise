use super::*;

fn assert_invalid(config: RepairConfig) {
    assert!(matches!(
        config.validate(),
        Err(PipelineError::InvalidConfig {
            stage: PipelineStage::Repair,
            code: "repair_max_proposal_bytes_invalid",
            ..
        })
    ));
}

#[test]
fn default_proposal_limit_is_sixty_four_kibibytes() {
    let config = RepairConfig::default();

    assert_eq!(config.max_proposal_bytes, 64 * 1024);
    config.validate().expect("default repair config should be valid");
}

#[test]
fn proposal_limit_accepts_inclusive_boundaries() {
    RepairConfig { max_proposal_bytes: 1 }
        .validate()
        .expect("minimum proposal limit should be valid");
    RepairConfig {
        max_proposal_bytes: MAX_REPAIR_PROPOSAL_BYTES_LIMIT,
    }
    .validate()
    .expect("maximum proposal limit should be valid");
}

#[test]
fn proposal_limit_rejects_values_outside_bounds() {
    assert_invalid(RepairConfig { max_proposal_bytes: 0 });
    assert_invalid(RepairConfig {
        max_proposal_bytes: MAX_REPAIR_PROPOSAL_BYTES_LIMIT + 1,
    });
}

#[test]
fn config_deserialization_rejects_unknown_fields() {
    let result = serde_json::from_value::<RepairConfig>(serde_json::json!({
        "max_proposal_bytes": 1,
        "unknown": true
    }));

    assert!(result.is_err());
}
