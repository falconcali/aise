use aise::core::turn_contract::{StoryId, StoryRevision};
use aise::domain::asset::frozen_ref::FrozenStoryPackRef;
use aise::domain::asset::ids::{
    LocationKey, NarrativeNodeKey, PackId, SemanticVersion, Sha256Digest, StoryPackKey, StoryRoleKey,
};
use aise::domain::asset::story_pack::{StoryProfile, StoryStyle};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::CharacterId;
use aise::domain::narrative::StorySummary;
use aise::domain::narrative_graph::definition::{
    NarrativeCondition, NarrativeGraphDefinition, NarrativeNodeDefinition, NarrativeNodeEffects, NarrativeNodeState,
};
use aise::domain::narrative_graph::director::{NarrativeDirector, NarrativeEvaluation, NarrativeLimits};
use aise::domain::narrative_graph::state::NarrativeRuntimeState;
use aise::domain::story_instance::binding::RoleBinding;
use aise::domain::story_instance::snapshot::{CurrentScene, StoryReadSnapshot};
use aise::domain::story_instance::state::CharacterInstanceState;
use std::collections::BTreeMap;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 4096).unwrap()
}

fn graph() -> NarrativeGraphDefinition {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        NarrativeNodeKey::from("node_a"),
        NarrativeNodeDefinition {
            title: bounded("A"),
            objective: bounded("Wake"),
            activate_when: NarrativeCondition::StoryStarted,
            complete_when: NarrativeCondition::TurnReaches { turn: 0 },
            skip_when: None,
            effects: NarrativeNodeEffects {
                on_activate: Vec::new(),
                on_complete: Vec::new(),
            },
            terminal: false,
        },
    );
    NarrativeGraphDefinition {
        entry_nodes: vec![NarrativeNodeKey::from("node_a")],
        nodes,
        edges: vec![],
    }
}

fn snapshot() -> StoryReadSnapshot {
    let story_id = StoryId::try_new("story-test-1").unwrap();
    let profile = StoryProfile {
        premise: bounded("premise"),
        language: bounded("zh-CN"),
        genre: Vec::new(),
        themes: Vec::new(),
        style: StoryStyle {
            tone: Vec::new(),
            point_of_view: bounded("third"),
            tense: bounded("past"),
        },
    };
    let role_key = StoryRoleKey::from("protagonist");
    let character_id = CharacterId::from("char-1");
    let mut role_bindings = BTreeMap::new();
    role_bindings.insert(
        role_key.clone(),
        RoleBinding {
            role_key: role_key.clone(),
            player_id: None,
            character_id: character_id.clone(),
            bound_at_ms: 0,
        },
    );
    let mut character_states = BTreeMap::new();
    character_states.insert(
        character_id.clone(),
        CharacterInstanceState {
            character_id,
            role_key,
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    );
    let pack_ref = FrozenStoryPackRef {
        pack_id: PackId::from("pack-1"),
        pack_key: StoryPackKey::from("demo"),
        version: SemanticVersion::try_new("0.1.0").unwrap(),
        digest: Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
    };
    StoryReadSnapshot::new(
        story_id,
        StoryRevision::new(0),
        pack_ref,
        profile,
        BTreeMap::new(),
        role_bindings,
        BTreeMap::new(),
        character_states,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        CurrentScene { text: "scene".into() },
        Vec::new(),
        NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        NarrativeRuntimeState::initial(),
        Vec::new(),
        Vec::new(),
        StorySummary { text: String::new() },
        Vec::new(),
    )
}

#[test]
fn director_activates_entry_node_when_story_started() {
    let director = NarrativeDirector::new(NarrativeLimits {
        max_nodes: 64,
        max_edges: 128,
        max_condition_depth: 8,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
    });
    let snapshot = snapshot();
    let runtime = NarrativeRuntimeState::initial();
    let plan = director
        .evaluate(NarrativeEvaluation {
            definition: &graph(),
            state: &runtime,
            snapshot: &snapshot,
        })
        .unwrap();
    assert_eq!(plan.active_nodes.len(), 1);
    assert_eq!(plan.active_nodes[0].as_str(), "node_a");
}

#[test]
fn director_completes_node_when_turn_reaches() {
    let director = NarrativeDirector::new(NarrativeLimits {
        max_nodes: 64,
        max_edges: 128,
        max_condition_depth: 8,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
    });
    let snapshot = snapshot();
    let mut runtime = NarrativeRuntimeState::initial();
    runtime
        .node_states
        .insert(NarrativeNodeKey::from("node_a"), NarrativeNodeState::Active);
    let definition = graph();
    let plan = director
        .evaluate(NarrativeEvaluation {
            definition: &definition,
            state: &runtime,
            snapshot: &snapshot,
        })
        .unwrap();
    assert!(
        plan.proposed_transitions.iter().any(
            |transition| transition.node_key.as_str() == "node_a" && transition.to == NarrativeNodeState::Completed
        )
    );
}

#[test]
fn director_rejects_condition_depth_exceeding_limit() {
    let director = NarrativeDirector::new(NarrativeLimits {
        max_nodes: 64,
        max_edges: 128,
        max_condition_depth: 1,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
    });
    let mut nodes = BTreeMap::new();
    nodes.insert(
        NarrativeNodeKey::from("node_a"),
        NarrativeNodeDefinition {
            title: bounded("A"),
            objective: bounded("Wake"),
            activate_when: NarrativeCondition::Not {
                condition: Box::new(NarrativeCondition::Not {
                    condition: Box::new(NarrativeCondition::Not {
                        condition: Box::new(NarrativeCondition::StoryStarted),
                    }),
                }),
            },
            complete_when: NarrativeCondition::TurnReaches { turn: 3 },
            skip_when: None,
            effects: NarrativeNodeEffects {
                on_activate: Vec::new(),
                on_complete: Vec::new(),
            },
            terminal: false,
        },
    );
    let definition = NarrativeGraphDefinition {
        entry_nodes: vec![NarrativeNodeKey::from("node_a")],
        nodes,
        edges: vec![],
    };
    let snapshot = snapshot();
    let runtime = NarrativeRuntimeState::initial();
    let result = director.evaluate(NarrativeEvaluation {
        definition: &definition,
        state: &runtime,
        snapshot: &snapshot,
    });
    assert!(result.is_err());
}

#[test]
fn director_returns_empty_plan_for_empty_graph() {
    let director = NarrativeDirector::new(NarrativeLimits {
        max_nodes: 64,
        max_edges: 128,
        max_condition_depth: 8,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
    });
    let definition = NarrativeGraphDefinition {
        entry_nodes: Vec::new(),
        nodes: BTreeMap::new(),
        edges: Vec::new(),
    };
    let snapshot = snapshot();
    let runtime = NarrativeRuntimeState::initial();
    let plan = director
        .evaluate(NarrativeEvaluation {
            definition: &definition,
            state: &runtime,
            snapshot: &snapshot,
        })
        .unwrap();
    assert!(plan.active_nodes.is_empty());
    assert!(plan.proposed_transitions.is_empty());
}

#[test]
fn runtime_state_defaults_to_inactive() {
    let runtime = NarrativeRuntimeState::initial();
    assert_eq!(
        runtime.node_state(&NarrativeNodeKey::from("missing")),
        NarrativeNodeState::Inactive
    );
}
