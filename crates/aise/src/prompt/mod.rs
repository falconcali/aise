mod asset;
mod catalog;
mod composition;
mod error;
mod loader;
mod metadata;
mod model;
mod pack;
mod policy;
pub mod profile;
mod renderer;
mod renderer_helpers;
mod resolver;
mod section_extractor;
mod slot;
pub mod trusted_prompt_source;
mod validator;

pub use asset::{CompiledPromptAsset, PromptAssetManifest, compute_asset_hash};
pub use catalog::PromptCatalog;
pub use composition::{
    CoreSystemInstruction, FinalTaskInstruction, PromptComposer, PromptComposition, PromptCompositionInput,
    PromptCompositionMetadata, PromptLayer, ProviderPromptEncoder, RuntimeContextMessage, RuntimePromptVars,
    TrustedPromptVars,
};
pub use error::PromptError;
pub use loader::{load_catalog, load_catalog_bundle};
pub use metadata::PromptMetadata;
pub use model::{
    AssetRef, AssetStatus, PromptKind, PromptLineageNode, PromptMessage, PromptRole, RenderedPrompt, SlotId,
};
pub use pack::{PromptPack, ResolvedPack, resolve_pack};
pub use policy::{PreamblePosition, PromptPolicy};
pub use profile::{PromptProfile, PromptProfileAssets, PromptProfileRegistry};
pub use renderer::PromptRenderer;
pub use renderer_helpers::{
    render_required_slot, render_required_slot_with_options, try_render_slot, try_render_slot_with_options,
};
pub use resolver::{PromptRenderOptions, PromptResolver, ResolvedSlot};
pub use slot::{OutputContract, SlotRegistry, SlotSpec, VarSpec, VarType, parse_slots_yaml};
pub use trusted_prompt_source::{CatalogPromptSource, TrustedPromptSource};
pub use validator::{validate_input_vars, validate_output_contract};
