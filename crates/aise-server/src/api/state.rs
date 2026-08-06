use crate::config::ServerConfig;
use crate::session::SessionRegistry;
use crate::tasks::TurnTaskSupervisor;
use aise::AiseEngine;
use std::sync::Arc;

pub struct AppState {
    pub engine: Arc<AiseEngine>,
    pub registry: Arc<SessionRegistry>,
    pub tasks: Arc<TurnTaskSupervisor>,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new(
        engine: Arc<AiseEngine>,
        registry: Arc<SessionRegistry>,
        tasks: Arc<TurnTaskSupervisor>,
        config: ServerConfig,
    ) -> Self {
        Self {
            engine,
            registry,
            tasks,
            config,
        }
    }
}
