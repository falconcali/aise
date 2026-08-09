#![forbid(unsafe_code)]

pub mod character;
pub mod config;
pub mod context;
pub mod core;
pub mod domain;
pub mod engine;
pub mod error;
pub mod llm;
pub mod persistence;
pub mod planning;
pub mod prompt;
pub mod runtime;
pub mod story;
pub mod validation;

pub use config::{
    AiseConfig, AssetLimitsConfig, ContextPreparationConfig, CoordinatorConfig, LlmConfig, PlannerConfig,
    PromptModuleConfig, RetrievalConfig, StorageConfig, TraceContentPolicy, TurnConfig, TurnContentLimitsConfig,
};
pub use core::turn_contract::{CommittedTurnResult, ExecuteTurnSpec};
pub use core::turn_event::{TurnEvent, TurnEventSink};
pub use engine::AiseEngine;
pub use error::AiseError;
pub use persistence::asset_store::{AssetStore, FrozenStoryPack, PackInfo, ValidatedStoryPack};
pub use persistence::store::MaterializedStoryInstanceSpec;
pub use story::pack_service::{
    AssetExportError, AssetImportError, AssetInput, NativeAssetImporter, PackExport, PackExportFormat, PackService,
};
