pub mod contracts;
pub mod engine;
pub mod rule;
pub mod scan;
pub mod state;

pub use contracts::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEntryMetadata, ActivationEvidence,
    ActivationIndexSnapshot, ActivationIndexSnapshotRef, ActivationMachineState, ActivationPatternKind,
    ActivationRejectionReason, ActivationRequest, ActivationResult, ActivationRuntimeLimits, ActivationWorkUsage,
    ExternalActivationSeed,
};
pub use engine::{ActivationError, KnowledgeActivationEngine};
pub use rule::{
    ActivationGroupKey, ActivationMatchRule, ActivationMode, ActivationPattern, ActivationRecursionRule,
    ActivationRuleValidationError, ActivationRuleVersion, ActivationScopeRule, ActivationSelectionRule,
    ActivationTimingRule, GenerationTrigger, KnowledgeActivationRule, SecondaryLogic, normalize_activation_literal,
};
pub use scan::{ActivationScanBuffer, ScanBufferError, ScanFragment, ScanFragmentId, ScanFragmentKind};
pub use state::{
    ActivationRunMode, ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta,
};
