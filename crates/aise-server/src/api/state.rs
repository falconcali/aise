use crate::config::ServerConfig;
use crate::session::SessionRegistry;
use crate::tasks::TurnTaskSupervisor;
use aise::AiseEngine;
use aise::story::instance_factory::StoryInstanceFactory;
use aise::story::pack_service::PackService;
use std::sync::Arc;

pub struct AppState {
    pub engine: Arc<AiseEngine>,
    pub registry: Arc<SessionRegistry>,
    pub tasks: Arc<TurnTaskSupervisor>,
    pub config: ServerConfig,
    pub pack_service: Option<Arc<PackService>>,
    pub instance_factory: Option<Arc<StoryInstanceFactory>>,
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
            pack_service: None,
            instance_factory: None,
        }
    }

    pub fn with_services(
        mut self,
        pack_service: Arc<PackService>,
        instance_factory: Arc<StoryInstanceFactory>,
    ) -> Self {
        self.pack_service = Some(pack_service);
        self.instance_factory = Some(instance_factory);
        self
    }
}
