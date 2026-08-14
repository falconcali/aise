use crate::config::{PromptCatalogSourceConfig, PromptModuleConfig};
use crate::prompt::catalog::PromptCatalog;
use crate::prompt::error::PromptError;
use crate::prompt::loader::{load_catalog, load_catalog_bundle};
use crate::prompt::profile::{PromptProfile, PromptProfileAssets, PromptProfileRegistry};
use crate::prompt::{PromptComposer, PromptComposition, PromptCompositionInput, PromptRenderOptions, SlotId};
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait TrustedPromptSource: Send + Sync {
    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError>;
}

pub struct CatalogPromptSource {
    catalog: Arc<PromptCatalog>,
    profiles: PromptProfileRegistry,
}

impl CatalogPromptSource {
    pub fn new(catalog: Arc<PromptCatalog>) -> Result<Self, PromptError> {
        let mut profiles = PromptProfileRegistry::default();
        profiles.register(
            PromptProfile::WriterPlanner,
            PromptProfileAssets {
                csi_slot: SlotId::new("context.writer_planner.csi"),
                rc_slot: SlotId::new("context.writer_planner.rc"),
                fti_slot: SlotId::new("context.writer_planner.fti"),
            },
        )?;
        profiles.register(
            PromptProfile::CharacterThink,
            PromptProfileAssets {
                csi_slot: SlotId::new("context.character_think.csi"),
                rc_slot: SlotId::new("context.character_think.rc"),
                fti_slot: SlotId::new("context.character_think.fti"),
            },
        )?;
        profiles.register(
            PromptProfile::StoryGenerator,
            PromptProfileAssets {
                csi_slot: SlotId::new("context.story_generator.csi"),
                rc_slot: SlotId::new("context.story_generator.rc"),
                fti_slot: SlotId::new("context.story_generator.fti"),
            },
        )?;
        profiles.register(
            PromptProfile::StoryStateExtractor,
            PromptProfileAssets {
                csi_slot: SlotId::new("context.story_state_extractor.csi"),
                rc_slot: SlotId::new("context.story_state_extractor.rc"),
                fti_slot: SlotId::new("context.story_state_extractor.fti"),
            },
        )?;
        profiles.register(
            PromptProfile::StoryRepairer,
            PromptProfileAssets {
                csi_slot: SlotId::new("context.story_repairer.csi"),
                rc_slot: SlotId::new("context.story_repairer.rc"),
                fti_slot: SlotId::new("context.story_repairer.fti"),
            },
        )?;
        Ok(Self { catalog, profiles })
    }

    pub fn from_config(config: &PromptModuleConfig) -> Result<Self, PromptError> {
        let catalog = match &config.source {
            PromptCatalogSourceConfig::Packaged => packaged_catalog()?,
            PromptCatalogSourceConfig::Directory { path } => load_catalog(path)?,
        };
        Self::new(Arc::new(catalog))
    }
}

impl TrustedPromptSource for CatalogPromptSource {
    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError> {
        PromptComposer::new(&self.catalog, &self.profiles).compose(input, &PromptRenderOptions::default())
    }
}

fn packaged_catalog() -> Result<PromptCatalog, PromptError> {
    let sources = BTreeMap::from([
        (
            "csi/story-repairer.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/story-repairer.md.j2"),
        ),
        (
            "rc/story-repairer.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/story-repairer.md.j2"),
        ),
        (
            "fti/story-repairer.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/story-repairer.md.j2"),
        ),
        (
            "csi/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/writer-planner.md.j2"),
        ),
        (
            "rc/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/writer-planner.md.j2"),
        ),
        (
            "fti/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/writer-planner.md.j2"),
        ),
        (
            "csi/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/character-think.md.j2"),
        ),
        (
            "rc/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/character-think.md.j2"),
        ),
        (
            "fti/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/character-think.md.j2"),
        ),
        (
            "csi/story-generator.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/story-generator.md.j2"),
        ),
        (
            "rc/story-generator.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/story-generator.md.j2"),
        ),
        (
            "fti/story-generator.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/story-generator.md.j2"),
        ),
        (
            "csi/story-state-extractor.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/story-state-extractor.md.j2"),
        ),
        (
            "rc/story-state-extractor.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/story-state-extractor.md.j2"),
        ),
        (
            "fti/story-state-extractor.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/story-state-extractor.md.j2"),
        ),
    ]);
    load_catalog_bundle(
        include_str!("../../assets/prompts/context-v2/index.yaml"),
        include_str!("../../assets/prompts/context-v2/slots.yaml"),
        &sources,
    )
}

#[cfg(test)]
#[path = "tests/trusted_prompt_source_tests.rs"]
mod tests;
