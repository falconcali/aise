use super::*;
use crate::core::tests::{snake_pack, store_with_default_pack};
use crate::core::{
    ActivatedKnowledge, ActivationRule, BoundedText, EntryOrigin, FactBody, FactEntry, FactId, IdempotencyKey,
    NarrativeDirection, NarrativeNodeId, PlayerContribution, RetrievalHint, RumorBody, RumorEntry, RumorId, StoryPack,
    StorySummary, Turn, TurnEvaluation, TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::pipeline::plan::plan_candidates::collect_candidates;
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
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: TurnStatus::Accepted,
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

async fn render_runtime_context(story_ctx: &StoryContext, fixture: &PlanFixture) -> String {
    let store = store_with_default_pack().await;
    let story_pack = store.pack_store().get_pack(&story_ctx.pack_ref).await.expect("pack loads");
    render_pack_context(story_ctx, fixture, story_pack.as_ref(), 0)
}

fn render_pack_context(
    story_ctx: &StoryContext,
    fixture: &PlanFixture,
    story_pack: &StoryPack,
    candidate_limit: usize,
) -> String {
    let trace = test_trace();
    let candidates = collect_candidates(story_pack, &fixture.knowledge, candidate_limit);
    let rendered = process_plan(
        &bundled_prompt(),
        story_ctx,
        &fixture.input(),
        story_pack,
        &candidates,
        trace.root(),
    )
    .expect("render");
    rendered.messages()[1].content.replace("\r\n", "\n")
}

async fn runtime_context(story_ctx: &StoryContext, processed: &str) -> String {
    render_runtime_context(story_ctx, &plan_fixture(processed)).await
}

fn vars(story_ctx: &StoryContext, fixture: &PlanFixture, story_opening: String) -> PromptVars {
    let pack = snake_pack();
    let candidates = collect_candidates(&pack, &fixture.knowledge, 0);
    plan_vars(story_ctx, &fixture.input(), story_opening, &candidates)
}

#[test]
fn vars_for_new_story_carry_opening_and_empty_summary_and_recent_story() {
    let story_ctx = StoryContext::new();
    let fixture = plan_fixture("I wave.");
    let vars = vars(&story_ctx, &fixture, "Rain falls.".to_owned());

    assert_eq!(var(&vars, "player_input"), "I wave.");
    assert_eq!(var(&vars, "story_opening"), "Rain falls.");
    assert_eq!(var(&vars, "story_summary"), "");
    assert_eq!(var(&vars, "recent_story"), "");
}

#[test]
fn vars_with_summary_and_recent_story_carry_both() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  "));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));
    story_ctx.recent_turns.push_back(accepted_turn(3, "The hall is dark."));
    let fixture = plan_fixture("I wave.");
    let vars = vars(&story_ctx, &fixture, String::new());

    assert_eq!(var(&vars, "story_summary"), "Earlier events.");
    assert_eq!(var(&vars, "story_opening"), "");
    assert_eq!(var(&vars, "recent_story"), "The door creaks.\n\nThe hall is dark.");
}

#[tokio::test]
async fn new_story_context_has_opening_then_input() {
    let story_ctx = StoryContext::new();
    let store = store_with_default_pack().await;
    let opening = store
        .pack_store()
        .get_pack(&story_ctx.pack_ref)
        .await
        .expect("pack loads")
        .opening
        .clone();

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        format!("## Story Opening\n\n{opening}\n\n## Pending Player Input\n\nI wave.")
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

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        format!(
            "## Story Opening\n\n{opening}\n\n## Recent Story\n\nThe door creaks.\n\n## Pending Player Input\n\nI wave."
        )
    );
}

#[tokio::test]
async fn summarized_story_context_orders_summary_recent_story_and_input() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));
    story_ctx.recent_turns.push_back(accepted_turn(2, "The door creaks."));

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Recent Story\n\nThe door creaks.\n\n## Pending Player Input\n\nI wave."
    );
}

#[tokio::test]
async fn summary_without_recent_story_omits_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events."));

    let content = runtime_context(&story_ctx, "I wave.").await;

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Pending Player Input\n\nI wave."
    );
}

#[test]
fn vars_without_narrative_carry_empty_direction_and_constraints() {
    let fixture = plan_fixture("I wave.");
    let vars = vars(&StoryContext::new(), &fixture, String::new());

    assert_eq!(var(&vars, "narrative_directions"), "");
    assert_eq!(var(&vars, "story_constraints"), "");
}

#[tokio::test]
async fn narrative_direction_and_constraints_precede_player_input() {
    let mut story_ctx = StoryContext::new();
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

    let content = render_runtime_context(&story_ctx, &fixture).await;

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
    let vars = vars(&StoryContext::new(), &fixture, String::new());

    assert_eq!(var(&vars, "known_facts"), "- The forest keeper is named Ash.");
    assert_eq!(var(&vars, "rumors"), "- A ghost haunts the old mill.");
}

#[tokio::test]
async fn knowledge_follows_story_and_precedes_narrative_direction() {
    let mut story_ctx = StoryContext::new();
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

    let content = render_runtime_context(&story_ctx, &fixture).await;

    assert_eq!(
        content,
        "## Story Summary\n\nEarlier events.\n\n## Known Facts\n\n- The forest keeper is named Ash.\n\n## Rumors\n\n- A ghost haunts the old mill.\n\n## Narrative Direction\n\n- The traveler meets the forest keeper.\n\n## Pending Player Input\n\nI wave."
    );
}

#[test]
fn candidate_sections_include_ids_and_hints_without_bodies() {
    let story_ctx = StoryContext::new();
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
    let story_ctx = StoryContext::new();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&["fact-a"], &[]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(content.contains("## Retrievable Facts"));
    assert!(!content.contains("## Retrievable Rumors"));
}

#[test]
fn empty_candidate_lists_omit_both_sections() {
    let story_ctx = StoryContext::new();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&[], &[]);

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(!content.contains("## Retrievable Facts"));
    assert!(!content.contains("## Retrievable Rumors"));
}

#[test]
fn candidate_ids_and_hints_are_json_escaped() {
    let story_ctx = StoryContext::new();
    let fixture = plan_fixture("I wave.");
    let mut pack = candidate_pack(&["fact-\"quoted\""], &[]);
    pack.seed_facts.values_mut().next().expect("fixture fact").retrieval =
        RetrievalHint::try_new("line \"one\"\nline two").expect("valid hint");

    let content = render_pack_context(&story_ctx, &fixture, &pack, 8);

    assert!(content.contains("- id: \"fact-\\\"quoted\\\"\""));
    assert!(content.contains("retrieval_hint: \"line \\\"one\\\"\\nline two\""));
}

#[test]
fn candidate_sections_follow_known_knowledge_and_precede_narrative_direction() {
    let mut story_ctx = StoryContext::new();
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
    let narrative = content.find("## Narrative Direction").expect("narrative direction section");

    assert!(known < retrievable_facts);
    assert!(retrievable_facts < retrievable_rumors);
    assert!(retrievable_rumors < narrative);
}

#[test]
fn final_instruction_requires_strict_json_protocol() {
    let story_ctx = StoryContext::new();
    let fixture = plan_fixture("I wave.");
    let pack = candidate_pack(&["fact-a"], &["rumor-a"]);
    let candidates = collect_candidates(&pack, &fixture.knowledge, 8);
    let trace = test_trace();

    let rendered = process_plan(
        &bundled_prompt(),
        &story_ctx,
        &fixture.input(),
        &pack,
        &candidates,
        trace.root(),
    )
    .expect("render");
    let instruction = rendered.messages()[2].content.replace("\r\n", "\n");

    assert!(instruction.contains("\"story_goal\": \"non-empty string\""));
    assert!(instruction.contains("\"facts_to_retrieve\": [\"fact-id\"]"));
    assert!(instruction.contains("\"rumors_to_retrieve\": [\"rumor-id\"]"));
    assert!(instruction.contains("Do not return retrieval hints, reasons, prose outside the JSON object"));
}
