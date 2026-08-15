use aise::domain::asset::character_card::{
    AssetSpecVersion, CharacterCard, CharacterMeta, CharacterProfile, CharacterSpec, SpeakingStyle,
};
use aise::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use aise::domain::asset::ids::{CharacterAssetKey, LocationKey, SceneKey, SemanticVersion, Sha256Digest, StoryRoleKey};
use aise::domain::asset::story_pack::{InitialRoleState, StoryProfile, StoryRole, StoryStyle};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::CharacterId;
use aise::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use aise::domain::narrative_graph::projector::NarrativePlan;
use aise::domain::story_instance::binding::{RoleBinding, RoleController};
use aise::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings};
use aise::domain::turn::StoryGeneratorOutput;
use aise::domain::turn::{BaselineContext, CharacterView, NarrativeGraphStateIndex, RetrievalSignals};
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
    let character_id = CharacterId::from("player-1");
    let role_key = StoryRoleKey::from("protagonist");
    let card = CharacterCard {
        spec: CharacterSpec::V3,
        spec_version: AssetSpecVersion::V3_0,
        character_key: CharacterAssetKey::from("player"),
        meta: CharacterMeta {
            name: bounded("Player"),
            creator: None,
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            tags: Vec::new(),
        },
        profile: CharacterProfile {
            description: bounded(adversarial),
            personality: Vec::new(),
            values: Vec::new(),
            fears: Vec::new(),
            speaking_style: SpeakingStyle {
                register: bounded("neutral"),
                verbosity: bounded("medium"),
                traits: Vec::new(),
            },
            dialogue_examples: Vec::new(),
        },
    };
    let role = StoryRole {
        role_label: bounded("Protagonist"),
        narrative_function: bounded("hero"),
        initial_state: InitialRoleState {
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        initial_relationships: Vec::new(),
        seed_memories: Vec::new(),
    };
    let binding = RoleBinding {
        role_key: role_key.clone(),
        character_id: character_id.clone(),
        character_asset: FrozenCharacterAssetRef {
            character_key: card.character_key.clone(),
            version: card.meta.version.clone(),
            digest: Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
        },
        controller: RoleController::Ai,
        bound_at_ms: 0,
    };
    let state = CharacterInstanceState {
        character_id: character_id.clone(),
        role_key: role_key.clone(),
        location: LocationKey::from("village"),
        goals: Vec::new(),
        attributes: BTreeMap::new(),
    };
    let player = CharacterView {
        character_id,
        role_key,
        role,
        binding,
        card,
        state,
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
        player_character: player,
        current_scene: CurrentScene {
            scene_key: SceneKey::from("scene_1"),
            location_key: LocationKey::from("village"),
            time: bounded("morning"),
            description: bounded("scene"),
            present_character_ids: Vec::new(),
        },
        scene_characters: Vec::new(),
        referenced_characters: Vec::new(),
        relevant_knowledge: Vec::new(),
        character_index_scope: aise::domain::turn::RetrievalIndexScope::Complete,
        knowledge_entry_index_scope: aise::domain::turn::RetrievalIndexScope::Complete,
        knowledge_entry_index: Vec::new(),
        character_index: Vec::new(),
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
    let projection = WriterPlannerPromptContextProjector.project(
        &baseline,
        &NarrativePlan::empty(),
        &bounded("go north"),
        &aise::config::PlannerConfig::default(),
    );
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
    let projection = WriterPlannerPromptContextProjector.project(
        &baseline,
        &NarrativePlan::empty(),
        &bounded(marker),
        &aise::config::PlannerConfig::default(),
    );
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
        ("current_scene".into(), Value::String("scene".into())),
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
