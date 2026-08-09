use crate::prompt::{
    asset::CompiledPromptAsset,
    error::PromptError,
    metadata::PromptMetadata,
    model::{AssetRef, PromptKind, PromptLineageNode, RenderedPrompt},
    pack::{PromptPack, ResolvedPack},
    policy::PromptPolicy,
    renderer::PromptRenderer,
    resolver::{PromptRenderOptions, PromptResolver, ResolvedSlot},
    slot::SlotRegistry,
    validator::{validate_input_vars, validate_output_contract},
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub struct PromptCatalog {
    assets: HashMap<AssetRef, CompiledPromptAsset>,
    slots: SlotRegistry,
    packs: HashMap<String, ResolvedPack>,
    raw_packs: HashMap<String, PromptPack>,
    resolver: PromptResolver,
    policies: Vec<PromptPolicy>,
    loaded_at: DateTime<Utc>,
    renderer: PromptRenderer,
}

pub(crate) struct PromptCatalogParts {
    pub assets: HashMap<AssetRef, CompiledPromptAsset>,
    pub slots: SlotRegistry,
    pub packs: HashMap<String, ResolvedPack>,
    pub raw_packs: HashMap<String, PromptPack>,
    pub resolver: PromptResolver,
    pub policies: Vec<PromptPolicy>,
    pub loaded_at: DateTime<Utc>,
    pub renderer: PromptRenderer,
}

impl PromptCatalog {
    pub(crate) fn from_parts(parts: PromptCatalogParts) -> Self {
        let PromptCatalogParts {
            assets,
            slots,
            packs,
            raw_packs,
            resolver,
            policies,
            loaded_at,
            renderer,
        } = parts;
        Self {
            assets,
            slots,
            packs,
            raw_packs,
            resolver,
            policies,
            loaded_at,
            renderer,
        }
    }

    pub fn asset(&self, asset_ref: &str) -> Option<&CompiledPromptAsset> {
        self.assets.get(asset_ref)
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn has_pack(&self, pack_name: &str) -> bool {
        self.packs.contains_key(pack_name)
    }

    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }

    pub fn raw_pack(&self, pack_name: &str) -> Option<&PromptPack> {
        self.raw_packs.get(pack_name)
    }

    pub fn loaded_at(&self) -> &DateTime<Utc> {
        &self.loaded_at
    }

    pub fn resolve(&self, slot_id: &str, options: &PromptRenderOptions) -> Result<ResolvedSlot, PromptError> {
        let selection = self.resolver.resolve_selection(slot_id, options, &self.packs)?;
        let hash = self
            .assets
            .get(&selection.asset_id)
            .map(|compiled| compiled.resolved_hash.clone());

        Ok(ResolvedSlot {
            slot: selection.slot.clone(),
            pack: selection.pack,
            root: PromptLineageNode {
                slot: selection.slot,
                asset_id: selection.asset_id,
                hash,
            },
            selection_reason: selection.selection_reason,
        })
    }

    pub fn render_slot(
        &self,
        slot_id: &str,
        vars: &HashMap<String, Value>,
        options: &PromptRenderOptions,
    ) -> Result<(RenderedPrompt, PromptMetadata), PromptError> {
        let start = std::time::Instant::now();
        let slot_spec = self.slots.get(slot_id);

        let input_validated = if let Some(spec) = slot_spec {
            validate_input_vars(spec, vars)?;
            true
        } else {
            false
        };

        let resolved = self.resolve(slot_id, options)?;
        let asset_ref = resolved.root.asset_id.clone();

        let compiled = self
            .assets
            .get(&asset_ref)
            .ok_or_else(|| PromptError::AssetNotFound(asset_ref.to_string()))?;

        if let Some(spec) = slot_spec {
            if !spec.accepts_kind(&compiled.manifest.kind) {
                return Err(PromptError::KindMismatch {
                    slot: slot_id.to_string(),
                    expected: spec.allowed_kinds.clone(),
                    actual: compiled.manifest.kind.clone(),
                });
            }
        }

        let mut rendered = self
            .renderer
            .render_prompt(&compiled.template_name, &compiled.manifest.kind, vars)?;

        let mut applied_policies = Vec::new();
        if let RenderedPrompt::Text(ref mut text) = rendered {
            for policy in &self.policies {
                if let Some(modified) = policy.apply_to_text(text) {
                    *text = modified;
                    applied_policies.push(policy.name().to_string());
                }
            }
        }

        let output_contract_validated = if let Some(spec) = slot_spec {
            if let Some(ref contract) = spec.output_contract {
                if let RenderedPrompt::Text(ref text) = rendered {
                    validate_output_contract(slot_id, contract, text)?;
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        let metadata = PromptMetadata {
            slot: resolved.slot.clone(),
            pack: resolved.pack,
            root: resolved.root,
            rendered_assets: vec![asset_ref],
            applied_policies,
            selection_reason: resolved.selection_reason,
            render_duration_ms: start.elapsed().as_millis() as u64,
            input_validated,
            output_contract_validated,
        };

        Ok((rendered, metadata))
    }

    pub fn render_text(
        &self,
        slot_id: &str,
        vars: &HashMap<String, Value>,
        options: &PromptRenderOptions,
    ) -> Result<String, PromptError> {
        let (rendered, _) = self.render_slot(slot_id, vars, options)?;
        match rendered {
            RenderedPrompt::Text(text) => Ok(normalize_rendered_text(&text)),
            RenderedPrompt::Messages(_) => Err(PromptError::KindMismatch {
                slot: slot_id.to_string(),
                expected: vec![PromptKind::Text, PromptKind::Fragment],
                actual: PromptKind::Messages,
            }),
        }
    }

    pub fn render_asset_text(&self, asset_ref: &str) -> Result<String, PromptError> {
        let compiled = self
            .assets
            .get(asset_ref)
            .ok_or_else(|| PromptError::AssetNotFound(asset_ref.to_string()))?;
        let vars = HashMap::new();
        let rendered = self
            .renderer
            .render_prompt(&compiled.template_name, &compiled.manifest.kind, &vars)?;
        match rendered {
            RenderedPrompt::Text(text) => Ok(normalize_rendered_text(&text)),
            RenderedPrompt::Messages(_) => Err(PromptError::KindMismatch {
                slot: asset_ref.to_string(),
                expected: vec![PromptKind::Text, PromptKind::Fragment],
                actual: PromptKind::Messages,
            }),
        }
    }
}

fn normalize_rendered_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
#[path = "tests/catalog_tests.rs"]
mod tests;
