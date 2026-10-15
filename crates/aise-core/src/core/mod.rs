mod activation;
mod bounded_text;
mod change;
mod digest;
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

pub use activation::{
    ActivatedEntry, ActivationCandidate, ActivationClass, ActivationConfig, ActivationEvidence, ActivationGroupKey,
    ActivationKey, ActivationMatchRule, ActivationMode, ActivationOutcome, ActivationPatternKind,
    ActivationProbability, ActivationRecursionRule, ActivationRejection, ActivationRule, ActivationSelectionRule,
    ActivationStopReason, ActivationTurn, ScanBuffer, ScanFragment, ScanFragmentKind, SecondaryLogic, activate,
};
pub use ids::{
    CharacterCardId, CharacterId, FactId, IdempotencyKey, MemoryId, NarrativeConditionId, NarrativeNodeId, PackId,
    PlayerId, RoleId, RumorId, SemanticVersion, Sha256Digest, StoryId,
};
pub use knowledge::{
    ActivatedKnowledge, EntryOrigin, FactBody, FactEntry, Recallable, RetrievalHint, RumorBody, RumorEntry,
};
pub use narrative::{
    ActiveConstraint, ConstraintWindow, NarrativeCondition, NarrativeConstraint, NarrativeDirection, NarrativeError,
    NarrativeEvidence, NarrativeGraph, NarrativeNode, NarrativePlan, NarrativeResolution, NarrativeState,
    NarrativeTransition, NodeStatus, SemanticQuery, Truth,
};
pub use role::{
    CharacterCard, CharacterCardRef, CharacterCardSpec, CharacterDecision, CharacterMeta, CharacterProfile,
    CharacterThinkRequest, DialogueExample, MemoryBody, MemoryEntry, PlayerRoleContext, ResolvedRole, RoleActor,
    RoleBody, RoleEntry,
};
pub use story::{
    MAX_ALIASES_PER_ROLE, MAX_ROLE_ASSETS_BYTES, MAX_ROLE_CATALOG_BYTES, MAX_ROLES_PER_PACK,
    MAX_SEED_MEMORIES_PER_ROLE, PackRef, PlayDefinition, StoryAssetSpec, StoryCommit, StoryContext, StoryError,
    StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle, StoryPack, StoryPackMeta, StoryPackProfile, StoryPackSpec,
    StorySummary,
};
pub use turn::{
    PlayerContribution, Turn, TurnCancellation, TurnControl, TurnEvaluation, TurnEvent, TurnEventSink, TurnNumber,
    TurnRequest, TurnSegment, TurnStatus,
};

pub use bounded_text::BoundedText;

pub use change::Change;
pub use world::{WorldChange, WorldState};

pub use error::CoreError;
pub use message::{ChatMessage, ChatMessageRole};
