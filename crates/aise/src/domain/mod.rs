pub mod asset;
pub mod error;
pub mod ids;
pub mod knowledge;
pub mod narrative;
pub mod narrative_graph;
pub mod story_instance;
pub mod story_sequence;
pub mod text;
pub mod turn;
pub use error::DomainInputError;
pub use ids::{
    CharacterId, ConstraintId, DynamicRoleCandidatePool, EventId, FactId, MemoryId, RoleId, RoleIdAllocationError,
    RoleIdHighWater, RumorId, StoryId, StoryRevision, TurnKey, TurnNumber, TurnNumberError,
    allocate_dynamic_role_candidates,
};
pub use narrative::{
    EventKind, StoryContinuity, StoryEvent, StorySegment, StorySegmentOrigin, StorySummary, StoryTurn,
};
pub use story_instance::info::StoryInfo;
pub use story_instance::snapshot::StoryReadSnapshot;
pub use story_sequence::{StoryContinuityError, StorySequence};
pub use turn::{
    BaselineContext, CharacterDecision, CharacterThinkRequest, InterpretedPlayerContribution, NarrativeGraphStateIndex,
    PlayerContributionKind, PlayerContributionUnit, RetrievalPlan, RetrievedContext, RoleContextView, SnapshotLimits,
    StoryGeneratorOutput, StoryStateExtractionDto, WriterPlan, WriterStoryGoal,
};
