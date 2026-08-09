use crate::domain::asset::ids::{NarrativeNodeKey, SceneKey, StoryRoleKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::story_sequence::StorySequence;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryConstraintDefinition {
    pub scope: StoryConstraintScope,
    pub requirement: StoryConstraintRequirement,
    pub lifecycle: StoryConstraintLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintScope {
    Story,
    Scene { scene_key: SceneKey },
    Role { role_key: StoryRoleKey },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintRequirement {
    Require { statement: BoundedText },
    Forbid { statement: BoundedText },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintLifecycle {
    Persistent,
    ThroughSequence { sequence: StorySequence },
    UntilNarrativeNodeResolved { node_key: NarrativeNodeKey },
}
