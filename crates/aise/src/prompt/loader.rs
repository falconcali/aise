use crate::prompt::{
    asset::{CompiledPromptAsset, PromptAssetManifest, compute_asset_hash},
    catalog::{PromptCatalog, PromptCatalogParts},
    error::PromptError,
    model::{AssetRef, AssetStatus, SlotId},
    pack::{PromptPack, resolve_pack},
    policy::PromptPolicy,
    renderer::PromptRenderer,
    resolver::{PromptResolver, parse_asset_ref},
    section_extractor::{PromptAssetSection, extract_asset_sections},
    slot::{SlotRegistry, parse_slots_yaml},
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    path::Path,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexFile {
    assets: Vec<PromptAssetManifest>,
    #[serde(default)]
    packs: Vec<PromptPack>,
    resolver: ResolverConfig,
    #[serde(default)]
    policies: Vec<PromptPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolverConfig {
    default_pack: String,
}

pub fn load_catalog(dir: &Path) -> Result<PromptCatalog, PromptError> {
    let index_path = dir.join("index.yaml");
    let index_content = std::fs::read_to_string(&index_path)
        .map_err(|error| PromptError::CatalogLoad(format!("failed to read {}: {}", index_path.display(), error)))?;
    let index: IndexFile = serde_yaml::from_str(&index_content)
        .map_err(|error| PromptError::CatalogLoad(format!("failed to parse {}: {}", index_path.display(), error)))?;

    let slots_path = dir.join("slots.yaml");
    let slots_content = std::fs::read_to_string(&slots_path)
        .map_err(|error| PromptError::CatalogLoad(format!("failed to read {}: {}", slots_path.display(), error)))?;
    let slots = parse_slots_yaml(&slots_content)
        .map_err(|error| PromptError::CatalogLoad(format!("failed to parse {}: {}", slots_path.display(), error)))?;

    validate_policy_support(&index.policies)?;

    let mut renderer = PromptRenderer::new();
    let mut assets: HashMap<AssetRef, CompiledPromptAsset> = HashMap::new();
    let mut asset_slot_ids: HashMap<AssetRef, Vec<SlotId>> = HashMap::new();

    for (source_path, manifests) in manifests_by_source_path(index.assets) {
        let template_path = dir.join(&source_path);
        let template_content = std::fs::read_to_string(&template_path).map_err(|error| {
            PromptError::CatalogLoad(format!("failed to read template {}: {}", template_path.display(), error))
        })?;
        let sections = extract_asset_sections(&source_path, &template_content).map_err(|error| {
            PromptError::CatalogLoad(format!("failed to parse sections in {}: {}", template_path.display(), error))
        })?;

        validate_sections_for_source(&source_path, &manifests, &sections, &slots)?;

        for manifest in manifests {
            validate_asset_status(&manifest)?;
            let section = sections.get(&manifest.asset_id).expect("section existence validated");
            let asset_ref = manifest.asset_id.clone();
            let computed_hash = compute_asset_hash(&section.body, &manifest);
            let resolved_hash = if let Some(ref declared_hash) = manifest.hash {
                if declared_hash != &computed_hash {
                    return Err(PromptError::CatalogLoad(format!(
                        "hash mismatch for asset `{}`: declared={}, computed={}",
                        manifest.asset_id, declared_hash, computed_hash
                    )));
                }
                declared_hash.clone()
            } else {
                computed_hash
            };
            let template_name = manifest.asset_id.to_string();
            renderer.add_template(&template_name, &section.body)?;

            if assets.contains_key(&asset_ref) {
                return Err(PromptError::CatalogLoad(format!(
                    "duplicate asset manifest `{}` declared in index.yaml",
                    asset_ref
                )));
            }

            assets.insert(
                asset_ref.clone(),
                CompiledPromptAsset {
                    manifest,
                    source_anchor: section.source_anchor.clone(),
                    resolved_hash,
                    template_name,
                },
            );
            asset_slot_ids.insert(asset_ref, section.slot_ids.clone());
        }
    }

    let mut raw_packs: HashMap<String, PromptPack> = HashMap::new();
    for pack in index.packs {
        let pack_name = pack.name.clone();
        match raw_packs.entry(pack_name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(pack);
            }
            Entry::Occupied(_) => {
                return Err(PromptError::CatalogLoad(format!(
                    "duplicate pack name `{}` declared in index.yaml",
                    pack_name
                )));
            }
        }
    }

    if !raw_packs.contains_key(&index.resolver.default_pack) {
        return Err(PromptError::CatalogLoad(format!(
            "resolver default_pack `{}` does not exist",
            index.resolver.default_pack
        )));
    }

    let mut resolved_packs = HashMap::new();
    for pack_name in raw_packs.keys() {
        let resolved = resolve_pack(pack_name, &raw_packs)?;
        resolved_packs.insert(pack_name.clone(), resolved);
    }

    for (pack_name, resolved) in &resolved_packs {
        for (slot_id, asset_ref) in &resolved.resolved_slots {
            let asset_id = parse_asset_ref(asset_ref)?;

            if !assets.contains_key(&asset_id) {
                return Err(PromptError::CatalogLoad(format!(
                    "pack `{}` references unknown asset `{}` for slot `{}`",
                    pack_name, asset_ref, slot_id
                )));
            }

            let declared_slots = asset_slot_ids
                .get(&asset_id)
                .expect("slot declarations exist for every compiled asset");
            if !declared_slots.iter().any(|declared_slot| declared_slot == slot_id) {
                let source_anchor = &assets[&asset_id].source_anchor;
                return Err(PromptError::CatalogLoad(format!(
                    "pack `{}` binds slot `{}` to asset `{}`, but section `{}` does not declare that slot",
                    pack_name, slot_id, asset_id, source_anchor
                )));
            }

            if let Some(slot_spec) = slots.get(slot_id) {
                let asset = &assets[&asset_id];
                if !slot_spec.accepts_kind(&asset.manifest.kind) {
                    return Err(PromptError::KindMismatch {
                        slot: slot_id.to_string(),
                        expected: slot_spec.allowed_kinds.clone(),
                        actual: asset.manifest.kind.clone(),
                    });
                }
            }
        }

        for (slot_id, slot_spec) in &slots {
            if slot_spec.required && !resolved.resolved_slots.contains_key(slot_id) {
                return Err(PromptError::CatalogLoad(format!(
                    "pack `{}` does not cover required slot `{}`",
                    pack_name, slot_id
                )));
            }
        }
    }

    let resolver = PromptResolver {
        default_pack: index.resolver.default_pack,
    };

    Ok(PromptCatalog::from_parts(PromptCatalogParts {
        assets,
        slots,
        packs: resolved_packs,
        raw_packs,
        resolver,
        policies: index.policies,
        loaded_at: chrono::Utc::now(),
        renderer,
    }))
}

fn validate_policy_support(policies: &[PromptPolicy]) -> Result<(), PromptError> {
    for policy in policies {
        match policy {
            PromptPolicy::Preamble { .. } => {}
            PromptPolicy::RuntimeGuard { name, .. } => {
                return Err(PromptError::CatalogLoad(format!(
                    "policy `{}` uses unsupported type `runtime_guard`; only `preamble` is currently supported",
                    name
                )));
            }
            PromptPolicy::PostValidator { name } => {
                return Err(PromptError::CatalogLoad(format!(
                    "policy `{}` uses unsupported type `post_validator`; only `preamble` is currently supported",
                    name
                )));
            }
        }
    }

    Ok(())
}

fn validate_asset_status(manifest: &PromptAssetManifest) -> Result<(), PromptError> {
    if manifest.status != AssetStatus::Active {
        return Err(PromptError::CatalogLoad(format!(
            "asset `{}` uses unsupported status `{}`; only `active` assets can be loaded",
            manifest.asset_id, manifest.status
        )));
    }

    Ok(())
}

fn manifests_by_source_path(manifests: Vec<PromptAssetManifest>) -> HashMap<String, Vec<PromptAssetManifest>> {
    let mut grouped: HashMap<String, Vec<PromptAssetManifest>> = HashMap::new();
    for manifest in manifests {
        grouped.entry(manifest.source_path.clone()).or_default().push(manifest);
    }
    grouped
}

fn validate_sections_for_source(
    source_path: &str,
    manifests: &[PromptAssetManifest],
    sections: &HashMap<AssetRef, PromptAssetSection>,
    slots: &SlotRegistry,
) -> Result<(), PromptError> {
    let manifest_ids: HashSet<&AssetRef> = manifests.iter().map(|manifest| &manifest.asset_id).collect();

    for manifest in manifests {
        if !sections.contains_key(&manifest.asset_id) {
            return Err(PromptError::CatalogLoad(format!(
                "asset `{}` declared in index.yaml does not have a matching section in `{}`",
                manifest.asset_id, source_path
            )));
        }
    }

    for section in sections.values() {
        if !manifest_ids.contains(&section.asset_id) {
            return Err(PromptError::CatalogLoad(format!(
                "section `{}` in `{}` is not registered in index.yaml",
                section.asset_id, source_path
            )));
        }

        validate_section_slot_compatibility(section, slots)?;
    }

    Ok(())
}

fn validate_section_slot_compatibility(section: &PromptAssetSection, slots: &SlotRegistry) -> Result<(), PromptError> {
    let Some(first_slot_id) = section.slot_ids.first() else {
        return Err(PromptError::CatalogLoad(format!(
            "section `{}` must declare at least one slot id",
            section.asset_id
        )));
    };

    let first_slot = slots.get(first_slot_id).ok_or_else(|| {
        PromptError::CatalogLoad(format!(
            "section `{}` references unknown slot `{}` in `{}`",
            section.asset_id, first_slot_id, section.source_anchor
        ))
    })?;

    for slot_id in section.slot_ids.iter().skip(1) {
        let slot = slots.get(slot_id).ok_or_else(|| {
            PromptError::CatalogLoad(format!(
                "section `{}` references unknown slot `{}` in `{}`",
                section.asset_id, slot_id, section.source_anchor
            ))
        })?;

        if slot.allowed_kinds != first_slot.allowed_kinds {
            return Err(PromptError::CatalogLoad(format!(
                "section `{}` declares multiple slots with different allowed_kinds: `{}` vs `{}`",
                section.asset_id, first_slot_id, slot_id
            )));
        }

        if slot.vars != first_slot.vars {
            return Err(PromptError::CatalogLoad(format!(
                "section `{}` declares multiple slots with different variable requirements: `{}` vs `{}`",
                section.asset_id, first_slot_id, slot_id
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/loader_tests.rs"]
mod tests;
