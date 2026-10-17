use super::*;
use crate::core::tests::{snake_pack, story_context, turn_evaluation};
use crate::core::{
    ActivatedKnowledge, ActivationRule, BoundedText, EntryOrigin, FactBody, FactEntry, FactId, IdempotencyKey,
    KnowledgeChange, MemoryEntry, NarrativeDirection, NarrativeNodeId, PlayerContribution, RetrievalHint, RoleEntry,
    RoleId, RumorBody, RumorEntry, RumorId, StoryPack, StorySummary, Turn, TurnNumber, TurnSegment, TurnStatus,
};
use crate::pipeline::plan::plan_candidates::collect_candidates;
use crate::pipeline::plan::plan_roles::collect_role_catalog;
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
        name: "plan-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn contribution(processed: &str) -> PlayerContribution {
    PlayerContribution {
        raw: "raw".to_owned(),
        processed: processed.to_owned(),
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
        player_contribution: contribution(&format!("processed-{number}")),
        turn_segment: TurnSegment::new(segment.to_owned()),
        knowledge_change: KnowledgeChange::default(),
        turn_evaluation: turn_evaluation(),
        turn_status: TurnStatus::Pass,
    }
}

fn var<'a>(vars: &'a PromptVars, name: &str) -> &'a str {
    vars.get(name).and_then(Value::as_str).expect("string var")
}

struct PlanFixture {
    player_contribution: PlayerContribution,
    directions: Vec<NarrativeDirection>,
    constraints: Vec<BoundedText>,
    knowledge: ActivatedKnowledge,
}

impl PlanFixture {
    fn input(&self) -> PlanInput<'_> {
        PlanInput {
            player_contribution: &self.player_contribution,
            directions: &self.directions,
            constraints: &self.constraints,
            knowledge: &self.knowledge,
        }
    }
}

fn plan_fixture(processed: &str) -> PlanFixture {
    PlanFixture {
        player_contribution: contribution(processed),
        directions: Vec::new(),
        constraints: Vec::new(),
        knowledge: ActivatedKnowledge::default(),
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn seed_context(pack: &StoryPack, story_ctx: &StoryContext) -> StoryContext {
    let mut ctx = story_ctx.clone();
    ctx.facts = pack
        .seed_facts
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                FactEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    ctx.rumors = pack
        .seed_rumors
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RumorEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    ctx.roles = pack
        .roles
        .iter()
        .map(|(id, body)| {
            let memories = body
                .seed_memories
                .iter()
                .map(|(mem_id, mem_body)| {
                    (
                        mem_id.clone(),
                        MemoryEntry {
                            id: mem_id.clone(),
                            origin: EntryOrigin::Seed,
                            body: mem_body.clone(),
                        },
                    )
                })
                .collect();
            (
                id.clone(),
                RoleEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: body.spec.clone(),
                    profile: body.default_profile.clone(),
                    memories,
                },
            )
        })
        .collect();
    ctx
}

fn render_runtime_context(story_ctx: &StoryContext, fixture: &PlanFixture) -> String {
    without_role_context(render_pack_context(story_ctx, fixture, &snake_pack(), 0))
}

fn without_role_context(content: String) -> String {
    let content = without_section(content, "## Player Character");
    without_section(content, "## AI Role Catalog")
}

fn without_section(content: String, heading: &str) -> String {
    let marker = format!("\n\n{heading}\n\n");
    let Some(start) = content.find(&marker) else {
        return content;
    };
    let catalog_start = start + marker.len();
    let Some(end) = content[catalog_start..].find("\n\n## ") else {
        return content;
    };
    format!("{}{}", &content[..start], &content[catalog_start + end..])
}

fn render_pack_context(
    story_ctx: &StoryContext,
    fixture: &PlanFixture,
    story_pack: &StoryPack,
    candidate_limit: usize,
) -> String {
    let seeded_ctx = seed_context(story_pack, story_ctx);
    let trace = test_trace();
    let candidates = collect_candidates(&seeded_ctx, &fixture.knowledge, candidate_limit);
    let player_role = &seeded_ctx.roles[&seeded_ctx.player];
    let role_catalog = collect_role_catalog(&seeded_ctx, &player_role.id);
    let rendered = process_plan(
        &bundled_prompt(),
        PlanPromptContext {
            story_ctx: &seeded_ctx,
            input: &fixture.input(),
            candidates: &candidates,
            player_role,
            role_catalog: &role_catalog,
            config: &PlanConfig::default(),
        },
        trace.root(),
    )
    .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

fn runtime_context(story_ctx: &StoryContext, processed: &str) -> String {
    render_runtime_context(story_ctx, &plan_fixture(processed))
}

fn vars(story_ctx: &StoryContext, fixture: &PlanFixture, story_opening: String) -> PromptVars {
    let pack = snake_pack();
    let seeded_ctx = seed_context(&pack, story_ctx);
    let candidates = collect_candidates(&seeded_ctx, &fixture.knowledge, 0);
    let player_role = &seeded_ctx.roles[&seeded_ctx.player];
    let role_catalog = collect_role_catalog(&seeded_ctx, &player_role.id);
    let player_role = player_role_text(player_role, &PlanConfig::default()).expect("player role renders");
    let role_catalog = role_catalog_text(&role_catalog).expect("catalog renders");
    plan_vars(
        &seeded_ctx,
        &fixture.input(),
        story_opening,
        &candidates,
        player_role,
        role_catalog,
        PlanConfig::default().max_story_goal_bytes,
    )
    .expect("vars render")
}

#[test]
fn vars_for_new_story_carry_opening_and_empty_summary_and_recent_story() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let vars = vars(&story_ctx, &fixture, "Rain falls.".to_owned());

    assert_eq!(var(&vars, "player_input"), "I wave.");
    assert_eq!(var(&vars, "story_opening"), "Rain falls.");
    assert_eq!(var(&vars, "story_summary"), "");
    assert_eq!(var(&vars, "recent_story"), "");
    assert_eq!(var(&vars, "max_story_goal_bytes"), "512");
}

#[test]
fn vars_with_summary_and_recent_story_carry_both() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("  Earlier events.  "));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.recent_turns.push_back(accepted_turn(3, "The hall is dark."));
    let fixture = plan_fixture("I wave.");
    let vars = vars(&story_ctx, &fixture, String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "recent_story"), "The door creaks.\n\nThe hall is dark.");
}

#[test]
fn new_story_context_has_opening_then_input() {
    let story_ctx = story_context();
    let opening = story_ctx.opening.clone();

    let content = runtime_context(&story_ctx, "I wave.");

    assert_eq!(
        content,
        format!("## Story Opening\n\n{opening}\n\n## Pending Player Input\n\nI wave.")
    );
}

#[test]
fn recent_story_without_summary_keeps_opening_before_recent_story() {
    let mut story_ctx = story_context();
    story_ctx.recent_turns.push_back(accepted_turn(1, "The door creaks."));
    let opening = story_ctx.opening.clone();

    let content = runtime_context(&story_ctx, "I wave.");

    assert_eq!(
        content,
        format!(
            "## Story Opening\n\n{opening}\n\n## Recent Story\n\nThe door creaks.\n\n## Pending Player Input\n\nI wave."
        )
    );
}

#[test]
fn summarized_story_context_orders_summary_recent_story_and_input() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("Earlier events."));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));

    let content = runtime_context(&story_ctx, "I wave.");

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Recent Story\n\nThe door creaks.\n\n## Pending Player Input\n\nI wave."
    );
}

#[test]
fn summary_without_recent_story_omits_opening() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("Earlier events."));

    let content = runtime_context(&story_ctx, "I wave.");

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Pending Player Input\n\nI wave."
    );
}

#[test]
fn vars_without_narrative_carry_empty_direction_and_constraints() {
    let fixture = plan_fixture("I wave.");
    let vars = vars(&story_context(), &fixture, String::new());

    assert_eq!(var(&vars, "narrative_directions"), "");
    assert_eq!(var(&vars, "story_constraints"), "");
}

#[test]
fn narrative_direction_and_constraints_precede_player_input() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("Earlier events."));
    let fixture = PlanFixture {
        player_contribution: contribution("I wave."),
        directions: vec![NarrativeDirection {
            node_id: NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
            dramatic_focus: text("The traveler meets the forest keeper."),
        }],
        constraints: vec![text("The ritual truth is revealed.")],
        knowledge: ActivatedKnowledge::default(),
    };

    let content = render_runtime_context(&story_ctx, &fixture);

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Narrative Direction\n\n- The traveler meets the forest keeper.\n\n## Story Constraints\n\n- The ritual truth is revealed.\n\n## Pending Player Input\n\nI wave."
    );
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

fn candidate_pack(fact_ids: &[&str], rumor_ids: &[&str]) -> StoryPack {
    let mut pack = snake_pack();
    let fact_body = pack.seed_facts.values().next().expect("fixture fact").clone();
    let rumor_body = pack.seed_rumors.values().next().expect("fixture rumor").clone();
    pack.seed_facts = fact_ids
        .iter()
        .map(|id| {
            let mut body = fact_body.clone();
            body.content = text(&format!("hidden fact body {id}"));
            body.retrieval = RetrievalHint::try_new(format!("fact hint {id}")).expect("valid hint");
            (FactId::try_new(*id).expect("valid fact id"), body)
        })
        .collect::<BTreeMap<_, _>>();
    pack.seed_rumors = rumor_ids
        .iter()
        .map(|id| {
            let mut body = rumor_body.clone();
            body.content = text(&format!("hidden rumor body {id}"));
            body.retrieval = RetrievalHint::try_new(format!("rumor hint {id}")).expect("valid hint");
            (RumorId::try_new(*id).expect("valid rumor id"), body)
        })
        .collect::<BTreeMap<_, _>>();
    pack
}

#[test]
fn vars_carry_knowledge_content_only() {
    let fixture = PlanFixture {
        knowledge: knowledge(),
        ..plan_fixture("I wave.")
    };
    let vars = vars(&story_context(), &fixture, String::new());

    assert_eq!(var(&vars, "known_facts"), "- The forest keeper is named Ash.");
    assert_eq!(var(&vars, "rumors"), "- A ghost haunts the old mill.");
}

#[test]
fn knowledge_follows_story_and_precedes_narrative_direction() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("Earlier events."));
    let fixture = PlanFixture {
        player_contribution: contribution("I wave."),
        directions: vec![NarrativeDirection {
            node_id: NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
            dramatic_focus: text("The traveler meets the forest keeper."),
        }],
        constraints: Vec::new(),
        knowledge: knowledge(),
    };

    let content = render_runtime_context(&story_ctx, &fixture);

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Known Facts\n\n- The forest keeper is named Ash.\n\n## Rumors\n\n- A ghost haunts the old mill.\n\n## Narrative Direction\n\n- The traveler meets the forest keeper.\n\n## Pending Player Input\n\nI wave."
    );
}

#[test]
fn candidate_sections_include_ids_and_hints_without_bodies() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&["fact-a"], &["rumor-a"]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(content.contains("## Retrievable Facts\n\n- id: \"fact-a\"\n  retrieval_hint: \"fact hint fact-a\""));
    assert!(content.contains("## Retrievable Rumors\n\n- id: \"rumor-a\"\n  retrieval_hint: \"rumor hint rumor-a\""));
    assert!(!content.contains("hidden fact body"));
    assert!(!content.contains("hidden rumor body"));
}

#[test]
fn empty_candidate_kind_omits_its_section() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&["fact-a"], &[]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(content.contains("## Retrievable Facts"));
    assert!(!content.contains("## Retrievable Rumors"));
}

#[test]
fn empty_candidate_lists_omit_both_sections() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&[], &[]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(!content.contains("## Retrievable Facts"));
    assert!(!content.contains("## Retrievable Rumors"));
}

#[test]
fn candidate_ids_and_hints_are_json_escaped() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let mut pack = candidate_pack(&["fact-\"quoted\""], &[]);
    pack.seed_facts.values_mut().next().expect("fixture fact").retrieval =
        RetrievalHint::try_new("line \"one\"\nline two").expect("valid hint");

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(content.contains("- id: \"fact-\\\"quoted\\\"\""));
    assert!(content.contains("retrieval_hint: \"line \\\"one\\\"\\nline two\""));
}

#[test]
fn ai_role_catalog_contains_lightweight_fields_without_private_role_data() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let mut pack = candidate_pack(&[], &[]);
    let role = pack.roles.values_mut().next().expect("fixture role");
    role.spec.name = text("Catalog Name");
    role.spec.alias = vec![text("Alias \"One\""), text("Alias Two")];
    role.spec.retrieval = RetrievalHint::try_new("Catalog\nHint").expect("valid hint");
    role.spec.backstory = text("PRIVATE_BACKSTORY");
    role.default_profile.personality = Some(text("PRIVATE_PROFILE"));
    role.seed_memories.values_mut().next().expect("fixture memory").content = text("PRIVATE_MEMORY");

    let content = render_pack_context(&story_ctx, &fixture, &pack, 0);

    assert!(content.contains("## AI Role Catalog"));
    assert!(content.contains("name: \"Catalog Name\""));
    assert!(content.contains("aliases: [\"Alias \\\"One\\\"\", \"Alias Two\"]"));
    assert!(content.contains("retrieval_hint: \"Catalog\\nHint\""));
    assert!(!content.contains("PRIVATE_BACKSTORY"));
    assert!(!content.contains("PRIVATE_PROFILE"));
    assert!(!content.contains("PRIVATE_MEMORY"));
}

#[test]
fn player_context_contains_planning_fields_without_voice_memory_or_retrieval_hint() {
    let story_ctx = story_context();
    let fixture = plan_fixture("I wave.");
    let mut pack = candidate_pack(&[], &[]);
    let player_id = RoleId::try_new("xu_xian").expect("valid role id");
    let player = pack.roles.get_mut(&player_id).expect("fixture player role");
    player.spec.name = text("Player Name");
    player.spec.alias = vec![text("Player Alias")];
    player.spec.backstory = text("PLAYER_BACKSTORY");
    player.default_profile.appearance = Some(text("PLAYER_APPEARANCE"));
    player.default_profile.personality = Some(text("PLAYER_PERSONALITY"));
    player.default_profile.speaking_style = Some(text("PLAYER_SPEAKING_STYLE"));
    player.default_profile.dialogue_examples[0].response = text("PLAYER_DIALOGUE_EXAMPLE");
    player.spec.retrieval = RetrievalHint::try_new("PLAYER_RETRIEVAL_HINT").expect("valid hint");
    player.seed_memories.values_mut().next().expect("fixture memory").content = text("PLAYER_MEMORY");

    let content = render_pack_context(&story_ctx, &fixture, &pack, 0);
    let player_section = content
        .split("## Player Character")
        .nth(1)
        .expect("player section")
        .split("## AI Role Catalog")
        .next()
        .expect("player section body");

    assert!(player_section.contains("role_id: \"xu_xian\""));
    assert!(player_section.contains("name: \"Player Name\""));
    assert!(player_section.contains("aliases: [\"Player Alias\"]"));
    assert!(player_section.contains("appearance: \"PLAYER_APPEARANCE\""));
    assert!(player_section.contains("personality: \"PLAYER_PERSONALITY\""));
    assert!(player_section.contains("backstory: \"PLAYER_BACKSTORY\""));
    assert!(!player_section.contains("PLAYER_SPEAKING_STYLE"));
    assert!(!player_section.contains("PLAYER_DIALOGUE_EXAMPLE"));
    assert!(!player_section.contains("PLAYER_RETRIEVAL_HINT"));
    assert!(!player_section.contains("PLAYER_MEMORY"));
}

#[test]
fn candidate_sections_follow_known_knowledge_and_precede_narrative_direction() {
    let mut story_ctx = story_context();
    story_ctx.summary = Some(summary("Earlier events."));
    let fixture = PlanFixture {
        player_contribution: contribution("I wave."),
        directions: vec![NarrativeDirection {
            node_id: NarrativeNodeId::try_new("arrival").expect("valid node id"),
            dramatic_focus: text("Meet the keeper."),
        }],
        constraints: Vec::new(),
        knowledge: knowledge(),
    };
    let pack = candidate_pack(&["fact-a"], &["rumor-a"]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);
    let known = content.find("## Known Facts").expect("known facts section");
    let retrievable_facts = content.find("## Retrievable Facts").expect("retrievable facts section");
    let retrievable_rumors = content.find("## Retrievable Rumors").expect("retrievable rumors section");
    let player_role = content.find("## Player Character").expect("player role section");
    let role_catalog = content.find("## AI Role Catalog").expect("role catalog section");
    let narrative = content.find("## Narrative Direction").expect("narrative direction section");

    assert!(known < retrievable_facts);
    assert!(retrievable_facts < retrievable_rumors);
    assert!(player_role < known);
    assert!(retrievable_rumors < role_catalog);
    assert!(role_catalog < narrative);
}

#[test]
fn instructions_enforce_goal_budget_rumor_semantics_and_strict_json_protocol() {
    let pack = candidate_pack(&["fact-a"], &["rumor-a"]);
    let story_ctx = seed_context(&pack, &story_context());
    let fixture = plan_fixture("I wave.");
    let candidates = collect_candidates(&story_ctx, &fixture.knowledge, 8);
    let player_role = &story_ctx.roles[&story_ctx.player];
    let role_catalog = collect_role_catalog(&story_ctx, &player_role.id);
    let trace = test_trace();
    let config = PlanConfig {
        max_story_goal_bytes: 321,
        ..PlanConfig::default()
    };

    let rendered = process_plan(
        &bundled_prompt(),
        PlanPromptContext {
            story_ctx: &story_ctx,
            input: &fixture.input(),
            candidates: &candidates,
            player_role,
            role_catalog: &role_catalog,
            config: &config,
        },
        trace.root(),
    )
    .expect("render");
    let context_instruction = rendered.messages()[0].content.replace("\r\n", "\n");
    let instruction = rendered.messages()[2].content.replace("\r\n", "\n");

    assert!(context_instruction.contains("public claims every character knows are circulating"));
    assert!(instruction.contains("public claims every character knows are circulating"));
    assert!(!context_instruction.contains("characters may have heard"));
    assert!(!instruction.contains("characters may have heard"));
    assert!(instruction.contains("no more than 321 UTF-8 bytes"));
    assert!(instruction.contains("\"story_goal\": \"one concise sentence, non-empty"));
    assert!(instruction.contains("\"facts_to_retrieve\": [\"fact-id\"]"));
    assert!(instruction.contains("\"rumors_to_retrieve\": [\"rumor-id\"]"));
    assert!(instruction.contains("\"roles_for_generation\": [\"role-id\"]"));
    assert!(instruction.contains("\"character_think_requests\": ["));
    assert!(instruction.contains("\"thinking_focus\": \"non-empty string\""));
    assert!(instruction.contains("Every returned RoleId must come from AI Role Catalog"));
    assert!(instruction.contains("Never return its RoleId in roles_for_generation or character_think_requests"));
    assert!(instruction.contains("A Character Think target does not need to be repeated in roles_for_generation"));
    assert!(
        instruction.contains("Do not return role profiles, backstories, memories, decisions, suggested utterances")
    );
}
