use super::*;
use crate::core::{BoundedText, CharacterProfile, DialogueExample, PlayerRoleContext, RetrievalHint};
use std::collections::BTreeMap;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn role(role_id: &str, name: &str, personality: &str) -> ResolvedRole {
    ResolvedRole {
        role_id: RoleId::try_new(role_id).expect("valid role ID"),
        name: text(name),
        aliases: vec![text(&format!("{name} alias"))],
        backstory: text(&format!("{name} backstory")),
        retrieval_hint: RetrievalHint::try_new(name).expect("valid retrieval hint"),
        profile: CharacterProfile {
            appearance: Some(text(&format!("{name} appearance"))),
            personality: Some(text(personality)),
            speaking_style: Some(text(&format!("{name} style"))),
            dialogue_examples: vec![DialogueExample {
                situation: text("Greeting"),
                response: text(&format!("I am {name}.")),
            }],
        },
        seed_memories: BTreeMap::new(),
    }
}

fn player() -> PlayerRoleContext {
    PlayerRoleContext {
        role_id: RoleId::try_new("player").expect("valid role ID"),
        name: text("Player"),
        aliases: vec![text("Player Alias")],
        backstory: text("Player backstory"),
        profile: CharacterProfile {
            appearance: Some(text("Player appearance")),
            personality: Some(text("Player personality")),
            speaking_style: Some(text("Player style")),
            dialogue_examples: vec![DialogueExample {
                situation: text("Greeting"),
                response: text("Hello."),
            }],
        },
    }
}

fn decision(role_id: &str, value: &str) -> CharacterDecision {
    CharacterDecision {
        role_id: RoleId::try_new(role_id).expect("valid role ID"),
        decision: text(value),
        suggested_utterance: None,
    }
}

#[test]
fn role_projection_preserves_effective_role_order() {
    let roles = vec![role("second", "Second", "calm"), role("first", "First", "bold")];

    let context =
        project_role_context(&player(), &roles, &[], &GenerateConfig::default()).expect("projection succeeds");

    assert!(context.roles.find("role_id: \"second\"") < context.roles.find("role_id: \"first\""));
}

#[test]
fn role_projection_uses_effective_card_profile_and_role_body_identity() {
    let roles = vec![role("role_body_id", "Role Body Name", "Card personality")];

    let context =
        project_role_context(&player(), &roles, &[], &GenerateConfig::default()).expect("projection succeeds");

    assert!(context.roles.contains("role_id: \"role_body_id\""));
    assert!(context.roles.contains("name: \"Role Body Name\""));
    assert!(context.roles.contains("profile.personality: \"Card personality\""));
    assert!(!context.roles.contains("card_id"));
    assert!(!context.roles.contains("Card Name"));
}

#[test]
fn player_projection_contains_full_profile_and_backstory() {
    let context = project_role_context(&player(), &[], &[], &GenerateConfig::default()).expect("projection succeeds");

    assert!(context.player_role.contains("role_id: \"player\""));
    assert!(context.player_role.contains("profile.appearance: \"Player appearance\""));
    assert!(context.player_role.contains("profile.personality: \"Player personality\""));
    assert!(context.player_role.contains("profile.speaking_style: \"Player style\""));
    assert!(context.player_role.contains("\"situation\":\"Greeting\""));
    assert!(context.player_role.contains("backstory: \"Player backstory\""));
}

#[test]
fn player_role_is_rejected_from_ai_roles() {
    let error = project_role_context(&player(), &[role("player", "Player", "calm")], &[], &GenerateConfig::default())
        .expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_player_role_in_ai_roles",
            ..
        }
    ));
}

#[test]
fn role_projection_excludes_private_memories() {
    let mut first = role("first", "First", "calm");
    first.seed_memories.insert(
        crate::core::MemoryId::try_new("first_memory").expect("valid memory ID"),
        crate::core::MemoryBody {
            content: text("First secret"),
            activation: crate::core::ActivationRule::default(),
            retrieval: RetrievalHint::try_new("First secret").expect("valid retrieval hint"),
        },
    );
    let mut second = role("second", "Second", "bold");
    second.seed_memories.insert(
        crate::core::MemoryId::try_new("second_memory").expect("valid memory ID"),
        crate::core::MemoryBody {
            content: text("Second secret"),
            activation: crate::core::ActivationRule::default(),
            retrieval: RetrievalHint::try_new("Second secret").expect("valid retrieval hint"),
        },
    );

    let context = project_role_context(&player(), &[first, second], &[], &GenerateConfig::default())
        .expect("projection succeeds");

    assert!(!context.roles.contains("First secret"));
    assert!(!context.roles.contains("Second secret"));
}

#[test]
fn role_projection_rejects_missing_decision_role() {
    let roles = vec![role("present", "Present", "calm")];
    let decisions = vec![decision("missing", "Wait.")];

    let error =
        project_role_context(&player(), &roles, &decisions, &GenerateConfig::default()).expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_character_decision_role_missing",
            ..
        }
    ));
}

#[test]
fn role_projection_rejects_duplicate_decision_role() {
    let roles = vec![role("present", "Present", "calm")];
    let decisions = vec![decision("present", "Wait."), decision("present", "Speak.")];

    let error =
        project_role_context(&player(), &roles, &decisions, &GenerateConfig::default()).expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_duplicate_character_decision",
            ..
        }
    ));
}

#[test]
fn role_projection_rejects_player_decision() {
    let decisions = vec![decision("player", "Choose.")];

    let error =
        project_role_context(&player(), &[], &decisions, &GenerateConfig::default()).expect_err("validation fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_player_character_decision_forbidden",
            ..
        }
    ));
}

#[test]
fn role_projection_rejects_aggregate_overflow() {
    let config = GenerateConfig {
        max_role_context_bytes: 1,
        ..GenerateConfig::default()
    };

    let error =
        project_role_context(&player(), &[role("role", "Role", "calm")], &[], &config).expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_role_context_too_large",
            ..
        }
    ));
}

#[test]
fn role_projection_rejects_mandatory_role_overflow() {
    let config = GenerateConfig {
        max_role_bytes_per_role: 1,
        ..GenerateConfig::default()
    };

    let error =
        project_role_context(&player(), &[role("role", "Role", "calm")], &[], &config).expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_role_projection_too_large",
            ..
        }
    ));
}

#[test]
fn role_projection_rejects_decision_overflow() {
    let config = GenerateConfig {
        max_decision_context_bytes: 1,
        ..GenerateConfig::default()
    };

    let error = project_role_context(
        &player(),
        &[role("role", "Role", "calm")],
        &[decision("role", "Wait.")],
        &config,
    )
    .expect_err("projection fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Generate,
            code: "generate_decision_context_too_large",
            ..
        }
    ));
}

#[test]
fn role_projection_truncates_backstory_on_utf8_boundary() {
    let mut selected = role("role", "Role", "calm");
    selected.backstory = text("白蛇传中的角色拥有一段很长的往事");
    let full = project_role_context(&player(), std::slice::from_ref(&selected), &[], &GenerateConfig::default())
        .expect("full projection succeeds");
    let config = GenerateConfig {
        max_role_bytes_per_role: full.roles.len() - 5,
        ..GenerateConfig::default()
    };

    let context = project_role_context(&player(), &[selected], &[], &config).expect("truncated projection succeeds");

    assert!(context.roles.contains("[truncated]"));
    assert!(context.roles.len() <= config.max_role_bytes_per_role);
}
