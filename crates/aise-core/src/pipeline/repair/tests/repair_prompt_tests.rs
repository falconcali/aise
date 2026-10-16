use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterProfile, PlayerContribution, PlayerRoleContext, RoleId, StoryContext,
};
use crate::pipeline::common::{ValidationContext, ValidationCriterionKind, ValidationIssue};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use std::collections::HashSet;
use std::path::Path;

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

struct RepairFixture {
    goal: BoundedText,
    contribution: PlayerContribution,
    knowledge: ActivatedKnowledge,
    player_role: PlayerRoleContext,
    issues: Vec<ValidationIssue>,
}

impl RepairFixture {
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
            proposal: "Current proposal.",
            proposal_version: 2,
            issues: &self.issues,
            repair_round: 1,
            repair_rounds_remaining: 1,
        }
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn fixture() -> RepairFixture {
    RepairFixture {
        goal: text("Reach the gate."),
        contribution: PlayerContribution {
            raw: "I follow.".to_owned(),
            processed: "The player follows.".to_owned(),
        },
        knowledge: ActivatedKnowledge::default(),
        player_role: PlayerRoleContext {
            role_id: RoleId::try_new("player").expect("valid role id"),
            name: text("Player"),
            aliases: Vec::new(),
            backstory: text("Backstory."),
            profile: CharacterProfile {
                appearance: None,
                personality: None,
                speaking_style: None,
                dialogue_examples: Vec::new(),
            },
        },
        issues: vec![
            ValidationIssue {
                kind: ValidationCriterionKind::Check,
                name: "explicit_constraints".to_owned(),
                message: text("Remove the conflict."),
            },
            ValidationIssue {
                kind: ValidationCriterionKind::Score,
                name: "narrative_quality".to_owned(),
                message: text("Clarify the ending."),
            },
        ],
    }
}

fn evidence() -> ValidationPromptEvidence {
    ValidationPromptEvidence {
        story_summary: "summary".to_owned(),
        story_opening: "opening".to_owned(),
        recent_story: "recent".to_owned(),
        story_goal: "goal".to_owned(),
        player_contribution: "contribution".to_owned(),
        narrative_directions: "directions".to_owned(),
        story_constraints: "constraints".to_owned(),
        activated_facts: "activated facts".to_owned(),
        activated_rumors: "activated rumors".to_owned(),
        retrieved_facts: "retrieved facts".to_owned(),
        retrieved_rumors: "retrieved rumors".to_owned(),
        player_role: "player role".to_owned(),
        resolved_roles: "resolved roles".to_owned(),
        character_decisions: "character decisions".to_owned(),
    }
}

fn prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads")
}

fn trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::MetadataOnly,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "repair-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

#[test]
fn repair_vars_contain_exact_contract_names_and_issue_order() {
    let fixture = fixture();
    let input = fixture.input();
    let vars = repair_vars(&input, &RepairConfig::default(), evidence()).expect("vars build");
    let actual = vars.keys().map(String::as_str).collect::<HashSet<_>>();
    let expected = [
        "story_summary",
        "story_opening",
        "recent_story",
        "story_goal",
        "player_contribution",
        "narrative_directions",
        "story_constraints",
        "activated_facts",
        "activated_rumors",
        "retrieved_facts",
        "retrieved_rumors",
        "player_role",
        "resolved_roles",
        "character_decisions",
        "current_proposal",
        "proposal_version",
        "repair_issues",
        "max_proposal_bytes",
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let issues = vars["repair_issues"].as_str().expect("issues string");

    assert_eq!(actual, expected);
    assert!(
        issues.find("explicit_constraints").expect("check issue")
            < issues.find("narrative_quality").expect("score issue")
    );
}

#[tokio::test]
async fn rendered_prompt_labels_all_runtime_sections_untrusted() {
    let fixture = fixture();
    let story_ctx = StoryContext::new();
    let store = store_with_default_pack().await;
    let trace = trace();
    let messages = repair_story(
        &prompt(),
        &story_ctx,
        &fixture.input(),
        &RepairConfig::default(),
        &store,
        trace.root(),
    )
    .await
    .expect("prompt renders")
    .into_messages();
    let instructions = format!("{}\n{}", messages[0].content, messages[2].content);
    let runtime = &messages[1].content;

    assert!(instructions.contains("one complete replacement story body"));
    assert!(instructions.contains("no JSON envelope, Markdown fence, analysis"));
    assert!(runtime.contains("# [UNTRUSTED CONTENT] Current Proposal"));
    assert!(runtime.contains("# [UNTRUSTED CONTENT] Repair Issues"));
    assert!(runtime.contains("# [UNTRUSTED CONTENT] Character Decisions"));
}
