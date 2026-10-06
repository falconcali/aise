use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change<T> {
    Unchanged,
    Replaced(T),
}