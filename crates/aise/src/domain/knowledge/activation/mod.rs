pub mod contracts;
pub mod engine;
pub mod evidence;
pub mod index;
pub mod provider;
pub mod rule;
pub mod scan;
pub mod state;

pub use contracts::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEntryBody, ActivationEntryMetadata, ActivationEvidence,
    ActivationIndexMetadata, ActivationIndexSnapshot, ActivationIndexSnapshotRef, ActivationMachineState,
    ActivationMacroValues, ActivationPatternKind, ActivationRecursionInput, ActivationRejectionReason,
    ActivationRequest, ActivationResult, ActivationRoundOutcome, ActivationRuntimeLimits, ActivationWorkUsage,
    ExternalActivationSeed, LoadedActivationEntry,
};
pub use engine::{ActivationError, ActivationStoreFailure, KnowledgeActivationEngine, KnowledgeActivationSession};
pub use evidence::{ActivationRejectionCounts, bounded_evidence_digest};
pub use index::{
    ActivationFragmentMatches, ActivationIndexLimits, CompiledPatternRef, FragmentPatternMatch, FrozenLiteralIndex,
    FrozenPackIndex, FrozenPackIndexKey, FrozenRegexSet, IndexedActivationPattern, MATCHER_VERSION,
    build_frozen_pack_index, macro_digest,
};
pub use provider::{
    ActivationProviderError, ActivationSeedProvider, ActivationSeedRequest, ProviderActivationCandidate,
};
pub use rule::{
    ActivationBudgetClass, ActivationGroupKey, ActivationMatchRule, ActivationMode, ActivationPattern,
    ActivationRecursionRule, ActivationRuleLimits, ActivationRuleValidationError, ActivationRuleVersion,
    ActivationScopeRule, ActivationSelectionRule, ActivationTimingRule, GenerationTrigger, KnowledgeActivationRule,
    SecondaryLogic, compile_activation_regex, normalize_activation_literal,
};
pub use scan::{ActivationScanBuffer, ScanBufferError, ScanFragment, ScanFragmentId, ScanFragmentKind};
pub use state::{
    ActivationRunMode, ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta,
};
