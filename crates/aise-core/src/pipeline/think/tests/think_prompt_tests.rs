use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    BoundedText, CharacterProfile, CharacterThinkRequest, EntryOrigin, MemoryBody, MemoryId, PlayerContribution,
    ResolvedRole, RetrievalHint, RoleId, RumorBody, RumorEntry, RumorId, StoryContext, StorySummary, TurnNumber,
};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use std::collections::BTreeMap;
use std::path::Path;

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn role(id: &str, marker: &str) -> ResolvedRole {
    ResolvedRole {
        role_id: RoleId::try_new(id).expect("valid role id"),
        name: text(&format!("{marker} name")),
        aliases: vec![text(&format!("{marker} alias"))],
        backstory: text(&format!("{marker} backstory")),
        retrieval_hint: RetrievalHint::try_new(format!("{marker} hint")).expect("valid hint"),
        profile: CharacterProfile {
            appearance: Some(text(&format!("{marker} appearance"))),
            personality: Some(text(&format!("{marker} personality"))),
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        seed_memories: BTreeMap::from([(
            MemoryId::try_new(format!("{marker}-memory")).expect("valid memory id"),
            MemoryBody {
                content: text(&format!("{marker} memory content")),
                activation: Default::default(),
                retrieval: RetrievalHint::try_new(format!("{marker} memory hint")).expect("valid memory hint"),
            },
        )]),
    }
}

fn rumor(id: &str, content: &str, hint: &str) -> RumorEntry {
    RumorEntry {
        id: RumorId::try_new(id).expect("valid rumor id"),
        origin: EntryOrigin::Seed,
        body: RumorBody {
            content: text(content),
            activation: Default::default(),
            retrieval: RetrievalHint::try_new(hint).expect("valid retrieval hint"),
        },
    }
}

fn prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt should load")
}

fn trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "think-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn rendered_text(rendered: &RenderedPrompt) -> String {
    rendered
        .messages()
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\r\n", "\n")
}

#[tokio::test]
async fn prompt_contains_only_target_role_private_context() {
    let store = store_with_default_pack().await;
    let story_ctx = StoryContext::new();
    let roles = [role("target", "TARGET_PRIVATE"), role("other", "OTHER_PRIVATE")];
    let requests = [CharacterThinkRequest {
        role_id: roles[0].role_id.clone(),
        thinking_focus: text("focus marker"),
    }];
    let player_contribution = PlayerContribution {
        raw: "raw player input".to_owned(),
        processed: "processed player input".to_owned(),
    };
    let activated_rumors = [rumor(
        "activated-rumor",
        "Activated rumor content.",
        "activated rumor hint",
    )];
    let retrieved_rumors = [rumor(
        "retrieved-rumor",
        "Retrieved rumor content.",
        "retrieved rumor hint",
    )];
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        activated_rumors: &activated_rumors,
        retrieved_rumors: &retrieved_rumors,
    };
    let trace = trace();

    let rendered = character_decision(
        &prompt(),
        &store,
        CharacterDecisionPrompt {
            story_ctx: &story_ctx,
            input: &input,
            request: &requests[0],
            role: &roles[0],
            config: &ThinkConfig::default(),
        },
        trace.root(),
    )
    .await
    .expect("prompt should render");
    let text = rendered_text(&rendered);

    assert!(text.contains("TARGET_PRIVATE backstory"));
    assert!(text.contains("TARGET_PRIVATE personality"));
    assert!(text.contains("TARGET_PRIVATE memory content"));
    assert!(!text.contains("TARGET_PRIVATE-memory"));
    assert!(!text.contains("TARGET_PRIVATE memory hint"));
    assert!(!text.contains("OTHER_PRIVATE"));
    assert!(!text.contains("FORBIDDEN_STORY_GOAL"));
    assert!(text.contains("Activated rumor content."));
    assert!(text.contains("Retrieved rumor content."));
    assert!(!text.contains("activated-rumor"));
    assert!(!text.contains("retrieved-rumor"));
    assert!(!text.contains("activated rumor hint"));
    assert!(!text.contains("retrieved rumor hint"));
    assert!(!text.contains("Narrative Direction"));
}

#[tokio::test]
async fn prompt_renders_required_runtime_sections_in_order() {
    let store = store_with_default_pack().await;
    let story_ctx = StoryContext::new();
    let roles = [role("target", "TARGET")];
    let requests = [CharacterThinkRequest {
        role_id: roles[0].role_id.clone(),
        thinking_focus: text("focus"),
    }];
    let player_contribution = PlayerContribution {
        raw: "raw".to_owned(),
        processed: "processed".to_owned(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        activated_rumors: &[],
        retrieved_rumors: &[],
    };
    let trace = trace();

    let rendered = character_decision(
        &prompt(),
        &store,
        CharacterDecisionPrompt {
            story_ctx: &story_ctx,
            input: &input,
            request: &requests[0],
            role: &roles[0],
            config: &ThinkConfig::default(),
        },
        trace.root(),
    )
    .await
    .expect("prompt should render");
    let text = rendered_text(&rendered);
    let sections = [
        "## Story Opening",
        "## Recent Story",
        "## Pending Player Contribution",
        "## Rumors",
        "## Target Role ID",
        "## Target Name and Aliases",
        "## Target Profile",
        "## Target Backstory",
        "## Target Seed Memories",
        "## Thinking Focus",
    ];
    let positions = sections
        .iter()
        .map(|section| text.find(section).expect("section should exist"))
        .collect::<Vec<_>>();

    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!text.contains("## Story Summary"));
    assert!(!text.contains("## Story Summary or Story Opening"));
    assert!(text.contains(r#"Aliases: ["TARGET alias"]"#));
    assert!(text.contains(r#"{"appearance":"TARGET appearance""#));
}

#[tokio::test]
async fn prompt_labels_existing_summary_as_story_summary() {
    let store = store_with_default_pack().await;
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(StorySummary {
        text: "Earlier events.".to_owned(),
        covered_through: TurnNumber::new(1),
    });
    let roles = [role("target", "TARGET")];
    let requests = [CharacterThinkRequest {
        role_id: roles[0].role_id.clone(),
        thinking_focus: text("focus"),
    }];
    let player_contribution = PlayerContribution {
        raw: "raw".to_owned(),
        processed: "processed".to_owned(),
    };
    let input = ThinkInput {
        requests: &requests,
        resolved_roles: &roles,
        player_contribution: &player_contribution,
        activated_rumors: &[],
        retrieved_rumors: &[],
    };
    let trace = trace();

    let rendered = character_decision(
        &prompt(),
        &store,
        CharacterDecisionPrompt {
            story_ctx: &story_ctx,
            input: &input,
            request: &requests[0],
            role: &roles[0],
            config: &ThinkConfig::default(),
        },
        trace.root(),
    )
    .await
    .expect("prompt should render");
    let text = rendered_text(&rendered);

    assert!(text.contains("## Story Summary\n\nEarlier events."));
    assert!(!text.contains("## Story Opening"));
    assert!(!text.contains("## Story Summary or Story Opening"));
}

#[tokio::test]
async fn oversized_profile_is_rejected_before_rendering() {
    let store = store_with_default_pack().await;
    let role = role("target", "TARGET");
    let story_ctx = StoryContext::new();
    let request = CharacterThinkRequest {
        role_id: role.role_id.clone(),
        thinking_focus: text("focus"),
    };
    let player_contribution = PlayerContribution {
        raw: String::new(),
        processed: String::new(),
    };
    let input = ThinkInput {
        requests: std::slice::from_ref(&request),
        resolved_roles: std::slice::from_ref(&role),
        player_contribution: &player_contribution,
        activated_rumors: &[],
        retrieved_rumors: &[],
    };
    let trace = trace();
    let config = ThinkConfig {
        max_profile_bytes: 1,
        ..ThinkConfig::default()
    };

    let error = character_decision(
        &prompt(),
        &store,
        CharacterDecisionPrompt {
            story_ctx: &story_ctx,
            input: &input,
            request: &request,
            role: &role,
            config: &config,
        },
        trace.root(),
    )
    .await
    .expect_err("oversized profile should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            code: "think_profile_too_long",
            ..
        }
    ));
}

#[tokio::test]
async fn oversized_backstory_is_rejected_before_rendering() {
    let store = store_with_default_pack().await;
    let role = role("target", "TARGET");
    let story_ctx = StoryContext::new();
    let request = CharacterThinkRequest {
        role_id: role.role_id.clone(),
        thinking_focus: text("focus"),
    };
    let player_contribution = PlayerContribution {
        raw: String::new(),
        processed: String::new(),
    };
    let input = ThinkInput {
        requests: std::slice::from_ref(&request),
        resolved_roles: std::slice::from_ref(&role),
        player_contribution: &player_contribution,
        activated_rumors: &[],
        retrieved_rumors: &[],
    };
    let trace = trace();
    let config = ThinkConfig {
        max_backstory_bytes: 1,
        ..ThinkConfig::default()
    };

    let error = character_decision(
        &prompt(),
        &store,
        CharacterDecisionPrompt {
            story_ctx: &story_ctx,
            input: &input,
            request: &request,
            role: &role,
            config: &config,
        },
        trace.root(),
    )
    .await
    .expect_err("oversized backstory should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            code: "think_backstory_too_long",
            ..
        }
    ));
}
