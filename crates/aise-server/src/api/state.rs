use crate::config::ServerConfig;
use crate::session::SessionRegistry;
use aise::AiseEngine;
use std::sync::Arc;

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
