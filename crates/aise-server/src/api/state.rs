use std::sync::Arc;

use aise::AiseEngine;

use crate::config::ServerConfig;
use crate::session::SessionRegistry;

/// Composition root state shared by all handlers.
pub struct AppState {
    pub engine: Arc<AiseEngine>,
    pub registry: Arc<SessionRegistry>,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new(engine: Arc<AiseEngine>, registry: Arc<SessionRegistry>, config: ServerConfig) -> Self {
        Self {
            engine,
            registry,
            config,
        }
    }
}
