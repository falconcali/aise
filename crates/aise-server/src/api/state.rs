use crate::config::ServerConfig;
use crate::session::SessionRegistry;
use crate::tasks::TurnTaskManager;
use aise::AiseEngine;
use std::sync::Arc;

pub struct AppState {
    pub engine: Arc<AiseEngine>,
    pub registry: Arc<SessionRegistry>,
    pub tasks: Arc<TurnTaskManager>,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new(
        engine: Arc<AiseEngine>,
        registry: Arc<SessionRegistry>,
        tasks: Arc<TurnTaskManager>,
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
