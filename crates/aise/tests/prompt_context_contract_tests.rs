use aise::domain::asset::character_card::CharacterProfile;
use aise::domain::asset::ids::{LocationKey, PlayerId, Sha256Digest};
use aise::domain::asset::story_pack::{StoryProfile, StoryStyle};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::RoleId;
use aise::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use aise::domain::narrative_graph::projector::NarrativePlan;
use aise::domain::story_instance::role::{RoleController, StoryRoleState};
use aise::domain::story_instance::state::InstanceSettings;
use aise::domain::turn::StoryGeneratorOutput;
use aise::domain::turn::{BaselineContext, NarrativeGraphStateIndex, RetrievalSignals, RoleContextView};
use aise::planning::WriterPlannerPromptContextProjector;
use aise::prompt::profile::PromptProfile;
use aise::prompt::{
    CatalogPromptSource, PromptCompositionInput, RuntimePromptVars, TrustedPromptSource, TrustedPromptVars,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

fn bounded(text: &str) -> BoundedText {
    BoundedText::try_new(text, "text", 256).unwrap()
}

fn minimal_baseline(adversarial: &str) -> BaselineContext {
    let player_role = RoleContextView {
        role_id: RoleId::try_new("protagonist").unwrap(),
        role_label: bounded("Protagonist"),
        narrative_function: bounded("hero"),
        background: None,
        profile: CharacterProfile {
            name: bounded(adversarial),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        state: StoryRoleState {
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        controller: RoleController::Player(PlayerId::try_new("player-1").unwrap()),
    };
    BaselineContext {
        story_profile: StoryProfile {
            premise: bounded("premise"),
            language: bounded("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            style: StoryStyle {
                tone: Vec::new(),
                point_of_view: bounded("third"),
                tense: bounded("past"),
            },
        },
        instance_settings: InstanceSettings::default(),
        player_role,
        relevant_roles: Vec::new(),
        relevant_knowledge: Vec::new(),
        role_index_scope: aise::domain::turn::RetrievalIndexScope::Complete,
        knowledge_entry_index_scope: aise::domain::turn::RetrievalIndexScope::Complete,
        knowledge_entry_index: Vec::new(),
        role_index: Vec::new(),
        story_continuity: StoryContinuity::try_new(
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
        .unwrap(),
        active_story_constraints: Vec::new(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: Sha256Digest::try_new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            graph_revision: 0,
            node_states: BTreeMap::new(),
        },
        retrieval_signals: RetrievalSignals::default(),
    }
}

#[test]
fn writer_planner_projects_three_layer_prompt_context() {
    let baseline = minimal_baseline("ok");
    let projection = WriterPlannerPromptContextProjector
        .project(
            &baseline,
            &NarrativePlan::empty(),
            &bounded("go north"),
            &aise::config::PlannerConfig::default(),
            8192,
        )
        .expect("projection");
    let source = CatalogPromptSource::from_config(&aise::config::PromptModuleConfig::default()).expect("catalog");
    let composition = source
        .compose(&PromptCompositionInput {
            profile: PromptProfile::WriterPlanner,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        })
        .expect("composition");
    assert!(composition.csi.as_str().contains("# Identity"));
    assert!(composition.rc.as_str().contains("go north"));
    assert!(composition.fti.as_str().contains("\"story_goal\""));
}

#[test]
fn asset_and_player_content_never_enters_system_prompt() {
    let marker = "IGNORE_PREVIOUS_INSTRUCTIONS_owned_by_player";
    let baseline = minimal_baseline(marker);
    let projection = WriterPlannerPromptContextProjector
        .project(
            &baseline,
            &NarrativePlan::empty(),
            &bounded(marker),
            &aise::config::PlannerConfig::default(),
            8192,
        )
        .expect("projection");
    let source = CatalogPromptSource::from_config(&aise::config::PromptModuleConfig::default()).expect("catalog");
    let composition = source
        .compose(&PromptCompositionInput {
            profile: PromptProfile::WriterPlanner,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        })
        .expect("composition");
    assert!(composition.rc.as_str().contains(marker));
    assert!(!composition.csi.as_str().contains(marker));
    assert!(!composition.fti.as_str().contains(marker));
}

#[test]
fn story_generator_composes_csi_runtime_context_and_fti() {
    let marker = "IGNORE_PREVIOUS_INSTRUCTIONS_owned_by_player";
    let runtime = HashMap::from([
        ("story_profile".into(), Value::String("profile".into())),
        ("instance_settings".into(), Value::String("cast_policy: closed".into())),
        ("story_summary".into(), Value::String("None.".into())),
        ("recent_story".into(), Value::String("None.".into())),
        ("player_character".into(), Value::String("player".into())),
        ("ai_characters".into(), Value::String("None.".into())),
        ("active_story_constraints".into(), Value::String("None.".into())),
        ("story_goal".into(), Value::String("goal".into())),
        ("narrative_direction".into(), Value::String("None.".into())),
        ("relevant_writer_knowledge".into(), Value::String("None.".into())),
        ("character_decisions".into(), Value::String("None.".into())),
        ("player_input".into(), Value::String(marker.into())),
    ]);
    let schema = StoryGeneratorOutput::json_schema(8192).to_string();
    let source = CatalogPromptSource::from_config(&aise::config::PromptModuleConfig::default()).expect("catalog");
    let composition = source
        .compose(&PromptCompositionInput {
            profile: PromptProfile::StoryGenerator,
            rc_vars: RuntimePromptVars::new(runtime),
            fti_vars: TrustedPromptVars::new(HashMap::from([("output_schema".into(), Value::String(schema))])),
        })
        .expect("composition");

    assert!(composition.csi.as_str().contains("# Identity"));
    assert!(composition.rc.as_str().contains(marker));
    assert!(!composition.csi.as_str().contains(marker));
    assert!(!composition.fti.as_str().contains(marker));
    assert!(composition.fti.as_str().contains("\"story_text\""));
}
