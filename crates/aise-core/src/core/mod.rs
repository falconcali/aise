mod activation;
mod asset;
mod bounded_text;
mod change;
pub mod error;
mod ids;
mod knowledge;
mod message;
mod narrative;
mod role;
mod story;
pub mod turn;
mod world;

#[cfg(test)]
pub(crate) mod tests;

pub use activation::ActivationRule;
pub use asset::{
    AssetConfig, AssetError, CharacterCard, CharacterCardSpec, CharacterMeta, PlayDefinition, StoryPack, StoryPackMeta,
    StoryPackProfile, StoryPackSpec,
};
pub use ids::{
    CharacterCardId, CharacterId, FactId, IdempotencyKey, MemoryId, NarrativeConditionId, NarrativeNodeId, PackId,
    PlayerId, RoleId, RumorId, SemanticVersion, Sha256Digest, StoryId,
};
pub use knowledge::{EntryOrigin, FactBody, FactEntry, Recallable, RetrievalHint, RumorBody, RumorEntry};
pub use narrative::{
    ActiveConstraint, ConstraintWindow, NarrativeCondition, NarrativeConfig, NarrativeConstraint, NarrativeDirection,
    NarrativeError, NarrativeEvidence, NarrativeGraph, NarrativeNode, NarrativePlan, NarrativeResolution,
    NarrativeState, NarrativeTransition, NodeStatus, SemanticQuery, Truth,
};
pub use role::{
    CharacterCardRef, CharacterProfile, DialogueExample, MemoryBody, MemoryEntry, RoleActor, RoleBody, RoleEntry,
};
pub use story::{
    PackRef, StoryAssetSpec, StoryCommit, StoryContext, StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle,
    StorySummary,
};
pub use turn::{
    PlayerContribution, Turn, TurnCancellation, TurnControl, TurnEvaluation, TurnEvent, TurnEventDeliveryError,
    TurnEventSink, TurnNumber, TurnRequest, TurnSegment, TurnStatus,
};

pub use bounded_text::BoundedText;

pub use change::Change;
pub use world::{WorldChange, WorldState};

pub use error::CoreError;
pub use message::{ChatMessage, ChatMessageRole};
