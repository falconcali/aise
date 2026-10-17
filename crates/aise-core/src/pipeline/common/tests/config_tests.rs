use super::*;

fn assert_invalid_config(error: PipelineError, stage: PipelineStage, expected_code: &'static str) {
    assert!(matches!(
        error,
        PipelineError::InvalidConfig {
            stage: actual_stage,
            code,
            ..
        } if actual_stage == stage && code == expected_code
    ));
}

#[test]
fn validation_loop_default_uses_two_repair_rounds() {
    assert_eq!(ValidationLoopConfig::default().max_repair_rounds, 2);
}

#[test]
fn validation_loop_accepts_zero_and_maximum_budget() {
    ValidationLoopConfig { max_repair_rounds: 0 }
        .validate()
        .expect("zero repair rounds should be valid");
    ValidationLoopConfig {
        max_repair_rounds: MAX_REPAIR_ROUNDS_LIMIT,
    }
    .validate()
    .expect("maximum repair rounds should be valid");
}

#[test]
fn validation_loop_rejects_excessive_budget() {
    let error = ValidationLoopConfig {
        max_repair_rounds: MAX_REPAIR_ROUNDS_LIMIT + 1,
    }
    .validate()
    .expect_err("excessive repair rounds should fail");

    assert_invalid_config(error, PipelineStage::Validate, "validation_max_repair_rounds_exceeded");
}

#[test]
fn pipeline_default_aggregates_validation_defaults() {
    let config = PipelineConfig::default();

    assert_eq!(config.validate.checks.len(), 4);
    assert_eq!(config.validate.scores.len(), 4);
    assert_eq!(config.validate.max_issue_bytes, 2048);
    assert_eq!(config.repair.max_proposal_bytes, 64 * 1024);
    assert_eq!(config.validation_loop.max_repair_rounds, 2);
    assert_eq!(config.extract.max_memory_concurrency.get(), 4);
    config.validate().expect("default pipeline config should be valid");
}

#[test]
fn pipeline_validation_propagates_stage_config_errors() {
    let mut config = PipelineConfig::default();
    config.validate.checks.clear();
    config.validate.scores.clear();
    let error = config.validate().expect_err("empty criteria should fail");

    assert_invalid_config(error, PipelineStage::Validate, "validate_criteria_empty");
}
