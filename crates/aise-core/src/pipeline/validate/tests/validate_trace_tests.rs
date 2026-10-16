use super::*;
use crate::core::{ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId};
use crate::pipeline::common::{PipelineStage, ValidationCheckResult, ValidationContext, ValidationScoreResult};
use crate::trace::AttributeValue;

struct TraceFixture {
    goal: BoundedText,
    player_contribution: PlayerContribution,
    knowledge: ActivatedKnowledge,
    player_role: PlayerRoleContext,
}

impl TraceFixture {
    fn input(&self) -> ValidateInput<'_> {
        ValidateInput {
            context: ValidationContext {
                story_goal: &self.goal,
                player_contribution: &self.player_contribution,
                directions: &[],
                constraints: &[],
                knowledge: &self.knowledge,
                retrieved_facts: &[],
                retrieved_rumors: &[],
                player_role: &self.player_role,
                roles: &[],
                character_decisions: &[],
            },
            proposal: "secret proposal",
            proposal_version: 4,
            repair_rounds_used: 1,
            repair_rounds_remaining: 2,
        }
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn fixture() -> TraceFixture {
    TraceFixture {
        goal: text("goal"),
        player_contribution: PlayerContribution {
            raw: "raw".to_owned(),
            processed: "processed".to_owned(),
        },
        knowledge: ActivatedKnowledge::default(),
        player_role: PlayerRoleContext {
            role_id: RoleId::try_new("player").expect("valid role"),
            name: text("Player"),
            aliases: Vec::new(),
            backstory: text("backstory"),
            profile: CharacterProfile {
                appearance: None,
                personality: None,
                speaking_style: None,
                dialogue_examples: Vec::new(),
            },
        },
    }
}

fn report() -> ValidationReport {
    ValidationReport {
        proposal_version: 4,
        checks: vec![
            ValidationCheckResult {
                name: "logic".to_owned(),
                description: "logic description".to_owned(),
                passed: true,
                issue: None,
            },
            ValidationCheckResult {
                name: "constraints".to_owned(),
                description: "constraint description".to_owned(),
                passed: false,
                issue: Some(text("secret issue")),
            },
        ],
        scores: vec![
            ValidationScoreResult {
                name: "goal".to_owned(),
                description: "goal description".to_owned(),
                min_score: 0.0,
                max_score: 10.0,
                target_score: 8.0,
                score: 8.0,
                issue: None,
            },
            ValidationScoreResult {
                name: "quality".to_owned(),
                description: "quality description".to_owned(),
                min_score: 0.0,
                max_score: 10.0,
                target_score: 8.0,
                score: 7.0,
                issue: Some(text("another secret issue")),
            },
        ],
    }
}

#[test]
fn success_metadata_contains_static_counts_and_rounds() {
    let fixture = fixture();
    let metadata = validation_metadata(&fixture.input(), &report());

    assert_eq!(
        &metadata[..9],
        [
            Attribute::u64(PROPOSAL_VERSION, 4),
            Attribute::u64(CHECKS_TOTAL, 2),
            Attribute::u64(CHECKS_PASSED, 1),
            Attribute::u64(CHECKS_FAILED, 1),
            Attribute::u64(SCORES_TOTAL, 2),
            Attribute::u64(SCORES_MET, 1),
            Attribute::u64(SCORES_UNMET, 1),
            Attribute::u64(REPAIR_ROUNDS_USED, 1),
            Attribute::u64(REPAIR_ROUNDS_REMAINING, 2),
        ]
    );
}

#[test]
fn success_metadata_contains_indexed_criterion_values() {
    let fixture = fixture();
    let metadata = validation_metadata(&fixture.input(), &report());

    assert!(metadata.contains(&Attribute::string(CHECK_NAMES[0], "logic")));
    assert!(metadata.contains(&Attribute::bool(CHECK_PASSED[0], true)));
    assert!(metadata.contains(&Attribute::string(CHECK_NAMES[1], "constraints")));
    assert!(metadata.contains(&Attribute::bool(CHECK_PASSED[1], false)));
    assert!(metadata.contains(&Attribute::string(SCORE_NAMES[0], "goal")));
    assert!(metadata.contains(&Attribute::f64(SCORE_VALUES[0], 8.0)));
    assert!(metadata.contains(&Attribute::f64(SCORE_TARGETS[0], 8.0)));
    assert!(metadata.contains(&Attribute::string(SCORE_NAMES[1], "quality")));
    assert!(metadata.contains(&Attribute::f64(SCORE_VALUES[1], 7.0)));
    assert!(metadata.contains(&Attribute::f64(SCORE_TARGETS[1], 8.0)));
}

#[test]
fn metadata_excludes_proposal_descriptions_and_issue_text() {
    let fixture = fixture();
    let metadata = validation_metadata(&fixture.input(), &report());
    let strings = metadata
        .iter()
        .filter_map(|attribute| match &attribute.value {
            AttributeValue::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(!strings.contains(&"secret proposal"));
    assert!(!strings.contains(&"logic description"));
    assert!(!strings.contains(&"secret issue"));
    assert!(!strings.contains(&"another secret issue"));
}

#[test]
fn invalid_output_error_preserves_validate_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Validate, "validate_issue_missing", "missing issue");

    let error = observation_error(&error);

    assert_eq!(error.code, "validate_issue_missing");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("validate"));
}
