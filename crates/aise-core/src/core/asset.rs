use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackInfo {
    pub pack_id: String,
    pub pack_key: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackSummaryInfo {
    pub pack_id: String,
    pub pack_key: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub tags: Vec<String>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardInfo {
    pub character_id: String,
    pub name: String,
    pub creator: Option<String>,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub path: String,
    pub message: String,
}
