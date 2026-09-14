use crate::domain::asset::ids::LocationKey;
use crate::domain::ids::RoleId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeParticipant {
    Role(RoleId),
    Location(LocationKey),
}

#[cfg(test)]
#[path = "tests/participant_tests.rs"]
mod tests;
