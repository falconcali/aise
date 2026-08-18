pub mod entry;
pub mod fact;
pub mod hint;
pub mod memory;
pub mod query;
pub mod rumor;

pub use crate::domain::error::KnowledgeIdError;
pub use entry::{KnowledgeEntity, KnowledgeEntry};
pub use fact::{Proposition, WorldFact};
pub use hint::{RetrievalHint, RetrievalHintError, normalize_static_retrieval_hint};
pub use memory::MemoryEntry;
pub use query::{
    KnowledgeIdAllocation, KnowledgeIdHighWater, KnowledgeIndexMatch, KnowledgeKind, KnowledgeSequence,
    KnowledgeSource, KnowledgeSourceId, allocate_knowledge_ids, new_knowledge_source_id,
};
pub use rumor::{Claim, SharedRumor, TruthValue};
