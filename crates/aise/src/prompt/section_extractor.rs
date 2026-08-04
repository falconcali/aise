use crate::prompt::{
    error::PromptError,
    model::{AssetRef, SlotId},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssetSection {
    pub asset_id: AssetRef,
    pub slot_ids: Vec<SlotId>,
    pub body: String,
    pub source_anchor: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionMetadata {
    asset_id: AssetRef,
    slot_ids: Vec<SlotId>,
    #[serde(default, rename = "notes")]
    _notes: Option<String>,
    #[serde(default, rename = "group")]
    _group: Option<String>,
}

enum SectionMarker {
    Start(SectionMetadata),
    End,
}

pub fn extract_asset_sections(
    source_path: &str,
    content: &str,
) -> Result<HashMap<AssetRef, PromptAssetSection>, PromptError> {
    let mut sections = HashMap::new();
    let mut current_metadata: Option<SectionMetadata> = None;
    let mut current_body_start = 0usize;
    let mut current_start_line = 0usize;
    let mut saw_section = false;
    let mut line_start = 0usize;

    for (line_index, raw_line) in content.split_inclusive('\n').enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim_end_matches(&['\r', '\n'][..]);
        let line_end = line_start + raw_line.len();

        match parse_marker(line)? {
            Some(SectionMarker::Start(metadata)) => {
                if let Some(open_section) = &current_metadata {
                    return Err(PromptError::CatalogLoad(format!(
                        "nested @asset block in `{}` at line {} while `{}` is still open",
                        source_path, line_number, open_section.asset_id
                    )));
                }

                if metadata.asset_id.trim().is_empty() {
                    return Err(PromptError::CatalogLoad(format!(
                        "empty asset_id in `{}` at line {}",
                        source_path, line_number
                    )));
                }
                if metadata.slot_ids.is_empty() {
                    return Err(PromptError::CatalogLoad(format!(
                        "section `{}` in `{}` must declare a non-empty slot_ids array",
                        metadata.asset_id, source_path
                    )));
                }
                if metadata.slot_ids.iter().any(|slot_id| slot_id.trim().is_empty()) {
                    return Err(PromptError::CatalogLoad(format!(
                        "section `{}` in `{}` contains an empty slot id",
                        metadata.asset_id, source_path
                    )));
                }

                current_start_line = line_number;
                current_body_start = line_end;
                current_metadata = Some(metadata);
                saw_section = true;
            }
            Some(SectionMarker::End) => {
                let metadata = current_metadata.take().ok_or_else(|| {
                    PromptError::CatalogLoad(format!(
                        "unexpected @endasset in `{}` at line {}",
                        source_path, line_number
                    ))
                })?;

                let asset_id = metadata.asset_id.clone();
                let source_anchor = format!("{}#{}", source_path, asset_id);
                let body = trim_structural_padding(&content[current_body_start..line_start]).to_string();

                if sections.contains_key(&asset_id) {
                    return Err(PromptError::CatalogLoad(format!(
                        "duplicate asset section `{}` in `{}`",
                        asset_id, source_path
                    )));
                }

                sections.insert(
                    asset_id.clone(),
                    PromptAssetSection {
                        asset_id,
                        slot_ids: metadata.slot_ids,
                        body,
                        source_anchor,
                    },
                );
            }
            None => {
                if current_metadata.is_none() && !line.trim().is_empty() {
                    return Err(PromptError::CatalogLoad(format!(
                        "unexpected content outside @asset block in `{}` at line {}",
                        source_path, line_number
                    )));
                }
            }
        }

        line_start = line_end;
    }

    if let Some(metadata) = current_metadata {
        return Err(PromptError::CatalogLoad(format!(
            "unclosed @asset block for `{}` in `{}` starting at line {}",
            metadata.asset_id, source_path, current_start_line
        )));
    }

    if !saw_section {
        return Err(PromptError::CatalogLoad(format!(
            "template `{}` must use @asset / @endasset sections",
            source_path
        )));
    }

    Ok(sections)
}

fn trim_structural_padding(body: &str) -> &str {
    let body = if let Some(stripped) = body.strip_prefix("\r\n") {
        stripped
    } else if let Some(stripped) = body.strip_prefix('\n') {
        stripped
    } else {
        body
    };

    if body.ends_with("\r\n\r\n") {
        &body[..body.len() - 2]
    } else if body.ends_with("\n\n") {
        &body[..body.len() - 1]
    } else {
        body
    }
}

fn parse_marker(line: &str) -> Result<Option<SectionMarker>, PromptError> {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix("{#") else {
        return Ok(None);
    };
    let Some(inner) = rest.strip_suffix("#}") else {
        return Ok(None);
    };

    let payload = inner.trim();
    if payload == "@endasset" {
        return Ok(Some(SectionMarker::End));
    }

    let Some(metadata_json) = payload.strip_prefix("@asset") else {
        return Ok(None);
    };

    let metadata_json = metadata_json.trim();
    if metadata_json.is_empty() {
        return Err(PromptError::CatalogLoad("`@asset` marker is missing metadata JSON".to_string()));
    }

    let metadata: SectionMetadata = serde_json::from_str(metadata_json)
        .map_err(|error| PromptError::CatalogLoad(format!("invalid @asset metadata JSON: {}", error)))?;

    Ok(Some(SectionMarker::Start(metadata)))
}

#[cfg(test)]
#[path = "tests/section_extractor_tests.rs"]
mod tests;
