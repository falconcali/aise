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
    ActivatedKnowledge, EntryOrigin, FactBody, FactChangeOp, FactEntry, KnowledgeChange, Recallable, RetrievalHint,
    RoleMemoryChangeOp, RumorBody, RumorChangeOp, RumorEntry,
};
pub use narrative::{
    ActiveConstraint, ConstraintWindow, NarrativeCondition, NarrativeConstraint, NarrativeDirection, NarrativeError,
    NarrativeEvidence, NarrativeGraph, NarrativeNode, NarrativePlan, NarrativeResolution, NarrativeState,
    NarrativeTransition, NodeStatus, SemanticQuery, Truth,
};
pub(crate) use role::validate_runtime_roles;
pub use role::{
    CharacterCard, CharacterCardSpec, CharacterDecision, CharacterMeta, CharacterProfile, CharacterThinkRequest,
    DialogueExample, MAX_ALIASES_PER_ROLE, MAX_MEMORIES_PER_ROLE, MAX_OPENING_BYTES, MAX_ROLE_CATALOG_BYTES,
    MAX_ROLE_STATE_BYTES, MAX_ROLES_PER_STORY, MemoryBody, MemoryEntry, RoleEntry, RoleSpec, SeedRole,
};
pub use story::{
    PackRef, PlayDefinition, StoryCommit, StoryContext, StoryError, StoryInstanceInfo, StoryLifeCycle, StoryPack,
    StoryPackMeta, StoryPackProfile, StoryPackSpec, StorySummary,
};
pub(crate) use story::{StoryCreateBundle, StoryInstanceSpec, StoryPackSnapshot};
pub use turn::{
    PlayerContribution, Turn, TurnCancellation, TurnCheckEvaluation, TurnChoiceEvaluation, TurnControl, TurnEvaluation,
    TurnEvent, TurnEventSink, TurnNumber, TurnRequest, TurnScoreEvaluation, TurnSegment, TurnStatus,
};

pub use bounded_text::BoundedText;

pub use change::Change;

pub use error::CoreError;
pub use message::{ChatMessage, ChatMessageRole};
