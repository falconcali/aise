use crate::prompt::PromptLayer;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("invalid prompt config field {field}: {reason}")]
    InvalidConfig { field: &'static str, reason: &'static str },

    #[error("failed to read prompt manifest {path}")]
    ManifestRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("prompt manifest {path} is {bytes} bytes, limit {limit}")]
    ManifestTooLarge { path: PathBuf, bytes: u64, limit: usize },

    #[error("failed to parse prompt manifest {path}")]
    ManifestParse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("prompt manifest declares {count} prompts, limit {limit}")]
    TooManyPrompts { count: usize, limit: usize },

    #[error("prompt manifest entry {index} has an empty prompt id")]
    EmptyPromptId { index: usize },

    #[error("duplicate prompt id {prompt_id}")]
    DuplicatePromptId { prompt_id: String },

    #[error("prompt {prompt_id} not found")]
    PromptNotFound { prompt_id: String },

    #[error("invalid {layer} template path {path} for prompt {prompt_id}: {reason}")]
    InvalidTemplatePath {
        prompt_id: String,
        layer: PromptLayer,
        path: PathBuf,
        reason: &'static str,
    },

    #[error("failed to read {layer} template {path} for prompt {prompt_id}")]
    TemplateRead {
        prompt_id: String,
        layer: PromptLayer,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{layer} template {path} for prompt {prompt_id} is {bytes} bytes, limit {limit}")]
    TemplateTooLarge {
        prompt_id: String,
        layer: PromptLayer,
        path: PathBuf,
        bytes: u64,
        limit: usize,
    },

    #[error("prompt templates total {bytes} bytes, limit {limit}")]
    TotalTemplateBytesExceeded { bytes: u64, limit: usize },

    #[error("failed to compile {layer} template for prompt {prompt_id}")]
    TemplateCompile {
        prompt_id: String,
        layer: PromptLayer,
        #[source]
        source: minijinja::Error,
    },

    #[error("failed to render {layer} template for prompt {prompt_id}")]
    TemplateRender {
        prompt_id: String,
        layer: PromptLayer,
        #[source]
        source: minijinja::Error,
    },
}

impl PromptError {
    pub fn kind(&self) -> &'static str {
        match self {
            PromptError::InvalidConfig { .. } => "invalid_config",
            PromptError::ManifestRead { .. } => "manifest_read",
            PromptError::ManifestTooLarge { .. } => "manifest_too_large",
            PromptError::ManifestParse { .. } => "manifest_parse",
            PromptError::TooManyPrompts { .. } => "too_many_prompts",
            PromptError::EmptyPromptId { .. } => "empty_prompt_id",
            PromptError::DuplicatePromptId { .. } => "duplicate_prompt_id",
            PromptError::PromptNotFound { .. } => "prompt_not_found",
            PromptError::InvalidTemplatePath { .. } => "invalid_template_path",
            PromptError::TemplateRead { .. } => "template_read",
            PromptError::TemplateTooLarge { .. } => "template_too_large",
            PromptError::TotalTemplateBytesExceeded { .. } => "total_template_bytes_exceeded",
            PromptError::TemplateCompile { .. } => "template_compile",
            PromptError::TemplateRender { .. } => "template_render",
        }
    }
}
