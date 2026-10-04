mod activation;
mod asset;
pub mod error;
mod ids;
mod message;
mod story;
pub mod turn;

pub use activation::{
    ActivationPreviewRequest, ActivationPreviewResult, ActivationTarget, GenerationTrigger, KnowledgeDelivery,
};
pub use asset::{CharacterCardInfo, PackInfo, PackSummaryInfo, ValidationIssue, ValidationReport};
pub use ids::{
    CharacterId, IdempotencyKey, KnowledgeSourceId, PackId, PlayerId, RoleId, SemanticVersion, Sha256Digest, StoryId,
};
pub use story::{RoleStateInfo, StoryContext, StoryHistoryInfo, StoryOpeningInfo, StorySnapshotInfo, StoryTurnInfo};
pub use turn::{
    CommittedTurnInfo, PlayerContribution, TurnCancellation, TurnControl, TurnEvent, TurnEventDeliveryError,
    TurnEventSink, TurnRequest, TurnResult,
};

pub use error::{CoreError, EngineError};
pub use message::{ChatMessage, ChatMessageRole};