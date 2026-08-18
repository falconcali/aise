use crate::config::StructuredOutputMode;
use crate::domain::asset::ids::Sha256Digest;
use crate::llm::accounting::LlmCompletion;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;

pub use crate::config::{ModelStructuredOutputCapabilities, StructuredOutputConfig};

#[derive(Debug, Clone, Default)]
pub struct ProviderTransportCapabilities {
    pub encodable_modes: std::collections::BTreeSet<StructuredOutputMode>,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("no structured output mode is eligible for the configured provider/model")]
pub struct StructuredOutputUnsupported;

pub fn resolve_structured_output_mode(
    configured_modes: &[StructuredOutputMode],
    provider_capabilities: &ProviderTransportCapabilities,
) -> Result<StructuredOutputMode, StructuredOutputUnsupported> {
    StructuredOutputMode::PREFERENCE_ORDER
        .into_iter()
        .find(|mode| configured_modes.contains(mode) && provider_capabilities.encodable_modes.contains(mode))
        .ok_or(StructuredOutputUnsupported)
}

#[derive(Debug, Error, Clone)]
#[error("llm output contract violation in {contract_name}: {reason}")]
pub struct LlmOutputViolation {
    pub contract_name: &'static str,
    pub reason: String,
}

impl LlmOutputViolation {
    pub fn new(contract_name: &'static str, reason: impl Into<String>) -> Self {
        Self {
            contract_name,
            reason: reason.into(),
        }
    }
}

pub type LlmOutputValidatorFn<T> = dyn Fn(&T) -> Result<(), LlmOutputViolation> + Send + Sync;

pub struct LlmOutputContract<T> {
    pub name: &'static str,
    pub schema: Arc<serde_json::Value>,
    pub compact_prompt_shape: Arc<str>,
    pub validate: Arc<LlmOutputValidatorFn<T>>,
}

impl<T> Clone for LlmOutputContract<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            schema: self.schema.clone(),
            compact_prompt_shape: self.compact_prompt_shape.clone(),
            validate: self.validate.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CompletionOutputRequest {
    Text,
    Structured(ResolvedStructuredOutputRequest),
}

#[derive(Debug, Clone)]
pub struct ResolvedStructuredOutputRequest {
    pub contract_name: &'static str,
    pub schema: Arc<serde_json::Value>,
    pub schema_hash: Sha256Digest,
    pub mode: StructuredOutputMode,
}

pub struct StructuredLlmCompletion<T> {
    pub value: T,
    pub completion: LlmCompletion,
}

pub fn canonical_schema_hash(schema: &serde_json::Value) -> Sha256Digest {
    let canonical = canonicalize_json(schema);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    Sha256Digest::from_bytes(bytes)
}

fn canonicalize_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(&String, &serde_json::Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = String::from("{");
            for (index, (key, val)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("string keys serialize"));
                out.push(':');
                out.push_str(&canonicalize_json(val));
            }
            out.push('}');
            out
        }
        serde_json::Value::Array(items) => {
            let mut out = String::from("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonicalize_json(item));
            }
            out.push(']');
            out
        }
        other => serde_json::to_string(other).expect("scalar json serializes"),
    }
}

#[cfg(test)]
#[path = "tests/output_contract_tests.rs"]
mod tests;
