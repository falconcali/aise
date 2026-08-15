use crate::config::AssetLimitsConfig;
use crate::domain::asset::character_card::{CharacterCard, CharacterProfile};
use crate::domain::asset::validation::{AssetValidationCode, AssetValidationIssue, ValidationReport};
use crate::domain::ids::CharacterId;
use crate::persistence::asset_store::{AssetStore, CharacterCardInfo, ValidatedCharacterCard};
use crate::persistence::store::StoreError;
use crate::story::pack_service::{check_forbidden_fields, sha256_digest, validate_character_profile};
use std::sync::Arc;

const CHARACTER_CARD_MAX_DEPTH: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum CharacterCardImportError {
    #[error("character card validation failed")]
    Invalid(ValidationReport),
    #[error("character card store operation failed")]
    Store(StoreError),
}

pub struct CharacterCardService {
    asset_store: Arc<dyn AssetStore>,
    limits: AssetLimitsConfig,
}

impl CharacterCardService {
    pub fn new(asset_store: Arc<dyn AssetStore>, limits: AssetLimitsConfig) -> Self {
        Self { asset_store, limits }
    }

    pub fn validate(&self, bytes: &[u8]) -> ValidationReport {
        let mut report = ValidationReport::ok();
        if bytes.len() > self.limits.max_manifest_bytes {
            report.push(AssetValidationIssue::new(
                AssetValidationCode::LimitExceeded,
                "/",
                "character card bytes exceed the manifest limit",
            ));
            return report;
        }
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/",
                    "character card is not valid JSON",
                ));
                return report;
            }
        };
        self.validate_value(&value, &mut report);
        report
    }

    fn validate_value(&self, value: &serde_json::Value, report: &mut ValidationReport) {
        match value.get("spec").and_then(serde_json::Value::as_str) {
            Some("aise_char_v4") => {}
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
            Some("4.0") => {}
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
        check_forbidden_fields(value, "/", report, 0, CHARACTER_CARD_MAX_DEPTH);
        match value.get("character_id").and_then(serde_json::Value::as_str) {
            Some(character_id) => {
                if CharacterId::try_new(character_id).is_err() {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::InvalidKey,
                        "/character_id",
                        "character_id must be a canonical UUID",
                    ));
                }
            }
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/character_id",
                    "character_id is missing",
                ));
            }
        }
        if let Some(tags) = value
            .get("meta")
            .and_then(|meta| meta.get("tags"))
            .and_then(serde_json::Value::as_array)
        {
            if tags.len() > self.limits.max_tags_per_item {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::LimitExceeded,
                    "/meta/tags",
                    "tag count exceeds limit",
                ));
            }
        }
        match value.get("profile") {
            Some(profile_value) => match serde_json::from_value::<CharacterProfile>(profile_value.clone()) {
                Ok(profile) => validate_character_profile(&profile, "/profile", &self.limits, report),
                Err(_) => {
                    report.push(AssetValidationIssue::new(
                        AssetValidationCode::SchemaInvalid,
                        "/profile",
                        "profile does not match the character profile schema",
                    ));
                }
            },
            None => {
                report.push(AssetValidationIssue::new(
                    AssetValidationCode::SchemaInvalid,
                    "/profile",
                    "profile is missing",
                ));
            }
        }
    }

    pub async fn import(&self, bytes: &[u8]) -> Result<CharacterCardInfo, CharacterCardImportError> {
        let report = self.validate(bytes);
        if !report.valid {
            return Err(CharacterCardImportError::Invalid(report));
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| invalid_card("/", "character card is not valid JSON"))?;
        let card: CharacterCard = serde_json::from_value(value)
            .map_err(|_| invalid_card("/", "character card does not match the final schema"))?;
        let canonical_json =
            serde_json::to_vec(&card).map_err(|_| invalid_card("/", "character card failed to canonicalize"))?;
        let digest = sha256_digest(&canonical_json);
        let validated = ValidatedCharacterCard {
            card,
            canonical_json,
            digest,
        };
        let frozen = self
            .asset_store
            .import_character(validated)
            .await
            .map_err(CharacterCardImportError::Store)?;
        Ok(CharacterCardInfo {
            character_id: frozen.card.character_id.clone(),
            name: frozen.card.profile.name.clone(),
            creator: frozen.card.meta.creator.clone(),
            version: frozen.card.meta.version.clone(),
            digest: frozen.digest,
        })
    }

    pub async fn list(&self) -> Result<Vec<CharacterCardInfo>, CharacterCardImportError> {
        let mut infos = self
            .asset_store
            .list_characters()
            .await
            .map_err(CharacterCardImportError::Store)?;
        infos.sort_by(|left, right| {
            left.name
                .as_str()
                .cmp(right.name.as_str())
                .then_with(|| left.character_id.as_str().cmp(right.character_id.as_str()))
                .then_with(|| left.version.as_str().cmp(right.version.as_str()))
        });
        Ok(infos)
    }
}

fn invalid_card(path: impl Into<String>, message: impl Into<String>) -> CharacterCardImportError {
    CharacterCardImportError::Invalid(ValidationReport::with_issues(vec![AssetValidationIssue::new(
        AssetValidationCode::SchemaInvalid,
        path,
        message,
    )]))
}

#[cfg(test)]
#[path = "tests/character_card_service_tests.rs"]
mod tests;
