use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AssetValidationError {
    #[error("invalid asset field {path}: {code}")]
    Invalid { code: AssetValidationCode, path: String },
    #[error("asset limit {limit} exceeded: actual {actual}, maximum {maximum}")]
    LimitExceeded {
        limit: &'static str,
        actual: u64,
        maximum: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetValidationCode {
    SchemaInvalid,
    UnsupportedSpec,
    UnsupportedSpecVersion,
    InvalidKey,
    InvalidVersion,
    UnknownField,
    ForbiddenField,
    MissingReference,
    DuplicateKey,
    MissingStoryOpening,
    InvalidSalience,
    LimitExceeded,
    EmptyText,
    GraphCycle,
    GraphUnreachable,
    GraphReferenceInvalid,
    GraphEffectForbidden,
    GraphConditionForbidden,
    AssetReferenceUnpinned,
    AssetDigestMismatch,
    ArchivePathUnsafe,
    ArchiveDuplicatePath,
    ArchiveSymlinkForbidden,
    ArchiveMimeForbidden,
    ArchiveSizeExceeded,
    ArchiveRatioExceeded,
    RetrievalHintRequired,
}

impl AssetValidationCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssetValidationCode::SchemaInvalid => "schema_invalid",
            AssetValidationCode::UnsupportedSpec => "unsupported_spec",
            AssetValidationCode::UnsupportedSpecVersion => "unsupported_spec_version",
            AssetValidationCode::InvalidKey => "invalid_key",
            AssetValidationCode::InvalidVersion => "invalid_version",
            AssetValidationCode::UnknownField => "unknown_field",
            AssetValidationCode::ForbiddenField => "forbidden_field",
            AssetValidationCode::MissingReference => "missing_reference",
            AssetValidationCode::DuplicateKey => "duplicate_key",
            AssetValidationCode::MissingStoryOpening => "missing_story_opening",
            AssetValidationCode::InvalidSalience => "invalid_salience",
            AssetValidationCode::LimitExceeded => "limit_exceeded",
            AssetValidationCode::EmptyText => "empty_text",
            AssetValidationCode::GraphCycle => "graph_cycle",
            AssetValidationCode::GraphUnreachable => "graph_unreachable",
            AssetValidationCode::GraphReferenceInvalid => "graph_reference_invalid",
            AssetValidationCode::GraphEffectForbidden => "graph_effect_forbidden",
            AssetValidationCode::GraphConditionForbidden => "graph_condition_forbidden",
            AssetValidationCode::AssetReferenceUnpinned => "asset_reference_unpinned",
            AssetValidationCode::AssetDigestMismatch => "asset_digest_mismatch",
            AssetValidationCode::ArchivePathUnsafe => "archive_path_unsafe",
            AssetValidationCode::ArchiveDuplicatePath => "archive_duplicate_path",
            AssetValidationCode::ArchiveSymlinkForbidden => "archive_symlink_forbidden",
            AssetValidationCode::ArchiveMimeForbidden => "archive_mime_forbidden",
            AssetValidationCode::ArchiveSizeExceeded => "archive_size_exceeded",
            AssetValidationCode::ArchiveRatioExceeded => "archive_ratio_exceeded",
            AssetValidationCode::RetrievalHintRequired => "retrieval_hint_required",
        }
    }
}

impl std::fmt::Display for AssetValidationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetValidationIssue {
    pub code: AssetValidationCode,
    pub path: String,
    pub message: String,
}

impl AssetValidationIssue {
    pub fn new(code: AssetValidationCode, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub valid: bool,
    pub issues: Vec<AssetValidationIssue>,
}

impl ValidationReport {
    pub fn ok() -> Self {
        Self {
            valid: true,
            issues: Vec::new(),
        }
    }

    pub fn with_issues(issues: Vec<AssetValidationIssue>) -> Self {
        let valid = issues.is_empty();
        Self { valid, issues }
    }

    pub fn push(&mut self, issue: AssetValidationIssue) {
        self.valid = false;
        self.issues.push(issue);
    }

    pub fn merge(&mut self, other: ValidationReport) {
        if !other.valid {
            self.valid = false;
        }
        self.issues.extend(other.issues);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoundedText(String);

impl BoundedText {
    pub fn try_new(
        value: impl Into<String>,
        field: &'static str,
        maximum_bytes: usize,
    ) -> Result<Self, AssetValidationError> {
        let value = value.into();
        let actual = value.len();
        if actual > maximum_bytes {
            return Err(AssetValidationError::LimitExceeded {
                limit: field,
                actual: actual as u64,
                maximum: maximum_bytes as u64,
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for BoundedText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BoundedText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value, "bounded_text", usize::MAX).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for BoundedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BoundedText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for BoundedText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScalarValue {
    Bool(bool),
    Integer(i64),
    Decimal(String),
    Text(String),
}
