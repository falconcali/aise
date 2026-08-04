use crate::prompt::{
    error::PromptError,
    model::{PromptKind, PromptMessage, RenderedPrompt},
};
use serde_json::Value;
use std::collections::HashMap;

pub struct PromptRenderer {
    env: minijinja::Environment<'static>,
}

impl std::fmt::Debug for PromptRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptRenderer").finish()
    }
}

impl PromptRenderer {
    pub fn new() -> Self {
        let mut env = minijinja::Environment::new();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        Self { env }
    }

    pub fn add_template(&mut self, name: &str, source: &str) -> Result<(), PromptError> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())
            .map_err(|error| PromptError::RenderError(format!("failed to compile template `{}`: {}", name, error)))
    }

    pub fn render(&self, template_name: &str, vars: &HashMap<String, Value>) -> Result<String, PromptError> {
        let template = self
            .env
            .get_template(template_name)
            .map_err(|error| PromptError::RenderError(format!("template `{}` not found: {}", template_name, error)))?;

        template
            .render(vars)
            .map(normalize_line_endings)
            .map_err(|error| PromptError::RenderError(format!("render `{}` failed: {}", template_name, error)))
    }

    pub fn render_prompt(
        &self,
        template_name: &str,
        kind: &PromptKind,
        vars: &HashMap<String, Value>,
    ) -> Result<RenderedPrompt, PromptError> {
        let rendered = self.render(template_name, vars)?;

        match kind {
            PromptKind::Text | PromptKind::Fragment => Ok(RenderedPrompt::Text(rendered)),
            PromptKind::Messages | PromptKind::FewShot => {
                let messages: Vec<PromptMessage> = serde_json::from_str(&rendered).map_err(|error| {
                    PromptError::RenderError(format!(
                        "failed to parse rendered messages from `{}`: {}",
                        template_name, error
                    ))
                })?;
                Ok(RenderedPrompt::Messages(messages))
            }
        }
    }
}

fn normalize_line_endings(text: String) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

impl Default for PromptRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/renderer_tests.rs"]
mod tests;
