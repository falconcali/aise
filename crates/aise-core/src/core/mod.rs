mod activation;
mod asset;
mod change;
pub mod error;
mod ids;
mod message;
mod story;
pub mod turn;
mod world;

pub use activation::{
    ActivationPreviewRequest, ActivationPreviewResult, ActivationTarget, GenerationTrigger, KnowledgeDelivery,
};
pub use asset::{CharacterCardInfo, PackInfo, PackSummaryInfo, ValidationIssue, ValidationReport};
pub use ids::{
    CharacterId, IdempotencyKey, KnowledgeSourceId, PackId, PlayerId, RoleId, SemanticVersion, Sha256Digest, StoryId,
};
pub use story::{
    CharacterCardRef, PackRef, StoryCommit, StoryContext, StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle,
    StorySummary,
};
pub use turn::{
    CommittedTurnInfo, PlayerContribution, Turn, TurnCancellation, TurnControl, TurnEvaluation, TurnEvent,
    TurnEventDeliveryError, TurnEventSink, TurnNumber, TurnRequest, TurnResult, TurnSegment, TurnStatus,
};

pub use change::Change;
pub use world::{WorldChange, WorldState};

pub use error::CoreError;
pub use message::{ChatMessage, ChatMessageRole};
