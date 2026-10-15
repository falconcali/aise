use super::*;
use crate::core::tests::store_with_default_pack;
use crate::core::{
    ActivatedKnowledge, ActivationRule, BoundedText, CharacterDecision, CharacterProfile, EntryOrigin, FactBody,
    FactEntry, FactId, IdempotencyKey, NarrativeDirection, NarrativeNodeId, PlayerContribution, PlayerRoleContext,
    ResolvedRole, RetrievalHint, RoleId, RumorBody, RumorEntry, RumorId, StorySummary, Turn, TurnEvaluation,
    TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::pipeline::GenerateConfig;
use crate::pipeline::generate::generate_roles::project_role_context;
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use std::collections::BTreeMap;
use std::path::Path;

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

fn bundled_prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads")
}

fn test_trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "generate-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

struct GenerateFixture {
    story_goal: String,
    player_contribution: PlayerContribution,
    directions: Vec<NarrativeDirection>,
    constraints: Vec<BoundedText>,
    knowledge: ActivatedKnowledge,
    retrieved_facts: Vec<FactEntry>,
    retrieved_rumors: Vec<RumorEntry>,
    player_role: PlayerRoleContext,
    roles: Vec<ResolvedRole>,
    character_decisions: Vec<CharacterDecision>,
}

impl GenerateFixture {
    fn input(&self) -> GenerateInput<'_> {
        GenerateInput {
            story_goal: &self.story_goal,
            player_contribution: &self.player_contribution,
            directions: &self.directions,
            constraints: &self.constraints,
            knowledge: &self.knowledge,
            retrieved_facts: &self.retrieved_facts,
            retrieved_rumors: &self.retrieved_rumors,
            player_role: &self.player_role,
            roles: &self.roles,
            character_decisions: &self.character_decisions,
        }
    }
}

fn generate_fixture(goal: &str, processed: &str) -> GenerateFixture {
    GenerateFixture {
        story_goal: goal.to_owned(),
        player_contribution: PlayerContribution {
            raw: "raw".to_owned(),
            processed: processed.to_owned(),
        },
        directions: Vec::new(),
        constraints: Vec::new(),
        knowledge: ActivatedKnowledge::default(),
        retrieved_facts: Vec::new(),
        retrieved_rumors: Vec::new(),
        player_role: player_role(),
        roles: Vec::new(),
        character_decisions: Vec::new(),
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn player_role() -> PlayerRoleContext {
    PlayerRoleContext {
        role_id: RoleId::try_new("player").expect("valid role ID"),
        name: text("Player"),
        aliases: vec![text("Player Alias")],
        backstory: text("Player backstory"),
        profile: CharacterProfile {
            appearance: Some(text("Player appearance")),
            personality: Some(text("Player personality")),
            speaking_style: Some(text("Player style")),
            dialogue_examples: Vec::new(),
        },
    }
}

fn role(role_id: &str, name: &str) -> ResolvedRole {
    ResolvedRole {
        role_id: RoleId::try_new(role_id).expect("valid role ID"),
        name: text(name),
        aliases: Vec::new(),
        backstory: text(&format!("{name} private backstory")),
        retrieval_hint: RetrievalHint::try_new(name).expect("valid retrieval hint"),
        profile: CharacterProfile {
            appearance: None,
            personality: Some(text("measured")),
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        seed_memories: BTreeMap::new(),
    }
}

fn decision(role_id: &str, value: &str) -> CharacterDecision {
    CharacterDecision {
        role_id: RoleId::try_new(role_id).expect("valid role ID"),
        decision: text(value),
        suggested_utterance: Some(text(&format!("Say {value}"))),
    }
}

fn role_context(fixture: &GenerateFixture) -> GenerateRoleContext {
    project_role_context(
        &fixture.player_role,
        &fixture.roles,
        &fixture.character_decisions,
        &GenerateConfig::default(),
    )
    .expect("role context projects")
}

fn player_section() -> String {
    let fixture = generate_fixture("Goal.", "I wave.");
    format!("# Player Character\n\n{}", role_context(&fixture).player_role)
}

fn knowledge() -> ActivatedKnowledge {
    ActivatedKnowledge {
        facts: vec![FactEntry {
            id: FactId::try_new("keeper_name").expect("test fact id should be valid"),
            origin: EntryOrigin::Seed,
            body: FactBody {
                content: text("The forest keeper is named Ash."),
                activation: ActivationRule::default(),
                retrieval: RetrievalHint::try_new("keeper").expect("test hint should be valid"),
            },
        }],
        rumors: vec![RumorEntry {
            id: RumorId::try_new("mill_ghost").expect("test rumor id should be valid"),
            origin: EntryOrigin::Seed,
            body: RumorBody {
                content: text("A ghost haunts the old mill."),
                activation: ActivationRule::default(),
                retrieval: RetrievalHint::try_new("mill").expect("test hint should be valid"),
            },
        }],
    }
}

fn retrieved_fact() -> FactEntry {
    FactEntry {
        id: FactId::try_new("river_direction").expect("test fact id should be valid"),
        origin: EntryOrigin::Seed,
        body: FactBody {
            content: text("The river runs north."),
            activation: ActivationRule::default(),
            retrieval: RetrievalHint::try_new("river").expect("test hint should be valid"),
        },
    }
}

fn retrieved_rumor() -> RumorEntry {
    RumorEntry {
        id: RumorId::try_new("bell_rumor").expect("test rumor id should be valid"),
        origin: EntryOrigin::Seed,
        body: RumorBody {
            content: text("The chapel bell rings by itself."),
            activation: ActivationRule::default(),
            retrieval: RetrievalHint::try_new("bell").expect("test hint should be valid"),
        },
    }
}

fn summary(text: &str) -> StorySummary {
    StorySummary {
        text: text.to_owned(),
        covered_through: TurnNumber::new(1),
    }
}

fn accepted_turn(number: u64, segment: &str) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("key-{number}")).expect("valid idempotency key"),
        player_contribution: PlayerContribution {
            raw: format!("raw-{number}"),
            processed: format!("processed-{number}"),
        },
        turn_segment: TurnSegment::new(segment.to_owned()),
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: TurnStatus::Accepted,
    }
}

fn var<'a>(vars: &'a PromptVars, name: &str) -> &'a str {
    vars.get(name).and_then(Value::as_str).expect("string var")
}

async fn runtime_context(story_ctx: &StoryContext, fixture: &GenerateFixture) -> String {
    let store = store_with_default_pack().await;
    let trace = test_trace();
    let role_context = role_context(fixture);
    let rendered = generate_story(
        &bundled_prompt(),
        story_ctx,
        &fixture.input(),
        &role_context,
        &store,
        trace.root(),
    )
    .await
    .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

async fn runtime_messages(story_ctx: &StoryContext, fixture: &GenerateFixture) -> Vec<String> {
    let store = store_with_default_pack().await;
    let trace = test_trace();
    let role_context = role_context(fixture);
    generate_story(
        &bundled_prompt(),
        story_ctx,
        &fixture.input(),
        &role_context,
        &store,
        trace.root(),
    )
    .await
    .expect("render")
    .into_messages()
    .into_iter()
    .map(|message| message.content.replace("\r\n", "\n"))
    .collect()
}

#[test]
fn vars_for_new_story_carry_opening_and_empty_summary() {
    let story_ctx = StoryContext::new();
    let fixture = generate_fixture("Goal.", "I wave.");
    let vars = generate_vars(&story_ctx, &fixture.input(), &role_context(&fixture), "Rain falls.".to_owned());

    assert_eq!(var(&vars, "story_opening"), "Rain falls.");
    assert_eq!(var(&vars, "story_summary"), "");
    assert_eq!(var(&vars, "recent_story"), "");
    assert_eq!(var(&vars, "story_goal"), "Goal.");
    assert_eq!(var(&vars, "player_contribution"), "I wave.");
}

#[test]
fn vars_with_summary_and_recent_story_carry_both() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  "));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.recent_turns.push_back(accepted_turn(3, "The hall is dark."));
    let fixture = generate_fixture("Goal.", "I wave.");
    let vars = generate_vars(&story_ctx, &fixture.input(), &role_context(&fixture), String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "recent_story"), "The door creaks.\n\nThe hall is dark.");
}

#[tokio::test]
async fn new_story_context_has_opening_then_goal_and_contribution() {
    let story_ctx = StoryContext::new();
    let store = store_with_default_pack().await;
    let opening = store
        .pack_store()
        .get_pack(&story_ctx.pack_ref)
        .await
        .expect("pack loads")
        .opening
        .clone();

    let content = runtime_context(&story_ctx, &generate_fixture("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Opening\n\n{opening}\n\n{}\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[tokio::test]
async fn recent_story_without_summary_keeps_opening_before_recent_story() {
    let mut story_ctx = StoryContext::new();
    story_ctx.recent_turns.push_back(accepted_turn(1, "The door creaks."));
    let store = store_with_default_pack().await;
    let opening = store
        .pack_store()
        .get_pack(&story_ctx.pack_ref)
        .await
        .expect("pack loads")
        .opening
        .clone();

    let content = runtime_context(&story_ctx, &generate_fixture("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Opening\n\n{opening}\n\n# Recent Story\n\nThe door creaks.\n\n{}\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[tokio::test]
async fn summarized_story_context_orders_summary_recent_story_goal_and_contribution() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));

    let content = runtime_context(&story_ctx, &generate_fixture("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Summary\n\nEarlier events.\n\n# Recent Story\n\nThe door creaks.\n\n{}\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[tokio::test]
async fn summary_without_recent_story_omits_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));

    let content = runtime_context(&story_ctx, &generate_fixture("Goal.", "I wave.")).await;

    assert_eq!(
        content,
        format!(
            "# Story Summary\n\nEarlier events.\n\n{}\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[tokio::test]
async fn story_constraints_precede_goal_and_contribution() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    let mut fixture = generate_fixture("Goal.", "I wave.");
    fixture.constraints = vec![text("The ritual truth is revealed.")];

    let content = runtime_context(&story_ctx, &fixture).await;

    assert_eq!(
        content,
        format!(
            "# Story Summary\n\nEarlier events.\n\n{}\n\n# Story Constraints\n\n- The ritual truth is revealed.\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[test]
fn vars_without_narrative_or_knowledge_carry_empty_values() {
    let fixture = generate_fixture("Goal.", "I wave.");
    let vars = generate_vars(&StoryContext::new(), &fixture.input(), &role_context(&fixture), String::new());

    assert_eq!(var(&vars, "known_facts"), "");
    assert_eq!(var(&vars, "rumors"), "");
    assert_eq!(var(&vars, "narrative_directions"), "");
    assert_eq!(var(&vars, "story_constraints"), "");
}

#[test]
fn vars_carry_knowledge_content_only() {
    let fixture = GenerateFixture {
        knowledge: knowledge(),
        ..generate_fixture("Goal.", "I wave.")
    };
    let vars = generate_vars(&StoryContext::new(), &fixture.input(), &role_context(&fixture), String::new());

    assert_eq!(var(&vars, "known_facts"), "- The forest keeper is named Ash.");
    assert_eq!(var(&vars, "rumors"), "- A ghost haunts the old mill.");
}

#[test]
fn retrieved_knowledge_follows_activated_knowledge() {
    let fixture = GenerateFixture {
        knowledge: knowledge(),
        retrieved_facts: vec![retrieved_fact()],
        retrieved_rumors: vec![retrieved_rumor()],
        ..generate_fixture("Goal.", "I wave.")
    };
    let vars = generate_vars(&StoryContext::new(), &fixture.input(), &role_context(&fixture), String::new());

    assert_eq!(
        var(&vars, "known_facts"),
        "- The forest keeper is named Ash.\n- The river runs north."
    );
    assert_eq!(
        var(&vars, "rumors"),
        "- A ghost haunts the old mill.\n- The chapel bell rings by itself."
    );
}

#[test]
fn retrieved_content_remains_in_its_knowledge_section() {
    let fixture = GenerateFixture {
        retrieved_facts: vec![retrieved_fact()],
        retrieved_rumors: vec![retrieved_rumor()],
        ..generate_fixture("Goal.", "I wave.")
    };
    let vars = generate_vars(&StoryContext::new(), &fixture.input(), &role_context(&fixture), String::new());

    assert!(var(&vars, "known_facts").contains("The river runs north."));
    assert!(!var(&vars, "known_facts").contains("chapel bell"));
    assert!(var(&vars, "rumors").contains("The chapel bell rings by itself."));
    assert!(!var(&vars, "rumors").contains("river runs north"));
}

#[tokio::test]
async fn knowledge_and_direction_follow_story_and_precede_constraints_goal_and_contribution() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    let fixture = GenerateFixture {
        directions: vec![NarrativeDirection {
            node_id: NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
            dramatic_focus: text("The traveler meets the forest keeper."),
        }],
        constraints: vec![text("The ritual truth is revealed.")],
        knowledge: knowledge(),
        ..generate_fixture("Goal.", "I wave.")
    };

    let content = runtime_context(&story_ctx, &fixture).await;

    assert_eq!(
        content,
        format!(
            "# Story Summary\n\nEarlier events.\n\n# Known Facts\n\n- The forest keeper is named Ash.\n\n# Rumors\n\n- A ghost haunts the old mill.\n\n{}\n\n# Narrative Direction\n\n- The traveler meets the forest keeper.\n\n# Story Constraints\n\n- The ritual truth is revealed.\n\n# Immediate Story Goal\n\nGoal.\n\n# Pending Player Contribution\n\nI wave.",
            player_section()
        )
    );
}

#[tokio::test]
async fn generate_prompt_contains_roles_and_ordered_decisions() {
    let fixture = GenerateFixture {
        roles: vec![role("first", "First"), role("second", "Second")],
        character_decisions: vec![decision("second", "Observe."), decision("first", "Answer.")],
        ..generate_fixture("Goal.", "I wave.")
    };

    let content = runtime_context(&StoryContext::new(), &fixture).await;
    let player = content.find("# Player Character").expect("player section");
    let roles = content.find("# Relevant AI Roles").expect("roles section");
    let decisions = content.find("# Character Decisions").expect("decisions section");
    let direction = content.find("# Immediate Story Goal").expect("goal section");
    let second_decision = content.find("decision: \"Observe.\"").expect("second decision");
    let first_decision = content.find("decision: \"Answer.\"").expect("first decision");

    assert!(player < roles);
    assert!(roles < decisions);
    assert!(decisions < direction);
    assert!(second_decision < first_decision);
    assert!(content.contains("role_id: \"first\""));
    assert!(content.contains("role_id: \"second\""));
}

#[tokio::test]
async fn generate_prompt_marks_decisions_as_viewpoint_not_fact() {
    let messages = runtime_messages(
        &StoryContext::new(),
        &GenerateFixture {
            roles: vec![role("first", "First")],
            character_decisions: vec![decision("first", "Wait.")],
            ..generate_fixture("Goal.", "I wave.")
        },
    )
    .await;
    let instructions = format!("{}\n{}", messages[0], messages[2]);

    assert!(instructions.contains("current viewpoint and immediate intent, not a world fact"));
    assert!(instructions.contains("optional guidance, not mandatory verbatim dialogue"));
    assert!(instructions.contains("private backstory is not automatically known by another role"));
    assert!(instructions.contains("unless a visible causal event"));
}

#[tokio::test]
async fn generate_prompt_preserves_player_agency() {
    let messages = runtime_messages(&StoryContext::new(), &generate_fixture("Goal.", "I wave.")).await;
    let instructions = format!("{}\n{}", messages[0], messages[2]);

    assert!(instructions.contains("player role never appears in Relevant AI Roles or Character Decisions"));
    assert!(instructions.contains("Do not make an unprovided consequential choice for the player"));
    assert!(
        instructions
            .contains("never override established Known Facts, the Pending Player Contribution, or Story Constraints")
    );
}
