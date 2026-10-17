use super::*;
use crate::core::tests::story_context;
use crate::core::{ActivatedKnowledge, BoundedText, PlayerContribution};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

#[test]
fn projection_preserves_validation_evidence_boundaries() {
    let goal = text("Reach the gate.");
    let contribution = PlayerContribution {
        raw: "I walk.".to_owned(),
        processed: "The player walks.".to_owned(),
    };
    let knowledge = ActivatedKnowledge::default();
    let mut story_ctx = story_context();
    story_ctx
        .roles
        .get_mut(&story_ctx.player)
        .expect("player role exists")
        .spec
        .backstory = text("Private backstory.");
    let context = ValidationContext {
        story_goal: &goal,
        player_contribution: &contribution,
        directions: &[],
        constraints: &[],
        knowledge: &knowledge,
        retrieved_facts: &[],
        retrieved_rumors: &[],
        role_ids: &[],
        character_decisions: &[],
    };

    let evidence = project_validation_evidence(PipelineStage::Repair, &story_ctx, &context).expect("evidence projects");

    assert_eq!(evidence.story_goal, "Reach the gate.");
    assert_eq!(evidence.player_contribution, "The player walks.");
    assert!(!evidence.story_opening.is_empty());
    assert!(evidence.player_role.contains("Private backstory."));
    assert!(evidence.activated_facts.is_empty());
    assert!(evidence.retrieved_facts.is_empty());
    assert!(evidence.activated_rumors.is_empty());
    assert!(evidence.retrieved_rumors.is_empty());
    assert!(evidence.resolved_roles.is_empty());
    assert!(evidence.character_decisions.is_empty());
    assert!(!evidence.player_role.contains("清明时节西湖骤雨"));
}
