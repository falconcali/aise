use crate::domain::asset::constraint::{StoryConstraintLifecycle, StoryConstraintRequirement, StoryConstraintScope};
use crate::domain::asset::ids::{ConstraintKey, PackId};
use crate::domain::ids::{ConstraintId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintSource {
    Pack {
        pack_id: PackId,
        constraint_key: ConstraintKey,
    },
    CommittedTurn {
        turn_id: TurnId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveStoryConstraint {
    pub id: ConstraintId,
    pub source: StoryConstraintSource,
    pub scope: StoryConstraintScope,
    pub requirement: StoryConstraintRequirement,
    pub lifecycle: StoryConstraintLifecycle,
}

pub use crate::domain::asset::constraint::StoryConstraintDefinition;
