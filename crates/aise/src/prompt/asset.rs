use crate::prompt::model::{AssetRef, AssetStatus, PromptKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptAssetManifest {
    pub asset_id: AssetRef,
    pub kind: PromptKind,
    pub source_path: String,
    #[serde(default)]
    pub input_schema_ref: Option<String>,
    #[serde(default)]
    pub output_contract_ref: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default = "default_active")]
    pub status: AssetStatus,
}

fn default_active() -> AssetStatus {
    AssetStatus::Active
}

#[derive(Debug, Clone)]
pub struct CompiledPromptAsset {
    pub manifest: PromptAssetManifest,
    pub source_anchor: String,
    pub resolved_hash: String,
    pub template_name: String,
}

pub fn compute_asset_hash(template_content: &str, manifest: &PromptAssetManifest) -> String {
    let mut hasher = Sha256::new();

    let normalized: String = template_content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    hasher.update(normalized.as_bytes());

    hasher.update(manifest.asset_id.as_ref().as_bytes());
    let kind = match manifest.kind {
        PromptKind::Text => b"\"text\"".as_slice(),
        PromptKind::Messages => b"\"messages\"".as_slice(),
        PromptKind::Fragment => b"\"fragment\"".as_slice(),
        PromptKind::FewShot => b"\"few_shot\"".as_slice(),
    };
    hasher.update(kind);

    if let Some(ref schema) = manifest.input_schema_ref {
        hasher.update(schema.as_bytes());
    }
    if let Some(ref contract) = manifest.output_contract_ref {
        hasher.update(contract.as_bytes());
    }

    let result = hasher.finalize();
    format!("sha256:{:x}", result)
}

#[cfg(test)]
#[path = "tests/asset_tests.rs"]
mod tests;
