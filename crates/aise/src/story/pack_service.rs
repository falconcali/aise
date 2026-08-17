use crate::config::{AssetLimitsConfig, NarrativeConfig};
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::WorldBookSource;
use crate::domain::asset::ids::{PackId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::StoryPack;
use crate::domain::asset::validation::{AssetValidationCode, AssetValidationIssue, BoundedText, ValidationReport};
use crate::domain::ids::RoleId;
use crate::persistence::asset_store::{AssetStore, PackInfo, ValidatedStoryPack};
use crate::persistence::store::StoreError;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum AssetInput<'a> {
    Json(&'a [u8]),
    Pack(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackExportFormat {
    Json,
    AisePack,
}

#[derive(Debug, Clone)]
pub enum PackExport {
    Json(Vec<u8>),
    AisePack(Vec<u8>),
}

#[derive(Debug, thiserror::Error)]
pub enum AssetImportError {
    #[error("asset validation failed")]
    Invalid(ValidationReport),
    #[error("asset store operation failed")]
    Store(StoreError),
    #[error("asset I/O failed: {code}")]
    Io { code: &'static str },
}

#[derive(Debug, thiserror::Error)]
pub enum AssetExportError {
    #[error("story pack was not found")]
    NotFound,
    #[error("asset store operation failed")]
    Store(StoreError),
    #[error("asset export I/O failed: {code}")]
    Io { code: &'static str },
}

const FORBIDDEN_FIELD_NAMES: &[&str] = &[
    "system_prompt",
    "developer_prompt",
    "prompt",
    "post_history_instructions",
    "jailbreak",
    "message_role",
    "template",
    "position",
    "depth",
    "injection_order",
    "stop",
    "model",
    "tools",
    "skills",
    "temperature",
    "max_tokens",
];

const NARRATIVE_ROLE_REFERENCE_FIELDS: &[&str] = &["role_key", "source_role_key", "target_role_key"];

pub fn check_forbidden_fields(
    value: &serde_json::Value,
    path: &str,
    report: &mut ValidationReport,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        report.push(AssetValidationIssue::new(
            AssetValidationCode::LimitExceeded,
            path,
            "asset nesting depth exceeds limit",
        ));
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}/{key}");
                if FORBIDDEN_FIELD_NAMES.contains(&key.as_str()) {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::ForbiddenField,
                        child_path.clone(),
                        "forbidden runtime field",
                    ));
                }
                check_forbidden_fields(child, &child_path, report, depth + 1, max_depth);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                check_forbidden_fields(item, &format!("{path}/{index}"), report, depth + 1, max_depth);
            }
        }
        _ => {}
    }
}

pub fn validate_character_profile(
    profile: &CharacterProfile,
    path: &str,
    limits: &AssetLimitsConfig,
    report: &mut ValidationReport,
) {
    validate_required_profile_text(&profile.name, &format!("{path}/name"), limits.max_profile_name_bytes, report);
    validate_optional_profile_text(
        profile.appearance.as_ref(),
        &format!("{path}/appearance"),
        limits.max_profile_appearance_bytes,
        report,
    );
    validate_optional_profile_text(
        profile.personality.as_ref(),
        &format!("{path}/personality"),
        limits.max_profile_personality_bytes,
        report,
    );
    validate_optional_profile_text(
        profile.speaking_style.as_ref(),
        &format!("{path}/speaking_style"),
        limits.max_profile_speaking_style_bytes,
        report,
    );
    if profile.dialogue_examples.len() > limits.max_dialogue_examples_per_profile {
        report.push(AssetValidationIssue::new(
            AssetValidationCode::LimitExceeded,
            format!("{path}/dialogue_examples"),
            format!(
                "dialogue example count {} exceeds limit {}",
                profile.dialogue_examples.len(),
                limits.max_dialogue_examples_per_profile
            ),
        ));
    }
    for (index, example) in profile.dialogue_examples.iter().enumerate() {
        let example_path = format!("{path}/dialogue_examples/{index}");
        validate_required_profile_text(
            &example.situation,
            &format!("{example_path}/situation"),
            limits.max_dialogue_situation_bytes,
            report,
        );
        validate_required_profile_text(
            &example.response,
            &format!("{example_path}/response"),
            limits.max_dialogue_response_bytes,
            report,
        );
    }
    match serde_json::to_vec(profile) {
        Ok(bytes) => {
            if bytes.len() > limits.max_profile_total_bytes {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::LimitExceeded,
                    path,
                    format!("profile bytes {} exceed limit {}", bytes.len(), limits.max_profile_total_bytes),
                ));
            }
        }
        Err(_) => {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::SchemaInvalid,
                path,
                "profile failed to serialize",
            ));
        }
    }
}

fn validate_required_profile_text(text: &BoundedText, path: &str, max_bytes: usize, report: &mut ValidationReport) {
    if text.as_str().trim().is_empty() {
        report.push(AssetValidationIssue::new(
            AssetValidationCode::EmptyText,
            path.to_owned(),
            "text must not be empty",
        ));
    }
    if text.as_str().len() > max_bytes {
        report.push(AssetValidationIssue::new(
            AssetValidationCode::LimitExceeded,
            path.to_owned(),
            format!("text bytes {} exceed limit {}", text.as_str().len(), max_bytes),
        ));
    }
}

fn validate_optional_profile_text(
    text: Option<&BoundedText>,
    path: &str,
    max_bytes: usize,
    report: &mut ValidationReport,
) {
    if let Some(text) = text {
        validate_required_profile_text(text, path, max_bytes, report);
    }
}

pub struct NativeAssetImporter {
    limits: AssetLimitsConfig,
    narrative: NarrativeConfig,
}

impl NativeAssetImporter {
    pub fn new(limits: AssetLimitsConfig, narrative: NarrativeConfig) -> Self {
        Self { limits, narrative }
    }

    pub fn limits(&self) -> &AssetLimitsConfig {
        &self.limits
    }

    pub fn parse(&self, input: AssetInput<'_>) -> ValidationReport {
        match input {
            AssetInput::Json(bytes) => self.parse_json(bytes),
            AssetInput::Pack(bytes) => self.parse_pack(bytes),
        }
    }

    fn parse_json(&self, bytes: &[u8]) -> ValidationReport {
        let mut report = ValidationReport::ok();
        if bytes.len() > self.limits.max_manifest_bytes {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::LimitExceeded,
                "/",
                format!("manifest bytes {} exceed limit", self.limits.max_manifest_bytes),
            ));
            return report;
        }
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/",
                    "manifest is not valid JSON",
                ));
                return report;
            }
        };
        self.validate_pack_value(&value, &mut report);
        report
    }

    fn parse_pack(&self, bytes: &[u8]) -> ValidationReport {
        let mut report = ValidationReport::ok();
        if (bytes.len() as u64) > self.limits.max_compressed_pack_bytes {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::ArchiveSizeExceeded,
                "",
                "compressed pack exceeds limit",
            ));
            return report;
        }
        match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
            Ok(mut archive) => {
                let file_count = archive.len();
                if file_count > self.limits.max_asset_files {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::LimitExceeded,
                        "",
                        "archive file count exceeds limit",
                    ));
                    return report;
                }
                let mut seen_paths = BTreeSet::new();
                let mut manifest: Option<serde_json::Value> = None;
                let mut total_uncompressed: u64 = 0;
                for index in 0..file_count {
                    let mut entry = match archive.by_index(index) {
                        Ok(entry) => entry,
                        Err(_) => {
                            report.push(AssetValidationIssue::new(
                                AssetValidationCode::ArchivePathUnsafe,
                                "",
                                "unreadable archive entry",
                            ));
                            continue;
                        }
                    };
                    let raw_path = entry.name().to_string();
                    let normalized = normalize_archive_path(&raw_path);
                    if !archive_path_is_safe(&normalized) {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::ArchivePathUnsafe,
                            raw_path,
                            "unsafe archive path",
                        ));
                        continue;
                    }
                    if !seen_paths.insert(normalized.clone()) {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::ArchiveDuplicatePath,
                            raw_path,
                            "duplicate normalized archive path",
                        ));
                        continue;
                    }
                    if normalized == "story.aise.json" {
                        let mut data = Vec::new();
                        let mut read = 0usize;
                        loop {
                            let mut chunk = [0u8; 8192];
                            let count = match entry.read(&mut chunk) {
                                Ok(count) => count,
                                Err(_) => {
                                    report.push(AssetValidationIssue::new(
                                        AssetValidationCode::SchemaInvalid,
                                        normalized.clone(),
                                        "manifest read failed",
                                    ));
                                    break;
                                }
                            };
                            if count == 0 {
                                break;
                            }
                            read = read.saturating_add(count);
                            if read > self.limits.max_manifest_bytes {
                                report.push(AssetValidationIssue::new(
                                    AssetValidationCode::LimitExceeded,
                                    normalized.clone(),
                                    "manifest exceeds limit",
                                ));
                                break;
                            }
                            data.extend_from_slice(&chunk[..count]);
                        }
                        if manifest.is_none() {
                            manifest = match serde_json::from_slice(&data) {
                                Ok(value) => Some(value),
                                Err(_) => {
                                    report.push(AssetValidationIssue::new(
                                        AssetValidationCode::SchemaInvalid,
                                        normalized,
                                        "manifest is not valid JSON",
                                    ));
                                    None
                                }
                            };
                        }
                    } else if !normalized.starts_with("assets/") {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::ArchivePathUnsafe,
                            raw_path,
                            "asset outside assets/ directory",
                        ));
                    }
                    total_uncompressed = total_uncompressed.saturating_add(entry.size());
                    if total_uncompressed > self.limits.max_uncompressed_pack_bytes {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::ArchiveSizeExceeded,
                            "",
                            "uncompressed pack exceeds limit",
                        ));
                    }
                }
                if bytes.len() as u64 > 0 {
                    let ratio = total_uncompressed / (bytes.len() as u64).max(1);
                    if ratio > self.limits.max_compression_ratio as u64 {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::ArchiveRatioExceeded,
                            "",
                            "compression ratio exceeds limit",
                        ));
                    }
                }
                if report.valid {
                    if let Some(manifest) = manifest {
                        self.validate_pack_value(&manifest, &mut report);
                    } else {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::MissingReference,
                            "/",
                            "missing story.aise.json manifest",
                        ));
                    }
                }
            }
            Err(_) => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "",
                    "archive is not a valid zip container",
                ));
            }
        }
        report
    }

    pub fn validate_pack_value(&self, value: &serde_json::Value, report: &mut ValidationReport) {
        match value.get("spec").and_then(serde_json::Value::as_str) {
            Some("aise_story_v5") => {}
            Some(other) => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::UnsupportedSpec,
                    "/spec",
                    format!("unsupported spec {other}"),
                ));
                return;
            }
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/spec",
                    "missing spec discriminator",
                ));
                return;
            }
        }
        match value.get("spec_version").and_then(serde_json::Value::as_str) {
            Some("5.0") => {}
            Some(other) => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::UnsupportedSpecVersion,
                    "/spec_version",
                    format!("unsupported spec_version {other}"),
                ));
                return;
            }
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/spec_version",
                    "missing spec_version discriminator",
                ));
                return;
            }
        }
        if value.pointer("/story/premise").is_some() {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::SchemaInvalid,
                "/story/premise",
                "premise is not supported",
            ));
            return;
        }
        if serde_json::from_value::<StoryPack>(value.clone()).is_err() {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::SchemaInvalid,
                "/",
                "pack JSON does not match the final schema",
            ));
        }
        self.check_forbidden_fields(value, "/", report, 0);
        let role_ids = self.validate_roles(value, report);
        self.validate_play_and_start(value, &role_ids, report);
        self.validate_narrative_role_references(value, &role_ids, report);
        self.validate_graph(value, report);
        self.validate_salience(value, report);
    }

    fn validate_roles(&self, value: &serde_json::Value, report: &mut ValidationReport) -> BTreeSet<String> {
        let mut role_ids = BTreeSet::new();
        let roles = match value.get("roles").and_then(serde_json::Value::as_object) {
            Some(roles) => roles,
            None => return role_ids,
        };
        if roles.len() > self.limits.max_roles {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::LimitExceeded,
                "/roles",
                format!("role count {} exceeds limit", roles.len()),
            ));
        }
        for (role_key, role_value) in roles {
            let role_path = format!("/roles/{role_key}");
            if RoleId::try_new(role_key.clone()).is_err() {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::InvalidKey,
                    role_path.clone(),
                    "role id does not match the required syntax",
                ));
            }
            role_ids.insert(role_key.clone());
            let profile_path = format!("{role_path}/default_profile");
            match role_value.get("default_profile") {
                None => {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::SchemaInvalid,
                        profile_path,
                        "default_profile is missing",
                    ));
                }
                Some(profile_value) => match serde_json::from_value::<CharacterProfile>(profile_value.clone()) {
                    Ok(profile) => validate_character_profile(&profile, &profile_path, &self.limits, report),
                    Err(_) => {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::SchemaInvalid,
                            profile_path,
                            "default_profile does not match the character profile schema",
                        ));
                    }
                },
            }
            let background_path = format!("{role_path}/background");
            match role_value.get("background") {
                None | Some(serde_json::Value::Null) => {}
                Some(serde_json::Value::String(text)) => {
                    if text.trim().is_empty() {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::EmptyText,
                            background_path.clone(),
                            "background must not be empty",
                        ));
                    }
                    if text.len() > self.limits.max_role_background_bytes {
                        report.push(AssetValidationIssue::new(
                            AssetValidationCode::LimitExceeded,
                            background_path,
                            format!(
                                "background bytes {} exceed limit {}",
                                text.len(),
                                self.limits.max_role_background_bytes
                            ),
                        ));
                    }
                }
                Some(_) => {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::SchemaInvalid,
                        background_path,
                        "background must be a string",
                    ));
                }
            }
            if let Some(relationships) = role_value.get("initial_relationships").and_then(serde_json::Value::as_array) {
                for (index, relationship) in relationships.iter().enumerate() {
                    let target_path = format!("{role_path}/initial_relationships/{index}/target_role_id");
                    if let Some(target) = relationship.get("target_role_id").and_then(serde_json::Value::as_str) {
                        if !roles.contains_key(target) {
                            report.push(AssetValidationIssue::new(
                                AssetValidationCode::MissingReference,
                                target_path,
                                "relationship target role is not defined",
                            ));
                        }
                    }
                }
            }
        }
        role_ids
    }

    fn check_forbidden_fields(
        &self,
        value: &serde_json::Value,
        path: &str,
        report: &mut ValidationReport,
        depth: usize,
    ) {
        check_forbidden_fields(value, path, report, depth, self.narrative.max_condition_depth.saturating_add(8));
    }

    fn validate_play_and_start(
        &self,
        value: &serde_json::Value,
        role_ids: &BTreeSet<String>,
        report: &mut ValidationReport,
    ) {
        let play = match value.get("play").and_then(serde_json::Value::as_object) {
            Some(play) => play,
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/play",
                    "play definition is missing",
                ));
                return;
            }
        };
        let start = match value.get("start").and_then(serde_json::Value::as_object) {
            Some(start) => start,
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/start",
                    "start definition is missing",
                ));
                return;
            }
        };
        if start.contains_key("role_openings") {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::SchemaInvalid,
                "/start/role_openings",
                "role_openings is not supported; use start.opening",
            ));
        }
        if start
            .get("opening")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|opening| opening.trim().is_empty())
        {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::MissingStoryOpening,
                "/start/opening",
                "story opening is missing or empty",
            ));
        }
        let playable = match play.get("playable_role_ids").and_then(serde_json::Value::as_array) {
            Some(playable) => playable,
            None => return,
        };
        for playable_role in playable {
            let key = match playable_role.as_str() {
                Some(key) => key,
                None => continue,
            };
            if !role_ids.contains(key) {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::MissingReference,
                    format!("/play/playable_role_ids/{key}"),
                    "playable role is not defined",
                ));
            }
        }
    }

    fn validate_narrative_role_references(
        &self,
        value: &serde_json::Value,
        role_ids: &BTreeSet<String>,
        report: &mut ValidationReport,
    ) {
        let narrative = match value.get("narrative") {
            Some(narrative) => narrative,
            None => return,
        };
        self.scan_role_references(narrative, "/narrative", role_ids, report);
    }

    fn scan_role_references(
        &self,
        value: &serde_json::Value,
        path: &str,
        role_ids: &BTreeSet<String>,
        report: &mut ValidationReport,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let child_path = format!("{path}/{key}");
                    if NARRATIVE_ROLE_REFERENCE_FIELDS.contains(&key.as_str()) {
                        if let Some(role_key) = child.as_str() {
                            if !role_ids.is_empty() && !role_ids.contains(role_key) {
                                report.push(AssetValidationIssue::new(
                                    AssetValidationCode::MissingReference,
                                    child_path.clone(),
                                    "narrative role reference is not defined",
                                ));
                            }
                        }
                    }
                    self.scan_role_references(child, &child_path, role_ids, report);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    self.scan_role_references(item, &format!("{path}/{index}"), role_ids, report);
                }
            }
            _ => {}
        }
    }

    fn validate_salience(&self, value: &serde_json::Value, report: &mut ValidationReport) {
        let mut stack = vec![(value, "/".to_owned(), 0)];
        while let Some((current, path, depth)) = stack.pop() {
            if depth > 24 {
                continue;
            }
            match current {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        let child_path = format!("{path}/{key}");
                        if key == "salience" {
                            if let Some(salience) = child.as_u64() {
                                if salience > 100 {
                                    report.push(AssetValidationIssue::new(
                                        AssetValidationCode::InvalidSalience,
                                        child_path.clone(),
                                        "salience must be within 0..=100",
                                    ));
                                }
                            }
                        }
                        stack.push((child, child_path, depth + 1));
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        stack.push((item, format!("{path}/{index}"), depth + 1));
                    }
                }
                _ => {}
            }
        }
    }

    fn validate_graph(&self, value: &serde_json::Value, report: &mut ValidationReport) {
        let narrative = match value.get("narrative").and_then(serde_json::Value::as_object) {
            Some(narrative) => narrative,
            None => return,
        };
        let nodes = match narrative.get("nodes").and_then(serde_json::Value::as_object) {
            Some(nodes) => nodes,
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/narrative/nodes",
                    "narrative nodes are missing",
                ));
                return;
            }
        };
        if nodes.len() > self.narrative.max_graph_nodes {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::LimitExceeded,
                "/narrative/nodes",
                "graph node count exceeds limit",
            ));
        }
        let edges = match narrative.get("edges").and_then(serde_json::Value::as_array) {
            Some(edges) => edges,
            None => return,
        };
        if edges.len() > self.narrative.max_graph_edges {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::LimitExceeded,
                "/narrative/edges",
                "graph edge count exceeds limit",
            ));
        }
        let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut edge_keys = BTreeSet::new();
        for (index, edge) in edges.iter().enumerate() {
            let from = edge
                .get("from")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let to = edge
                .get("to")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !nodes.contains_key(&from) {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::GraphReferenceInvalid,
                    format!("/narrative/edges/{index}/from"),
                    "edge source node is not defined",
                ));
            }
            if !nodes.contains_key(&to) {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::GraphReferenceInvalid,
                    format!("/narrative/edges/{index}/to"),
                    "edge target node is not defined",
                ));
            }
            if let Some(key) = edge.get("edge_key").and_then(serde_json::Value::as_str) {
                if !edge_keys.insert(key.to_string()) {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::DuplicateKey,
                        format!("/narrative/edges/{index}/edge_key"),
                        "duplicate edge key",
                    ));
                }
            }
            adjacency.entry(from.clone()).or_default().push(to.clone());
        }
        let entry_nodes: Vec<String> = narrative
            .get("entry_nodes")
            .and_then(serde_json::Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if entry_nodes.is_empty() {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::GraphUnreachable,
                "/narrative/entry_nodes",
                "no entry nodes defined",
            ));
        }
        for entry in &entry_nodes {
            if !nodes.contains_key(entry) {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::GraphReferenceInvalid,
                    format!("/narrative/entry_nodes/{entry}"),
                    "entry node is not defined",
                ));
            }
        }
        if let Some(cycle) = find_cycle(&adjacency, &entry_nodes) {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::GraphCycle,
                format!("/narrative/nodes/{cycle}"),
                "narrative graph contains a cycle",
            ));
        }
        let reachable = reachable_set(&adjacency, &entry_nodes);
        for node_key in nodes.keys() {
            if !reachable.contains(node_key.as_str()) {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::GraphUnreachable,
                    format!("/narrative/nodes/{node_key}"),
                    "narrative node is unreachable from entry nodes",
                ));
            }
        }
    }
}

fn normalize_archive_path(raw: &str) -> String {
    raw.replace('\\', "/")
}

fn archive_path_is_safe(normalized: &str) -> bool {
    if normalized.starts_with('/') {
        return false;
    }
    if normalized.len() > 1 && normalized.as_bytes()[1] == b':' {
        return false;
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return false;
    }
    true
}

fn find_cycle(adjacency: &BTreeMap<String, Vec<String>>, entry_nodes: &[String]) -> Option<String> {
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    let mut stack: Vec<String> = Vec::new();
    for node in entry_nodes {
        if dfs_cycle(node, adjacency, &mut color, &mut stack) {
            return stack.last().cloned();
        }
    }
    for node in adjacency.keys() {
        if dfs_cycle(node, adjacency, &mut color, &mut stack) {
            return stack.last().cloned();
        }
    }
    None
}

fn dfs_cycle(
    node: &str,
    adjacency: &BTreeMap<String, Vec<String>>,
    color: &mut BTreeMap<String, u8>,
    stack: &mut Vec<String>,
) -> bool {
    match color.get(node) {
        Some(1) => return true,
        Some(2) => return false,
        _ => {}
    }
    color.insert(node.to_string(), 1);
    stack.push(node.to_string());
    if let Some(nexts) = adjacency.get(node) {
        for next in nexts {
            if dfs_cycle(next, adjacency, color, stack) {
                return true;
            }
        }
    }
    stack.pop();
    color.insert(node.to_string(), 2);
    false
}

fn reachable_set(adjacency: &BTreeMap<String, Vec<String>>, entry_nodes: &[String]) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    let mut queue: Vec<&String> = entry_nodes.iter().collect();
    while let Some(node) = queue.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        if let Some(nexts) = adjacency.get(node) {
            for next in nexts {
                if !visited.contains(next) {
                    queue.push(next);
                }
            }
        }
    }
    visited
}

pub struct PackService {
    importer: NativeAssetImporter,
    asset_store: Arc<dyn AssetStore>,
}

impl PackService {
    pub fn new(importer: NativeAssetImporter, asset_store: Arc<dyn AssetStore>) -> Self {
        Self { importer, asset_store }
    }

    pub fn validate(&self, input: AssetInput<'_>) -> ValidationReport {
        self.importer.parse(input)
    }

    pub async fn import(&self, input: AssetInput<'_>) -> Result<PackInfo, AssetImportError> {
        let report = self.importer.parse(input);
        if !report.valid {
            return Err(AssetImportError::Invalid(report));
        }
        let canonical_manifest = match input {
            AssetInput::Json(bytes) => bytes.to_vec(),
            AssetInput::Pack(bytes) => bytes.to_vec(),
        };
        let digest = sha256_digest(&canonical_manifest);
        let pack: StoryPack = match input {
            AssetInput::Json(bytes) => serde_json::from_slice(bytes).map_err(|_| {
                invalid_import(
                    AssetValidationCode::SchemaInvalid,
                    "/",
                    "pack JSON does not match the final schema",
                )
            })?,
            AssetInput::Pack(_) => {
                return Err(AssetImportError::Io {
                    code: "pack_container_deserialize_unsupported",
                });
            }
        };
        let resolved_world_book = match &pack.world_book {
            WorldBookSource::Embedded(book) => {
                crate::domain::asset::world_book::validate_topic_dictionary(&book.topics).map_err(|_| {
                    invalid_import(
                        AssetValidationCode::DuplicateKey,
                        "/world_book/topics",
                        "topic label or alias collides after normalization",
                    )
                })?;
                book.clone()
            }
            WorldBookSource::Frozen(_) => {
                return Err(invalid_import(
                    AssetValidationCode::MissingReference,
                    "/world_book",
                    "frozen world book is not present in the imported dependency set",
                ));
            }
        };
        let validated = ValidatedStoryPack {
            pack,
            canonical_manifest,
            digest,
            resolved_world_book,
        };
        let frozen = self.asset_store.import_pack(validated).await.map_err(AssetImportError::Store)?;
        Ok(PackInfo {
            pack_id: frozen.pack_id,
            pack_key: frozen.pack.meta.pack_key,
            version: frozen.pack.meta.version,
            digest: frozen.digest,
        })
    }

    pub async fn export(&self, pack_id: &PackId, format: PackExportFormat) -> Result<PackExport, AssetExportError> {
        let frozen = self.asset_store.export_pack(pack_id).await.map_err(|error| match error {
            StoreError::NotFound => AssetExportError::NotFound,
            other => AssetExportError::Store(other),
        })?;
        match format {
            PackExportFormat::Json => {
                if !frozen.pack.assets.is_empty() {
                    return Err(AssetExportError::Io {
                        code: "assets_require_pack_container",
                    });
                }
                let bytes = serde_json::to_vec(&frozen.pack).map_err(|_| AssetExportError::Io {
                    code: "pack_json_serialize_failed",
                })?;
                Ok(PackExport::Json(bytes))
            }
            PackExportFormat::AisePack => Ok(PackExport::AisePack(frozen.digest.to_string().into_bytes())),
        }
    }

    pub async fn list(&self) -> Result<Vec<PackSummary>, AssetExportError> {
        let packs = self.asset_store.list_packs().await.map_err(|error| match error {
            StoreError::NotFound => AssetExportError::NotFound,
            other => AssetExportError::Store(other),
        })?;
        Ok(packs
            .into_iter()
            .map(|frozen| PackSummary {
                pack_id: frozen.pack_id,
                pack_key: frozen.pack.meta.pack_key,
                title: frozen.pack.meta.title.to_string(),
                author: frozen.pack.meta.author.to_string(),
                version: frozen.pack.meta.version,
                description: frozen.pack.meta.description.to_string(),
                tags: frozen.pack.meta.tags.iter().map(|tag| tag.to_string()).collect(),
                digest: frozen.digest,
            })
            .collect())
    }

    pub async fn delete(&self, pack_id: &PackId) -> Result<bool, AssetExportError> {
        self.asset_store.delete_pack(pack_id).await.map_err(|error| match error {
            StoreError::NotFound => AssetExportError::NotFound,
            other => AssetExportError::Store(other),
        })
    }
}

fn invalid_import(code: AssetValidationCode, path: impl Into<String>, message: impl Into<String>) -> AssetImportError {
    AssetImportError::Invalid(ValidationReport::with_issues(vec![AssetValidationIssue::new(
        code, path, message,
    )]))
}

#[derive(Debug, Clone)]
pub struct PackSummary {
    pub pack_id: PackId,
    pub pack_key: StoryPackKey,
    pub title: String,
    pub author: String,
    pub version: SemanticVersion,
    pub description: String,
    pub tags: Vec<String>,
    pub digest: Sha256Digest,
}

pub fn sha256_digest(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    Sha256Digest::from_bytes(out)
}
