use crate::prompt::{catalog::PromptCatalog, error::PromptError, resolver::PromptRenderOptions};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub fn try_render_slot(
    catalog: Option<&Arc<PromptCatalog>>,
    slot_id: &str,
    vars: &HashMap<String, Value>,
) -> Option<String> {
    try_render_slot_with_options(catalog, slot_id, vars, &PromptRenderOptions::default())
}

pub fn try_render_slot_with_options(
    catalog: Option<&Arc<PromptCatalog>>,
    slot_id: &str,
    vars: &HashMap<String, Value>,
    options: &PromptRenderOptions,
) -> Option<String> {
    let catalog = catalog?;
    catalog.render_text(slot_id, vars, options).ok()
}

pub fn render_required_slot(
    catalog: Option<&Arc<PromptCatalog>>,
    slot_id: &str,
    vars: &HashMap<String, Value>,
) -> Result<String, PromptError> {
    render_required_slot_with_options(catalog, slot_id, vars, &PromptRenderOptions::default())
}

pub fn render_required_slot_with_options(
    catalog: Option<&Arc<PromptCatalog>>,
    slot_id: &str,
    vars: &HashMap<String, Value>,
    options: &PromptRenderOptions,
) -> Result<String, PromptError> {
    let catalog = catalog.ok_or(PromptError::CatalogNotLoaded)?;
    catalog.render_text(slot_id, vars, options).map_err(|error| {
        PromptError::RenderError(format!("failed to render required prompt slot `{slot_id}`: {error}"))
    })
}

#[cfg(test)]
#[path = "tests/renderer_helpers_tests.rs"]
mod tests;
