use crate::domain::asset::ids::{CanonicalEventKey, FactKey, NarrativeEdgeKey, NarrativeNodeKey, StoryRoleKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::narrative_graph::effect::NarrativeEffectDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeGraphDefinition {
    pub entry_nodes: Vec<NarrativeNodeKey>,
    pub nodes: BTreeMap<NarrativeNodeKey, NarrativeNodeDefinition>,
    pub edges: Vec<NarrativeEdgeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeDefinition {
    pub title: BoundedText,
    pub objective: BoundedText,
    pub activate_when: NarrativeCondition,
    pub complete_when: NarrativeCondition,
    pub skip_when: Option<NarrativeCondition>,
    pub effects: NarrativeNodeEffects,
    pub terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeEffects {
    #[serde(default)]
    pub on_activate: Vec<NarrativeEffectDefinition>,
    #[serde(default)]
    pub on_complete: Vec<NarrativeEffectDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeEdgeDefinition {
    pub edge_key: NarrativeEdgeKey,
    pub from: NarrativeNodeKey,
    pub to: NarrativeNodeKey,
    pub when: NarrativeCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeCondition {
    All {
        conditions: Vec<NarrativeCondition>,
    },
    Any {
        conditions: Vec<NarrativeCondition>,
    },
    Not {
        condition: Box<NarrativeCondition>,
    },
    StoryStarted,
    NodeState {
        node_key: NarrativeNodeKey,
        state: NarrativeNodeState,
    },
    EventOccurred {
        event_key: CanonicalEventKey,
    },
    FactStateEquals {
        fact_key: FactKey,
        value: ScalarValue,
    },
    CharacterStateEquals {
        role_key: StoryRoleKey,
        attribute: BoundedText,
        value: ScalarValue,
    },
    RelationshipReaches {
        source_role_key: StoryRoleKey,
        target_role_key: StoryRoleKey,
        minimum_trust: i16,
    },
    TurnReaches {
        turn: u64,
    },
    PlayerActionOccurred {
        event_key: CanonicalEventKey,
    },
    RoleControllerIs {
        role_key: StoryRoleKey,
        controller: RoleControllerKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeNodeState {
    Inactive,
    Active,
    Completed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleControllerKind {
    Player,
    Ai,
}
