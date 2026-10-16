use super::*;
use crate::core::{ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId};
use crate::pipeline::common::{PipelineStage, ValidationContext, ValidationCriterionKind, ValidationIssue};
use crate::trace::AttributeValue;

struct TraceFixture {
    goal: BoundedText,
    contribution: PlayerContribution,
    knowledge: ActivatedKnowledge,
    player_role: PlayerRoleContext,
    issues: Vec<ValidationIssue>,
}

impl TraceFixture {
    fn input(&self) -> RepairInput<'_> {
        RepairInput {
            context: ValidationContext {
                story_goal: &self.goal,
                player_contribution: &self.contribution,
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
            issues: &self.issues,
            repair_round: 2,
            repair_rounds_remaining: 1,
        }
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn fixture() -> TraceFixture {
    TraceFixture {
        goal: text("goal"),
        contribution: PlayerContribution {
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
        issues: vec![ValidationIssue {
            kind: ValidationCriterionKind::Check,
            name: "logic".to_owned(),
            message: text("secret issue"),
        }],
    }
}

#[test]
fn success_metadata_contains_versions_rounds_issue_count_and_output_bytes() {
    let fixture = fixture();
    let metadata = repair_metadata(&fixture.input(), 17);

    assert_eq!(
        metadata,
        vec![
            Attribute::u64(INPUT_PROPOSAL_VERSION, 4),
            Attribute::u64(OUTPUT_PROPOSAL_VERSION, 5),
            Attribute::u64(ROUND, 2),
            Attribute::u64(ROUNDS_REMAINING, 1),
            Attribute::u64(ISSUES, 1),
            Attribute::u64(OUTPUT_BYTES, 17),
        ]
    );
}

#[test]
fn metadata_excludes_proposal_and_issue_content() {
    let fixture = fixture();
    let metadata = repair_metadata(&fixture.input(), 17);
    let strings = metadata
        .iter()
        .filter_map(|attribute| match &attribute.value {
            AttributeValue::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(!strings.contains(&"secret proposal"));
    assert!(!strings.contains(&"secret issue"));
}

#[test]
fn invalid_output_error_preserves_repair_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Repair, "repair_proposal_too_long", "too long");

    let error = observation_error(&error);

    assert_eq!(error.code, "repair_proposal_too_long");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("repair"));
}
