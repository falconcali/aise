use crate::prompt::{
    error::PromptError,
    model::{AssetRef, PromptLineageNode, SlotId},
    pack::ResolvedPack,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptRenderOptions {
    pub pack_override: Option<String>,
}

impl PromptRenderOptions {
    pub fn with_pack_override(pack_override: Option<String>) -> Self {
        Self { pack_override }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSlot {
    pub slot: SlotId,
    pub pack: String,
    pub root: PromptLineageNode,
    pub selection_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedSelection {
    pub slot: SlotId,
    pub pack: String,
    pub asset_id: AssetRef,
    pub selection_reason: String,
}

#[derive(Debug, Clone)]
pub struct PromptResolver {
    pub default_pack: String,
}

impl PromptResolver {
    pub fn select_pack(&self, options: &PromptRenderOptions) -> (String, String) {
        if let Some(ref pack_override) = options.pack_override {
            return (pack_override.clone(), "explicit_override".to_string());
        }

        (self.default_pack.clone(), "default".to_string())
    }

    pub(crate) fn resolve_selection(
        &self,
        slot_id: &str,
        options: &PromptRenderOptions,
        compiled_packs: &HashMap<String, ResolvedPack>,
    ) -> Result<ResolvedSelection, PromptError> {
        let (pack_name, reason) = self.select_pack(options);

        let resolved = compiled_packs
            .get(&pack_name)
            .ok_or_else(|| PromptError::PackNotFound(pack_name.clone()))?;

        let asset_ref = resolved.resolved_slots.get(slot_id).ok_or_else(|| {
            PromptError::SlotNotFound(format!("slot `{}` not found in pack `{}`", slot_id, pack_name))
        })?;

        let asset_id = parse_asset_ref(asset_ref)?;

        Ok(ResolvedSelection {
            slot: slot_id.into(),
            pack: pack_name,
            asset_id,
            selection_reason: reason,
        })
    }
}

pub fn parse_asset_ref(asset_ref: &str) -> Result<AssetRef, PromptError> {
    if asset_ref.trim().is_empty() {
        return Err(PromptError::AssetNotFound("asset ref cannot be empty".to_string()));
    }

    if asset_ref.contains('@') {
        return Err(PromptError::AssetNotFound(format!(
            "asset ref `{}` must be a bare `asset_id` without a revision suffix",
            asset_ref
        )));
    }

    Ok(asset_ref.into())
}

#[cfg(test)]
#[path = "tests/resolver_tests.rs"]
mod tests;
