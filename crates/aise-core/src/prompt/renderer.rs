use crate::prompt::loader::LoadedPrompt;
use crate::prompt::{PromptError, PromptLayer, PromptVars};
use minijinja::{AutoEscape, Environment, UndefinedBehavior};

pub(crate) struct PromptRenderer {
    env: Environment<'static>,
}

impl PromptRenderer {
    pub(crate) fn new() -> Self {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.set_auto_escape_callback(|_| AutoEscape::None);
        Self { env }
    }

    pub(crate) fn compile(&mut self, prompt: &LoadedPrompt) -> Result<PromptTemplateNames, PromptError> {
        let names = PromptTemplateNames {
            csi: template_name(&prompt.id, PromptLayer::Csi),
            rc: template_name(&prompt.id, PromptLayer::Rc),
            fti: template_name(&prompt.id, PromptLayer::Fti),
        };
        for layer in PromptLayer::ORDERED {
            self.env
                .add_template_owned(names.get(layer).to_owned(), prompt.source(layer).to_owned())
                .map_err(|source| PromptError::TemplateCompile {
                    prompt_id: prompt.id.to_string(),
                    layer,
                    source,
                })?;
        }
        Ok(names)
    }

    pub(crate) fn render(
        &self,
        names: &PromptTemplateNames,
        prompt_id: &str,
        layer: PromptLayer,
        vars: &PromptVars,
    ) -> Result<String, PromptError> {
        let render_error = |source| PromptError::TemplateRender {
            prompt_id: prompt_id.to_owned(),
            layer,
            source,
        };
        self.env
            .get_template(names.get(layer))
            .map_err(render_error)?
            .render(vars)
            .map_err(render_error)
    }
}

pub(crate) struct PromptTemplateNames {
    csi: String,
    rc: String,
    fti: String,
}

impl PromptTemplateNames {
    pub(crate) fn get(&self, layer: PromptLayer) -> &str {
        match layer {
            PromptLayer::Csi => &self.csi,
            PromptLayer::Rc => &self.rc,
            PromptLayer::Fti => &self.fti,
        }
    }
}

pub(crate) fn template_name(prompt_id: &str, layer: PromptLayer) -> String {
    format!("{prompt_id}/{layer}")
}

#[cfg(test)]
#[path = "tests/renderer_tests.rs"]
mod tests;
