use crate::prompt::model::PromptKind;

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("{0}")]
    CatalogLoad(String),

    #[error("PromptCatalog not loaded")]
    CatalogNotLoaded,

    #[error("slot not found: {0}")]
    SlotNotFound(String),

    #[error("pack not found: {0}")]
    PackNotFound(String),

    #[error("asset not found: {0}")]
    AssetNotFound(String),

    #[error("kind mismatch on slot `{slot}`: expected {expected:?}, got {actual}")]
    KindMismatch {
        slot: String,
        expected: Vec<PromptKind>,
        actual: PromptKind,
    },

    #[error("render error: {0}")]
    RenderError(String),

    #[error("schema validation failed: {0}")]
    SchemaValidationFailed(String),

    #[error("inheritance cycle or depth exceeded: {0}")]
    InheritanceCycleOrDepthExceeded(String),

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("child render failed: {0}")]
    ChildRenderFailed(String),

    #[error("output contract violated on slot `{slot}`: {reason}")]
    OutputContractViolation { slot: String, reason: String },

    #[error("prompt profile already registered: {0}")]
    DuplicateProfileRegistration(String),

    #[error("prompt profile is not registered: {0}")]
    ProfileNotRegistered(String),

    #[error("prompt profile `{profile}` reuses slot `{slot}` across CSI/RC/FTI")]
    DuplicateLayerSlot { profile: String, slot: String },

    #[error("prompt layer `{layer}` for profile `{profile}` must render as text")]
    LayerMustRenderAsText { profile: String, layer: String },

    #[error("prompt trust boundary violated: {0}")]
    TrustBoundaryViolation(String),
}
