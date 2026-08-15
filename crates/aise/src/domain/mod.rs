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
pub use ids::{CharacterId, ConstraintId, EventId, FactId, MemoryId, RoleId, RumorId, StoryId, StoryRevision, TurnId};
pub use narrative::{
    EventKind, StoryContinuity, StoryEvent, StorySegment, StorySegmentOrigin, StorySummary, StoryTurn,
};
pub use story_instance::info::StoryInfo;
pub use story_instance::snapshot::StoryReadSnapshot;
pub use story_instance::state::CurrentScene;
pub use story_sequence::{StoryContinuityError, StorySequence};
pub use turn::{
    BaselineContext, CharacterDecision, CharacterThinkRequest, ContextItem, NarrativeGraphStateIndex,
    RetrievalAudience, RetrievalPlan, RetrievalRequest, RetrievalSignals, RetrievedContext, RoleContextView,
    SnapshotLimits, StoryGeneratorOutput, StoryStateExtractorOutput, WriterPlan, WriterStoryGoal,
};
