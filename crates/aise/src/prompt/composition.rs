use crate::prompt::catalog::PromptCatalog;
use crate::prompt::error::PromptError;
use crate::prompt::metadata::PromptMetadata;
use crate::prompt::profile::{PromptProfile, PromptProfileRegistry};
use crate::prompt::resolver::PromptRenderOptions;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptLayer {
    Csi,
    Rc,
    Fti,
}

impl PromptLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csi => "csi",
            Self::Rc => "rc",
            Self::Fti => "fti",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSystemInstruction(String);

impl CoreSystemInstruction {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextMessage(String);

impl RuntimeContextMessage {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalTaskInstruction(String);

impl FinalTaskInstruction {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct PromptComposition {
    pub profile: PromptProfile,
    pub csi: CoreSystemInstruction,
    pub rc: RuntimeContextMessage,
    pub fti: FinalTaskInstruction,
    pub metadata: PromptCompositionMetadata,
}

#[derive(Debug, Clone)]
pub struct PromptCompositionMetadata {
    pub csi: PromptMetadata,
    pub rc: PromptMetadata,
    pub fti: PromptMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimePromptVars(HashMap<String, Value>);

impl RuntimePromptVars {
    pub fn new(vars: HashMap<String, Value>) -> Self {
        Self(vars)
    }

    pub fn as_map(&self) -> &HashMap<String, Value> {
        &self.0
    }
}

impl From<HashMap<String, Value>> for RuntimePromptVars {
    fn from(vars: HashMap<String, Value>) -> Self {
        Self::new(vars)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrustedPromptVars(HashMap<String, Value>);

impl TrustedPromptVars {
    pub fn new(vars: HashMap<String, Value>) -> Self {
        Self(vars)
    }

    pub fn as_map(&self) -> &HashMap<String, Value> {
        &self.0
    }
}

impl From<HashMap<String, Value>> for TrustedPromptVars {
    fn from(vars: HashMap<String, Value>) -> Self {
        Self::new(vars)
    }
}

#[derive(Debug, Clone)]
pub struct PromptCompositionInput {
    pub profile: PromptProfile,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

pub struct PromptComposer<'a> {
    catalog: &'a PromptCatalog,
    profiles: &'a PromptProfileRegistry,
}

impl<'a> PromptComposer<'a> {
    pub fn new(catalog: &'a PromptCatalog, profiles: &'a PromptProfileRegistry) -> Self {
        Self { catalog, profiles }
    }

    pub fn compose(
        &self,
        input: &PromptCompositionInput,
        options: &PromptRenderOptions,
    ) -> Result<PromptComposition, PromptError> {
        let assets = self.profiles.assets_for(input.profile)?;
        let empty_vars = HashMap::new();
        let (csi, csi_metadata) =
            self.render_layer(input.profile, PromptLayer::Csi, assets.csi_slot.as_str(), &empty_vars, options)?;
        let (rc, rc_metadata) = self.render_layer(
            input.profile,
            PromptLayer::Rc,
            assets.rc_slot.as_str(),
            input.rc_vars.as_map(),
            options,
        )?;
        let (fti, fti_metadata) = self.render_layer(
            input.profile,
            PromptLayer::Fti,
            assets.fti_slot.as_str(),
            input.fti_vars.as_map(),
            options,
        )?;

        Ok(PromptComposition {
            profile: input.profile,
            csi: CoreSystemInstruction(csi),
            rc: RuntimeContextMessage(rc),
            fti: FinalTaskInstruction(fti),
            metadata: PromptCompositionMetadata {
                csi: csi_metadata,
                rc: rc_metadata,
                fti: fti_metadata,
            },
        })
    }

    fn render_layer(
        &self,
        profile: PromptProfile,
        layer: PromptLayer,
        slot_id: &str,
        vars: &HashMap<String, Value>,
        options: &PromptRenderOptions,
    ) -> Result<(String, PromptMetadata), PromptError> {
        self.catalog
            .render_text_with_metadata(slot_id, vars, options)
            .map_err(|error| match error {
                PromptError::KindMismatch { .. } => PromptError::LayerMustRenderAsText {
                    profile: profile.to_string(),
                    layer: layer.as_str().to_string(),
                },
                other => other,
            })
    }
}

pub trait ProviderPromptEncoder: Send + Sync {
    type Encoded;

    fn encode(&self, composition: &PromptComposition) -> Result<Self::Encoded, PromptError>;
}

#[cfg(test)]
#[path = "tests/composition_tests.rs"]
mod tests;
