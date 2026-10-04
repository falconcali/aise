use crate::core::{ChatMessage, ChatMessageRole};
use crate::prompt::loader::load_catalog;
use crate::prompt::prompt_trace;
use crate::prompt::renderer::{PromptRenderer, PromptTemplateNames};
use crate::prompt::{PromptConfig, PromptError, PromptLayer};
use crate::trace::Observation;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

pub type PromptVars = HashMap<String, serde_json::Value>;

#[derive(Clone, Debug, Serialize)]
pub struct PromptSpec<'a> {
    prompt_id: &'a str,
    vars: PromptVars,
}

impl<'a> PromptSpec<'a> {
    pub fn new(prompt_id: &'a str, vars: PromptVars) -> Self {
        Self { prompt_id, vars }
    }

    pub fn prompt_id(&self) -> &str {
        self.prompt_id
    }

    pub fn vars(&self) -> &PromptVars {
        &self.vars
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RenderedPrompt {
    prompt_id: Arc<str>,
    messages: Vec<ChatMessage>,
}

impl RenderedPrompt {
    pub fn prompt_id(&self) -> &str {
        &self.prompt_id
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<ChatMessage> {
        self.messages
    }
}

pub struct Prompt {
    catalog: HashMap<Arc<str>, PromptTemplateNames>,
    renderer: PromptRenderer,
}

impl Prompt {
    pub fn new(config: PromptConfig) -> Result<Self, PromptError> {
        let loaded = load_catalog(&config)?;
        let mut renderer = PromptRenderer::new();
        let mut catalog = HashMap::with_capacity(loaded.prompts.len());
        for prompt in &loaded.prompts {
            let names = renderer.compile(prompt)?;
            catalog.insert(Arc::clone(&prompt.id), names);
        }
        log_catalog_loaded(catalog.len(), loaded.total_template_bytes);
        Ok(Self { catalog, renderer })
    }

    pub fn prompt_ids(&self) -> impl Iterator<Item = &str> {
        self.catalog.keys().map(|prompt_id| &**prompt_id)
    }

    pub fn render(&self, spec: PromptSpec<'_>, observation: &Observation) -> Result<RenderedPrompt, PromptError> {
        let prompt_observation = prompt_trace::begin_prompt_render(observation, &spec);
        let rendered_prompt = self.inner_render(spec);
        prompt_trace::finish_prompt_render(prompt_observation, &rendered_prompt);
        rendered_prompt
    }

    fn inner_render(&self, spec: PromptSpec<'_>) -> Result<RenderedPrompt, PromptError> {
        let (prompt_id, names) =
            self.catalog
                .get_key_value(spec.prompt_id())
                .ok_or_else(|| PromptError::PromptNotFound {
                    prompt_id: spec.prompt_id().to_owned(),
                })?;

        let messages = PromptLayer::ORDERED
            .into_iter()
            .map(|layer| {
                self.renderer
                    .render(names, prompt_id, layer, spec.vars())
                    .map(|content| layer_message(layer, content))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RenderedPrompt {
            prompt_id: Arc::clone(prompt_id),
            messages,
        })
    }
}

fn layer_message(layer: PromptLayer, content: String) -> ChatMessage {
    match layer.role() {
        ChatMessageRole::System => ChatMessage::system(content),
        ChatMessageRole::User => ChatMessage::user(content),
    }
}

fn log_catalog_loaded(prompt_count: usize, total_template_bytes: u64) {
    tracing::info!(
        target: "aise::prompt",
        prompt_count,
        total_template_bytes,
        "prompt catalog loaded"
    );
}

#[cfg(test)]
#[path = "tests/prompt_tests.rs"]
mod tests;
