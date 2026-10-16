use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId, StoryContext,
};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

#[tokio::test]
async fn projection_preserves_validation_evidence_boundaries() {
    let goal = text("Reach the gate.");
    let contribution = PlayerContribution {
        raw: "I walk.".to_owned(),
        processed: "The player walks.".to_owned(),
    };
    let knowledge = ActivatedKnowledge::default();
    let player_role = PlayerRoleContext {
        role_id: RoleId::try_new("player").expect("valid role"),
        name: text("Player"),
        aliases: Vec::new(),
        backstory: text("Private backstory."),
        profile: CharacterProfile {
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
    };
    let context = ValidationContext {
        story_goal: &goal,
        player_contribution: &contribution,
        directions: &[],
        constraints: &[],
        knowledge: &knowledge,
        retrieved_facts: &[],
        retrieved_rumors: &[],
        player_role: &player_role,
        roles: &[],
        character_decisions: &[],
    };
    let story_ctx = StoryContext::new();
    let store = store_with_default_pack().await;

    let evidence = project_validation_evidence(PipelineStage::Repair, &story_ctx, &context, &store)
        .await
        .expect("evidence projects");

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
}
