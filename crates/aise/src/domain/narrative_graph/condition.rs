use crate::domain::asset::ids::{CanonicalEventKey, FactKey, NarrativeNodeKey, StoryRoleKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use serde::{Deserialize, Serialize};

pub use crate::domain::narrative_graph::definition::{NarrativeCondition, NarrativeNodeState, RoleControllerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleControllerKindMirror {
    Player,
    Ai,
}

#[allow(dead_code)]
pub(crate) fn _condition_anchor(
    _: NarrativeCondition,
    _: NarrativeNodeKey,
    _: CanonicalEventKey,
    _: FactKey,
    _: StoryRoleKey,
    _: BoundedText,
    _: ScalarValue,
) {
}
