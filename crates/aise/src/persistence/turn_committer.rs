use async_trait::async_trait;
use std::sync::Arc;

use crate::error::AiseError;
use crate::persistence::store::Store;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Persists a validated Turn atomically and consistently (R-AISE-05).
/// Reads `ctx.draft` and produces a `TurnCommit` for the `Store`.
pub struct TurnCommitter {
    #[allow(dead_code)] // store is exercised once draft-to-commit assembly is implemented
    store: Arc<dyn Store>,
}

impl TurnCommitter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnExecutionPipeline for TurnCommitter {
    fn stage(&self) -> &'static str {
        "turn_committer"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: assemble TurnCommit from ctx.draft and call
        // self.store.commit_turn(&commit).
        Ok(())
    }
}
