use thiserror::Error;

#[derive(Error, Debug)]
pub enum PromptError {
    #[error("prompt template not found")]
    TemplateNotFound,
    #[error("prompt template parsing failed: {0}")]
    ParsingFailed(String),
    #[error("prompt template rendering failed: {0}")]
    RenderingFailed(String),
    #[error("prompt template validation failed: {0}")]
    ValidationFailed(String),
}

impl PromptError {
    pub fn kind(&self) -> &'static str {
        match self {
            PromptError::TemplateNotFound => "template_not_found",
            PromptError::ParsingFailed(..) => "parsing_failed",
            PromptError::RenderingFailed(..) => "rendering_failed",
            PromptError::ValidationFailed(..) => "validation_failed",
        }
    }
}
