use crate::core::TurnNumber;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "turn", rename_all = "snake_case")]
pub enum EntryOrigin {
    Seed,
    Turn(TurnNumber),
}
