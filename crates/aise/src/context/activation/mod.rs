pub mod coordinator;
pub mod index;
pub mod preview;
pub mod preview_service;
pub mod seed_provider;

pub use coordinator::{ActivationRunOutcome, ActivationRunSpec, KnowledgeActivationCoordinator};
pub use index::{
    ActivationOverlayIndex, FragmentMatchCache, FragmentMatchCacheKey, FragmentMatchCacheValue, FrozenPackIndexCache,
    InMemoryFragmentMatchCache, MATCHER_VERSION, build_overlay_index, compose_index_snapshot,
};
pub use preview::{
    ActivationPreviewEntry, ActivationPreviewError, ActivationPreviewEvidence, ActivationPreviewLimits,
    ActivationPreviewResult, ActivationPreviewSpec, ActivationPreviewTarget, project_preview,
};
pub use preview_service::{ActivationPreviewServiceConfig, KnowledgeActivationPreviewService};
pub use seed_provider::{ActivationSeedProvider, ActivationSeedRequest};
