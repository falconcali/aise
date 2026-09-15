pub mod coordinator;
pub mod fragment_cache;
pub mod index;
pub mod preview;
pub mod preview_service;
pub(crate) mod scan_builder;

pub use coordinator::{ActivationRunOutcome, ActivationRunSpec, KnowledgeActivationCoordinator};
pub use fragment_cache::{
    FragmentMatchCache, FragmentMatchCacheKey, FragmentMatchCacheStats, FragmentMatchCacheValue, FragmentPatternMatch,
    LruFragmentMatchCache,
};
pub use index::{
    ActivationOverlayIndex, FrozenPackIndexCache, MATCHER_VERSION, build_overlay_index, compose_index_snapshot,
};
pub use preview::{
    ActivationPreviewEntry, ActivationPreviewError, ActivationPreviewEvidence, ActivationPreviewLimits,
    ActivationPreviewResult, ActivationPreviewSpec, ActivationPreviewTarget, project_preview,
};
pub use preview_service::{ActivationPreviewServiceConfig, KnowledgeActivationPreviewService};
