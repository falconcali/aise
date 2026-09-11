pub mod coordinator;
pub mod matcher;
pub mod preview;
pub mod scan_buffer;

pub use coordinator::{ActivationBodyLoader, KnowledgeActivationCoordinator};
pub use matcher::{ActivationMatch, ActivationMatcher};
pub use preview::{
    ActivationPreviewEntry, ActivationPreviewError, ActivationPreviewEvidence, ActivationPreviewLimits,
    ActivationPreviewResult, ActivationPreviewSpec, ActivationPreviewTarget, project_preview,
};
pub use scan_buffer::{ActivationScanBuffer, ScanFragment, ScanFragmentId, ScanFragmentKind};
