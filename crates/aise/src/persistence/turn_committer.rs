use crate::error::AiseError;
use crate::persistence::store::Store;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;

pub struct TurnCommitter {
    #[allow(dead_code)]
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
        Ok(())
    }
}
