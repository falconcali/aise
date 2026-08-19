use super::*;
use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{NarrativeNodeKey, PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::ids::{StoryId, StoryRevision, TurnKey, TurnNumber};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::effect::{CharacterImpulse, ImpulseUrgency};
use crate::domain::narrative_graph::projector::{NarrativePlan, NarrativeProjection};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshot, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    CharacterThinkRequest, NarrativeGraphStateIndex, RetrievalPlan, RetrievalSignals, WriterPlan, WriterStoryGoal,
};
use crate::turn::turn_budget::TurnBudget;
use crate::turn::turn_contract::{IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest};
use crate::turn::turn_trace::TraceRecorder;
use std::time::{Duration, Instant};

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).unwrap()
}

fn prompt_context() -> StoryGeneratorPromptContext {
    StoryGeneratorPromptContext {
        story_profile: StoryProfilePromptView {
            title: bounded("Untitled Story"),
            language: bounded("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            tone: Vec::new(),
            point_of_view: bounded("second"),
            tense: bounded("present"),
        },
        instance_settings: StoryGeneratorInstanceSettingsPromptView {
            cast_policy: CastPolicy::Closed,
        },
        story_continuity: StoryContinuityPromptView {
            story_summary: bounded("summary"),
            recent_story: vec![bounded("recent")],
        },
        player_role: role("player"),
        ai_roles: vec![role("npc")],
        relevant_knowledge: crate::prompt::WorldKnowledgePromptView::default(),
        story_goal: bounded("goal-marker"),
        narrative_direction: crate::prompt::NarrativeDirectionPromptView::default(),
        active_story_constraints: Vec::new(),
        character_decisions: Vec::new(),
        interpreted_player_contribution: crate::domain::turn::InterpretedPlayerContribution { units: vec![] },
    }
}

fn role(id: &str) -> StoryGeneratorRolePromptView {
    StoryGeneratorRolePromptView {
        role_id: RoleId::try_new(id).unwrap(),
        name: bounded(id),
        role_label: bounded("role"),
        appearance: Some(bounded("description")),
        personality: None,
        speaking_style: Some(bounded("neutral, medium length")),
        dialogue_examples: Vec::new(),
        background: Some(bounded("background")),
        state: StoryGeneratorRoleStatePromptView {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        knowledge: crate::prompt::RoleKnowledgePromptView::default(),
    }
}

fn decision(
    id: &str,
    decision_text: &str,
    suggested_utterance: Option<&str>,
) -> StoryGeneratorCharacterDecisionPromptView {
    StoryGeneratorCharacterDecisionPromptView {
        role_id: RoleId::try_new(id).unwrap(),
        name: bounded(id),
        decision: bounded(decision_text),
        suggested_utterance: suggested_utterance.map(bounded),
    }
}

#[test]
fn story_generator_assets_have_required_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/story-generator.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/story-generator.md.j2");

    assert_eq!(section_item_count(csi, "## MUST", "## SHOULD"), 10);
    assert_eq!(section_item_count(csi, "## SHOULD", "## NEVER"), 3);
    assert_eq!(section_item_count(csi, "## NEVER", "# Runtime Data Boundary"), 5);
    assert_eq!(section_item_count(fti, "## MUST", "## NEVER"), 5);
    assert_eq!(section_item_count(fti, "## NEVER", "# Output"), 3);
    assert!(!fti.contains("## SHOULD"));
}

#[test]
fn story_generator_assets_require_every_contribution_component_on_page() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/story-generator.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/story-generator.md.j2");
    assert!(csi.contains("not-yet-narrated source material for this segment"));
    assert!(csi.contains("Realize each unit according to its `kind`"));
    assert!(csi.contains("Preserve order and essential meaning"));
    assert!(csi.contains("`requested_outcome` as a non-authoritative desired result"));
    assert!(csi.contains("Omit, merely imply, or jump past"));
    assert!(fti.contains(
        "Put every explicitly supplied Player Character utterance, attempted action, and private thought on the page"
    ));
}

#[test]
fn story_generator_runtime_context_has_exact_section_order() {
    let rc = include_str!("../../../assets/prompts/context-v2/rc/story-generator.md.j2");
    let headings = [
        "## Story Profile",
        "## Instance Settings",
        "## Story Continuity",
        "## Player Character",
        "## AI Characters",
        "## Active Story Constraints",
        "## Immediate Story Goal",
        "## Narrative Direction",
        "## Relevant Knowledge",
        "## AI Character Decisions",
        "## Interpreted Player Contribution",
    ];
    let mut previous = 0;
    for heading in headings {
        let current = rc.find(heading).unwrap();
        assert!(current >= previous);
        previous = current;
    }
    assert_eq!(rc.matches("{{ output_schema }}").count(), 0);
    assert!(rc.rfind("## Interpreted Player Contribution").unwrap() > rc.rfind("## AI Character Decisions").unwrap());
    assert!(!rc.contains("Current Scene"));
}

#[test]
fn empty_optional_story_generator_sections_render_no_sentinel() {
    assert_eq!(
        crate::prompt::render_narrative_direction(&crate::prompt::NarrativeDirectionPromptView::default()),
        ""
    );
    assert_eq!(
        crate::prompt::render_relevant_knowledge(&crate::prompt::WorldKnowledgePromptView::default()),
        ""
    );
    assert_eq!(render_decisions(&[]), "");
    assert_eq!(render_roles(&[]), "");
}

#[test]
fn story_profile_omits_empty_optional_lists() {
    let rendered = render_story_profile_view(&StoryProfilePromptView {
        title: bounded("The Lodge Keeper"),
        language: bounded("zh-CN"),
        genre: Vec::new(),
        themes: Vec::new(),
        tone: Vec::new(),
        point_of_view: bounded("second"),
        tense: bounded("present"),
    });
    assert!(!rendered.contains("genre"));
    assert!(!rendered.contains("themes"));
    assert!(!rendered.contains("tone"));
    assert!(rendered.contains("title: \"The Lodge Keeper\""));
    assert!(rendered.contains("language: \"zh-CN\""));
    assert!(rendered.contains("point_of_view: \"second\""));
}

#[test]
fn role_rendering_elides_empty_goals_and_attributes_but_keeps_location() {
    let rendered = render_role(&role("npc"), None);
    assert!(rendered.contains("location: \"hall\""));
    assert!(!rendered.contains("goals"));
    assert!(!rendered.contains("attributes"));
}

#[test]
fn decision_rendering_contains_only_target_name_decision_and_optional_utterance() {
    let rendered = render_decisions(&[
        decision("npc-1", "hide", Some("stay back")),
        decision("npc-2", "flee", None),
    ]);
    assert!(rendered.contains("role_id: \"npc-1\""));
    assert!(rendered.contains("decision: \"hide\""));
    assert!(rendered.contains("suggested_utterance: \"stay back\""));
    assert!(rendered.contains("role_id: \"npc-2\""));
    assert!(rendered.contains("decision: \"flee\""));
    let npc2_block = rendered.split("- role_id: \"npc-2\"").nth(1).unwrap();
    assert!(!npc2_block.contains("suggested_utterance"));
    let npc1_index = rendered.find("npc-1").unwrap();
    let npc2_index = rendered.find("npc-2").unwrap();
    assert!(npc1_index < npc2_index);
}

#[test]
fn runtime_projection_contains_only_allowlisted_semantic_sections() {
    let vars = render_runtime_vars(&prompt_context());
    let values = vars.as_map();

    assert_eq!(values.len(), 12);
    assert_eq!(values["story_goal"].as_str(), Some("\"goal-marker\""));
    assert!(values["interpreted_player_contribution"].as_str().unwrap().is_empty());
    assert!(!values.contains_key("current_scene"));
    assert!(!values.contains_key("retrieval_plan"));
    assert!(!values.contains_key("character_think_requests"));
    assert!(!values.contains_key("role_index"));
    assert!(!values.contains_key("narrative_state_view"));
    assert!(!values.contains_key("retrieval_signals"));
    assert!(values.contains_key("character_decisions"));
}

#[test]
fn story_generator_renders_story_continuity_as_prose() {
    let mut context = prompt_context();
    context.story_continuity = StoryContinuityPromptView {
        story_summary: bounded("summary-one"),
        recent_story: vec![bounded("recent-one"), bounded("recent-two")],
    };
    let vars = render_runtime_vars(&context);
    let values = vars.as_map();
    assert_eq!(values["story_summary"].as_str().unwrap(), "summary-one");
    assert_eq!(values["recent_story"].as_str().unwrap(), "recent-one\n\nrecent-two");
}

#[test]
fn full_role_rendering_uses_stage_visibility_contract() {
    let mut value = role("guard");
    value.dialogue_examples = vec![dialogue_example("challenged", "State your business.")];
    let rendered = render_role(&value, Some("- "));
    assert!(rendered.starts_with("- role_id: \"guard\"\n  name: \"guard\""));
    assert!(rendered.contains("\n  role: \"role\""));
    assert!(!rendered.contains("presence:"));
    assert!(rendered.contains("\n  appearance: \"description\""));
    assert!(rendered.contains("\n  speaking_style: \"neutral, medium length\""));
    assert!(rendered.contains("\n  dialogue_examples:"));
    assert!(rendered.contains("\n  background: \"background\""));
    assert!(!rendered.contains("control:"));
}

#[test]
fn player_role_rendering_omits_redundant_presence_and_controller() {
    let rendered = render_role(&role("player"), None);
    assert!(!rendered.contains("presence:"));
    assert!(!rendered.contains("control:"));
}

#[test]
fn global_budget_prunes_dialogue_examples_by_descending_role_id() {
    let mut context = prompt_context();
    context.player_role.role_id = RoleId::try_new("a-player").unwrap();
    context.player_role.dialogue_examples = vec![dialogue_example("player", "player response")];
    context.ai_roles[0].role_id = RoleId::try_new("z-npc").unwrap();
    context.ai_roles[0].dialogue_examples = vec![dialogue_example("npc", "npc response")];
    let initial_tokens = runtime_tokens(&render_runtime_vars(&context));
    let vars = prune_dialogue_examples_to_budget(&mut context, initial_tokens - 1, 0).expect("pruned");
    assert!(runtime_tokens(&vars) < initial_tokens);
    assert!(context.ai_roles[0].dialogue_examples.is_empty());
    assert_eq!(context.player_role.dialogue_examples.len(), 1);
}

#[test]
fn required_data_overflow_removes_all_examples_before_error() {
    let mut context = prompt_context();
    context.player_role.dialogue_examples = vec![dialogue_example("player", "response")];
    context.ai_roles[0].dialogue_examples = vec![dialogue_example("npc", "response")];
    let error = prune_dialogue_examples_to_budget(&mut context, 1, 0).unwrap_err();
    assert!(matches!(error, StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget));
    assert!(context.player_role.dialogue_examples.is_empty());
    assert!(context.ai_roles[0].dialogue_examples.is_empty());
}

fn dialogue_example(situation: &str, response: &str) -> DialogueExample {
    DialogueExample {
        situation: bounded(situation),
        response: bounded(response),
    }
}

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn story_profile() -> StoryProfile {
    StoryProfile {
        language: bounded("zh-CN"),
        genre: Vec::new(),
        themes: Vec::new(),
        style: StoryStyle {
            tone: Vec::new(),
            point_of_view: bounded("third"),
            tense: bounded("past"),
        },
    }
}

fn story_continuity() -> StoryContinuity {
    StoryContinuity::try_new(
        StorySummary {
            text: bounded(""),
            summarized_through: None,
        },
        Vec::new(),
        StoryContinuityLimits {
            max_summary_bytes: 256,
            max_recent_segments: 4,
            max_recent_segment_bytes: 128,
            max_recent_segment_tokens: 32,
        },
    )
    .unwrap()
}

fn story_role(id: &str, controller: RoleController) -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new(id).unwrap(),
        controller,
        role_label: bounded(id),
        narrative_function: bounded("role"),
        background: None,
        effective_profile: CharacterProfile {
            name: bounded(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character: None,
        state: StoryRoleState {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    }
}

fn character_impulse(target_id: &str) -> CharacterImpulse {
    CharacterImpulse {
        source_node: NarrativeNodeKey::try_new("node.impulse").unwrap(),
        target_role_id: RoleId::try_new(target_id).unwrap(),
        goal: bounded("goal"),
        reason: None,
        emotion: None,
        urgency: ImpulseUrgency::Medium,
        expires_after_turn: None,
    }
}

fn sample_baseline(player: &StoryRole, relevant: &[&StoryRole]) -> BaselineContext {
    BaselineContext {
        story_title: bounded("Untitled Story"),
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        player_role: RoleContextView::from(&StoryRoleView::from(player)),
        relevant_roles: relevant
            .iter()
            .map(|role| RoleContextView::from(&StoryRoleView::from(*role)))
            .collect(),
        relevant_world_knowledge: crate::domain::turn::RelevantWorldKnowledge::default(),
        knowledge_index: Vec::new(),
        role_index: Vec::new(),
        story_continuity: story_continuity(),
        active_story_constraints: Vec::new(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: digest(),
            graph_revision: 0,
            node_states: BTreeMap::new(),
        },
        retrieval_signals: RetrievalSignals::default(),
    }
}

fn sample_snapshot(roles: &[&StoryRole]) -> StoryReadSnapshot {
    let mut role_map = BTreeMap::new();
    for role in roles {
        role_map.insert(role.role_id.clone(), StoryRoleView::from(*role));
    }
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_title: bounded("Untitled Story"),
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        roles: role_map,
        relationships: Vec::new(),
        narrative_definition: NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        narrative_state: NarrativeRuntimeState::initial(),
        fact_values: BTreeMap::new(),
        story_continuity: story_continuity(),
        active_constraints: Vec::new(),
        entity_catalog: Vec::new(),
        topic_dictionary: BTreeMap::new(),
        knowledge_snapshot: KnowledgeSnapshotRef {
            story_id: StoryId::try_new("story-1").unwrap(),
            pack_digest: digest(),
            base_revision: StoryRevision::new(0),
            knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::zero(),
        },
        role_id_high_water: crate::domain::ids::RoleIdHighWater::zero(),
    })
    .unwrap()
}

fn build_context(
    all_roles: &[&StoryRole],
    baseline: BaselineContext,
    think_targets: Vec<&str>,
    impulse_targets: Vec<&str>,
) -> TurnExecutionContext {
    build_context_with_retrieval(all_roles, baseline, think_targets, impulse_targets, None)
}

fn build_context_with_retrieval(
    all_roles: &[&StoryRole],
    baseline: BaselineContext,
    think_targets: Vec<&str>,
    impulse_targets: Vec<&str>,
    retrieved: Option<crate::domain::turn::RetrievedContext>,
) -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    let identity = TurnIdentity::new(
        TurnKey::new(StoryId::try_new("story-1").unwrap(), TurnNumber::try_new(1).unwrap()),
        IdempotencyKey::try_new("idem-1").unwrap(),
        0,
    );
    let request = TurnRequest::try_new("go north".to_owned()).unwrap();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = TraceRecorder::with_limits(budget.max_trace_spans());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control, trace).unwrap();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(sample_snapshot(all_roles), baseline).unwrap();
    if !impulse_targets.is_empty() {
        ctx.set_narrative_projection(NarrativeProjection {
            plan: NarrativePlan {
                active_nodes: Vec::new(),
                active_directions: Vec::new(),
                world_event_intents: Vec::new(),
                character_impulses: impulse_targets.into_iter().map(character_impulse).collect(),
                effect_dispositions: Vec::new(),
            },
            condition_queries: Vec::new(),
            expected_graph_revision: 0,
        })
        .unwrap();
    }
    let plan = WriterPlan {
        interpreted_player_contribution: crate::domain::turn::InterpretedPlayerContribution { units: vec![] },
        story_goal: WriterStoryGoal {
            summary: bounded("goal"),
        },
        retrieval_plan: RetrievalPlan::default(),
        character_think_requests: think_targets
            .into_iter()
            .map(|id| CharacterThinkRequest {
                role_id: RoleId::try_new(id).unwrap(),
                reason: bounded("think"),
            })
            .collect(),
    };
    ctx.set_writer_plan(plan).unwrap();
    if !ctx.plan().unwrap().character_think_requests.is_empty() {
        let decisions = ctx
            .plan()
            .unwrap()
            .character_think_requests
            .iter()
            .map(|request| crate::domain::turn::CharacterDecision {
                role_id: request.role_id.clone(),
                decision: bounded("decision"),
                suggested_utterance: None,
            })
            .collect();
        ctx.set_character_decisions(decisions).unwrap();
    }
    if let Some(retrieved) = retrieved {
        ctx.set_retrieved_context(retrieved).unwrap();
    }
    ctx
}

#[test]
fn story_generator_unions_relevant_and_requested_ai_roles() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc_alpha = story_role("npc-alpha", RoleController::Ai);
    let npc_bravo = story_role("npc-bravo", RoleController::Ai);
    let npc_charlie = story_role("npc-charlie", RoleController::Ai);
    let all_roles = [&player, &npc_alpha, &npc_bravo, &npc_charlie];
    let baseline = sample_baseline(&player, &[&npc_charlie]);
    let ctx = build_context(
        &all_roles,
        baseline,
        vec!["npc-alpha", "npc-charlie"],
        vec!["npc-bravo", "npc-charlie"],
    );
    let baseline = ctx.baseline().unwrap();
    let config = ContextPreparationConfig::default();
    let ai_roles = project_ai_roles(&ctx, baseline, &config).expect("ai roles project");

    let ids: Vec<String> = ai_roles.iter().map(|role| role.role_id.as_str().to_owned()).collect();
    assert_eq!(ids, vec!["npc-alpha", "npc-bravo", "npc-charlie"]);
    assert!(!ids.contains(&"protagonist".to_owned()));
}

#[test]
fn runtime_context_projectors_preserve_slot_key_sets() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let all_roles = [&player];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context(&all_roles, baseline, Vec::new(), Vec::new());

    let projector = DefaultStoryGeneratorPromptContextProjector::new(ContextPreparationConfig::default());
    let projection = projector.project(&ctx).expect("generator projection");

    let expected: std::collections::BTreeSet<&str> = [
        "story_profile",
        "instance_settings",
        "story_summary",
        "recent_story",
        "player_character",
        "ai_characters",
        "active_story_constraints",
        "story_goal",
        "narrative_direction",
        "relevant_knowledge",
        "character_decisions",
        "interpreted_player_contribution",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = projection.rc_vars.as_map().keys().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "story_generator RC slot keys must match assets/prompts/context-v2/slots.yaml exactly"
    );
}

fn knowledge_item(
    source_id: crate::domain::knowledge::KnowledgeSourceId,
    body: &str,
) -> crate::domain::turn::RetrievedKnowledgeItem {
    crate::domain::turn::RetrievedKnowledgeItem::from_parts(
        source_id,
        bounded(body),
        crate::domain::knowledge::KnowledgeSource::Seed {
            pack_id: crate::domain::asset::ids::PackId::try_new("pack-1").unwrap(),
            pack_digest: digest(),
        },
        crate::domain::turn::RelevanceRank {
            match_level: crate::domain::turn::MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
        BTreeMap::new(),
    )
}

fn retrieval_limits() -> crate::domain::turn::RetrievedContextLimits {
    crate::domain::turn::RetrievedContextLimits {
        max_role_audiences: 8,
        max_items_per_audience: 8,
        max_tokens_per_audience: 10_000,
        max_total_items: 32,
        max_total_tokens: 10_000,
        max_item_bytes: 4096,
    }
}

#[test]
fn generator_preserves_baseline_relevant_knowledge() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let all_roles = [&player];
    let mut baseline = sample_baseline(&player, &[]);
    baseline.relevant_world_knowledge = crate::domain::turn::RelevantWorldKnowledge {
        facts: vec![crate::domain::turn::RelevantWorldKnowledgeItem {
            source_id: crate::domain::knowledge::KnowledgeSourceId::Fact(
                crate::domain::ids::FactId::try_new("fact_0001").unwrap(),
            ),
            content: bounded("the tower fell a year ago"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let ctx = build_context(&all_roles, baseline, Vec::new(), Vec::new());

    let projector = DefaultStoryGeneratorPromptContextProjector::new(ContextPreparationConfig::default());
    let projection = projector.project(&ctx).expect("generator projection");

    assert_eq!(projection.context.relevant_knowledge.facts.len(), 1);
    assert_eq!(
        projection.context.relevant_knowledge.facts[0].as_str(),
        "the tower fell a year ago"
    );
    let rendered = projection
        .rc_vars
        .as_map()
        .get("relevant_knowledge")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    assert!(rendered.contains("the tower fell a year ago"));
}

#[test]
fn generator_attaches_character_context_by_role_id() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc_alpha = story_role("npc-alpha", RoleController::Ai);
    let npc_bravo = story_role("npc-bravo", RoleController::Ai);
    let all_roles = [&player, &npc_alpha, &npc_bravo];
    let baseline = sample_baseline(&player, &[&npc_alpha, &npc_bravo]);
    let mut characters = BTreeMap::new();
    characters.insert(
        RoleId::try_new("npc-alpha").unwrap(),
        crate::domain::turn::RetrievedCharacterContext {
            role: None,
            known_rumors: vec![knowledge_item(
                crate::domain::knowledge::KnowledgeSourceId::Rumor(
                    crate::domain::ids::RumorId::try_new("rumor_0001").unwrap(),
                ),
                "alpha heard a rumor",
            )],
            memories: Vec::new(),
        },
    );
    characters.insert(
        RoleId::try_new("npc-bravo").unwrap(),
        crate::domain::turn::RetrievedCharacterContext {
            role: None,
            known_rumors: vec![knowledge_item(
                crate::domain::knowledge::KnowledgeSourceId::Rumor(
                    crate::domain::ids::RumorId::try_new("rumor_0002").unwrap(),
                ),
                "bravo heard a different rumor",
            )],
            memories: Vec::new(),
        },
    );
    let retrieved = crate::domain::turn::RetrievedContext::try_new(
        crate::domain::turn::RetrievedWorldKnowledge::default(),
        characters,
        retrieval_limits(),
    )
    .unwrap();
    let ctx = build_context_with_retrieval(
        &all_roles,
        baseline,
        vec!["npc-alpha", "npc-bravo"],
        Vec::new(),
        Some(retrieved),
    );

    let projector = DefaultStoryGeneratorPromptContextProjector::new(ContextPreparationConfig::default());
    let projection = projector.project(&ctx).expect("generator projection");

    let alpha = projection
        .context
        .ai_roles
        .iter()
        .find(|role| role.role_id.as_str() == "npc-alpha")
        .expect("alpha role present");
    let bravo = projection
        .context
        .ai_roles
        .iter()
        .find(|role| role.role_id.as_str() == "npc-bravo")
        .expect("bravo role present");

    assert_eq!(alpha.knowledge.known_rumors.len(), 1);
    assert_eq!(alpha.knowledge.known_rumors[0].as_str(), "alpha heard a rumor");
    assert_eq!(bravo.knowledge.known_rumors.len(), 1);
    assert_eq!(bravo.knowledge.known_rumors[0].as_str(), "bravo heard a different rumor");
}
