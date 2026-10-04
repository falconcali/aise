use crate::core::ChatMessageRole;
use serde::Deserialize;
use std::fmt;

pub(crate) const MANIFEST_FILE_NAME: &str = "index.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptManifest {
    pub(crate) prompts: Vec<PromptManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptManifestEntry {
    pub(crate) id: String,
    pub(crate) csi: String,
    pub(crate) rc: String,
    pub(crate) fti: String,
}

impl PromptManifestEntry {
    pub(crate) fn path(&self, layer: PromptLayer) -> &str {
        match layer {
            PromptLayer::Csi => &self.csi,
            PromptLayer::Rc => &self.rc,
            PromptLayer::Fti => &self.fti,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PromptLayer {
    Csi,
    Rc,
    Fti,
}

impl PromptLayer {
    pub const ORDERED: [PromptLayer; 3] = [PromptLayer::Csi, PromptLayer::Rc, PromptLayer::Fti];

    pub fn as_str(self) -> &'static str {
        match self {
            PromptLayer::Csi => "csi",
            PromptLayer::Rc => "rc",
            PromptLayer::Fti => "fti",
        }
    }

    pub fn role(self) -> ChatMessageRole {
        match self {
            PromptLayer::Csi | PromptLayer::Fti => ChatMessageRole::System,
            PromptLayer::Rc => ChatMessageRole::User,
        }
    }
}

impl fmt::Display for PromptLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
