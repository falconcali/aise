use crate::prompt::error::PromptError;
use crate::prompt::model::SlotId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptProfile {
    WriterPlanner,
    CharacterThink,
    StoryGenerator,
    StoryRepairer,
}

impl PromptProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            PromptProfile::WriterPlanner => "writer_planner",
            PromptProfile::CharacterThink => "character_think",
            PromptProfile::StoryGenerator => "story_generator",
            PromptProfile::StoryRepairer => "story_repairer",
        }
    }
}

impl std::fmt::Display for PromptProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProfileAssets {
    pub csi_slot: SlotId,
    pub rc_slot: SlotId,
    pub fti_slot: SlotId,
}

#[derive(Debug, Default)]
pub struct PromptProfileRegistry {
    entries: HashMap<PromptProfile, PromptProfileAssets>,
}

impl PromptProfileRegistry {
    pub fn register(&mut self, profile: PromptProfile, assets: PromptProfileAssets) -> Result<(), PromptError> {
        if self.entries.contains_key(&profile) {
            return Err(PromptError::DuplicateProfileRegistration(profile.to_string()));
        }

        let duplicate_slot = if assets.csi_slot == assets.rc_slot || assets.csi_slot == assets.fti_slot {
            Some(assets.csi_slot.to_string())
        } else if assets.rc_slot == assets.fti_slot {
            Some(assets.rc_slot.to_string())
        } else {
            None
        };

        if let Some(slot) = duplicate_slot {
            return Err(PromptError::DuplicateLayerSlot {
                profile: profile.to_string(),
                slot,
            });
        }

        self.entries.insert(profile, assets);
        Ok(())
    }

    pub fn assets_for(&self, profile: PromptProfile) -> Result<&PromptProfileAssets, PromptError> {
        self.entries
            .get(&profile)
            .ok_or_else(|| PromptError::ProfileNotRegistered(profile.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedSystemPrompt(String);

impl TrustedSystemPrompt {
    pub fn try_new(value: impl Into<String>) -> Result<Self, crate::prompt::error::PromptError> {
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedContextMessage(String);

impl UntrustedContextMessage {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[path = "tests/profile_registry_tests.rs"]
mod tests;
