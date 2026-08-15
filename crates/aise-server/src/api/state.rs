use crate::config::ServerConfig;
use crate::session::SessionRegistry;
use crate::tasks::TurnTaskSupervisor;
use aise::AiseEngine;
use aise::persistence::StoryHistoryReadPort;
use aise::story::character_card_service::CharacterCardService;
use aise::story::instance_factory::StoryInstanceFactory;
use aise::story::pack_service::PackService;
use std::sync::Arc;

pub struct AppState {
    pub engine: Arc<AiseEngine>,
    pub registry: Arc<SessionRegistry>,
    pub tasks: Arc<TurnTaskSupervisor>,
    pub config: ServerConfig,
    pub pack_service: Option<Arc<PackService>>,
    pub character_card_service: Option<Arc<CharacterCardService>>,
    pub instance_factory: Option<Arc<StoryInstanceFactory>>,
    pub story_history_reader: Option<Arc<dyn StoryHistoryReadPort>>,
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
            character_card_service: None,
            instance_factory: None,
            story_history_reader: None,
        }
    }

    pub fn with_services(
        mut self,
        pack_service: Arc<PackService>,
        character_card_service: Arc<CharacterCardService>,
        instance_factory: Arc<StoryInstanceFactory>,
        story_history_reader: Arc<dyn StoryHistoryReadPort>,
    ) -> Self {
        self.pack_service = Some(pack_service);
        self.character_card_service = Some(character_card_service);
        self.instance_factory = Some(instance_factory);
        self.story_history_reader = Some(story_history_reader);
        self
    }
}
