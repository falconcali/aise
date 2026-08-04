use crate::prompt::{
    error::PromptError,
    model::{AssetRef, SlotId},
};
use serde::Deserialize;
use std::collections::HashMap;

const MAX_INHERITANCE_DEPTH: usize = 4;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptPack {
    pub name: String,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub slots: HashMap<SlotId, AssetRef>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPack {
    pub name: String,
    pub resolved_slots: HashMap<SlotId, AssetRef>,
    pub extends_chain: Vec<String>,
}

pub fn resolve_pack(name: &str, all_packs: &HashMap<String, PromptPack>) -> Result<ResolvedPack, PromptError> {
    let mut chain: Vec<String> = Vec::new();
    let mut current = name.to_string();

    loop {
        if chain.contains(&current) {
            return Err(PromptError::InheritanceCycleOrDepthExceeded(format!(
                "cycle detected: {} already in chain {:?}",
                current, chain
            )));
        }
        if chain.len() >= MAX_INHERITANCE_DEPTH {
            return Err(PromptError::InheritanceCycleOrDepthExceeded(format!(
                "max depth {} exceeded at pack `{}`",
                MAX_INHERITANCE_DEPTH, current
            )));
        }

        let pack = all_packs
            .get(&current)
            .ok_or_else(|| PromptError::PackNotFound(current.clone()))?;

        chain.push(current.clone());

        match &pack.extends {
            Some(parent) => current = parent.clone(),
            None => break,
        }
    }

    let mut resolved_slots = HashMap::new();
    for pack_name in chain.iter().rev() {
        if let Some(pack) = all_packs.get(pack_name) {
            resolved_slots.extend(pack.slots.clone());
        }
    }

    Ok(ResolvedPack {
        name: name.to_string(),
        resolved_slots,
        extends_chain: chain,
    })
}

#[cfg(test)]
#[path = "tests/pack_tests.rs"]
mod tests;
