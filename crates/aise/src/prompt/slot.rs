use crate::prompt::{
    error::PromptError,
    model::{PromptKind, SlotId},
};
use serde::Deserialize;
use std::collections::{HashMap, hash_map::Entry};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    String,
    Number,
    Bool,
    Array,
    Object,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarSpec {
    pub name: String,
    pub var_type: VarType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputContract {
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSpec {
    pub slot_id: SlotId,
    pub allowed_kinds: Vec<PromptKind>,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub output_contract_required: bool,
    #[serde(default)]
    pub optimizable: bool,
    #[serde(default)]
    pub allow_child_render: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub vars: Vec<VarSpec>,
    #[serde(default)]
    pub output_contract: Option<OutputContract>,
}

fn default_true() -> bool {
    true
}

impl SlotSpec {
    pub fn accepts_kind(&self, kind: &PromptKind) -> bool {
        self.allowed_kinds.contains(kind)
    }

    fn validate_supported_options(&self) -> Result<(), PromptError> {
        let mut unsupported = Vec::new();

        if self.structured_output {
            unsupported.push("structured_output");
        }
        if self.output_contract_required {
            unsupported.push("output_contract_required");
        }
        if self.optimizable {
            unsupported.push("optimizable");
        }
        if self.allow_child_render {
            unsupported.push("allow_child_render");
        }

        if !unsupported.is_empty() {
            return Err(PromptError::CatalogLoad(format!(
                "slot `{}` uses unsupported options: {}",
                self.slot_id,
                unsupported.join(", ")
            )));
        }

        Ok(())
    }
}

pub type SlotRegistry = HashMap<SlotId, SlotSpec>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotsFile {
    slots: Vec<SlotSpec>,
}

pub fn parse_slots_yaml(content: &str) -> Result<SlotRegistry, PromptError> {
    let file: SlotsFile = serde_yaml::from_str(content).map_err(|error| PromptError::CatalogLoad(error.to_string()))?;
    let mut registry = SlotRegistry::new();
    for spec in file.slots {
        spec.validate_supported_options()?;

        let slot_id = spec.slot_id.clone();
        match registry.entry(slot_id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(spec);
            }
            Entry::Occupied(_) => {
                return Err(PromptError::CatalogLoad(format!(
                    "duplicate slot_id `{}` in slots.yaml",
                    slot_id
                )));
            }
        }
    }
    Ok(registry)
}

#[cfg(test)]
#[path = "tests/slot_tests.rs"]
mod tests;
