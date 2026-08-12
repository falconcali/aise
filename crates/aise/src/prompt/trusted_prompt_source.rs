use crate::config::{PromptCatalogSourceConfig, PromptModuleConfig};
use crate::prompt::catalog::PromptCatalog;
use crate::prompt::error::PromptError;
use crate::prompt::loader::{load_catalog, load_catalog_bundle};
use crate::prompt::model::AssetRef;
use crate::prompt::profile::{PromptProfile, PromptProfileAssets, PromptProfileRegistry, TrustedSystemPrompt};
use crate::prompt::{PromptComposer, PromptComposition, PromptCompositionInput, PromptRenderOptions, SlotId};
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait TrustedPromptSource: Send + Sync {
    fn resolve(&self, profile: PromptProfile) -> Result<TrustedSystemPrompt, PromptError>;
    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError> {
        Err(PromptError::ProfileNotRegistered(input.profile.to_string()))
    }
}

pub struct CatalogPromptSource {
    catalog: Arc<PromptCatalog>,
    profile_assets: BTreeMap<PromptProfile, AssetRef>,
    profiles: PromptProfileRegistry,
}

impl CatalogPromptSource {
    pub fn new(
        catalog: Arc<PromptCatalog>,
        profile_assets: BTreeMap<PromptProfile, AssetRef>,
    ) -> Result<Self, PromptError> {
        if profile_assets.len() != all_profiles().len() {
            return Err(PromptError::CatalogLoad(
                "profile assets must contain exactly four business profiles".into(),
            ));
        }
        for profile in all_profiles() {
            let asset_ref = profile_assets
                .get(&profile)
                .ok_or_else(|| PromptError::AssetNotFound(format!("profile asset missing for {}", profile.as_str())))?;
            if catalog.asset(asset_ref.as_str()).is_none() {
                return Err(PromptError::AssetNotFound(asset_ref.as_str().to_owned()));
            }
        }
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
        Ok(Self {
            catalog,
            profile_assets,
            profiles,
        })
    }

    pub fn from_config(config: &PromptModuleConfig) -> Result<Self, PromptError> {
        let catalog = match &config.source {
            PromptCatalogSourceConfig::Packaged => packaged_catalog()?,
            PromptCatalogSourceConfig::Directory { path } => load_catalog(path)?,
        };
        Self::new(Arc::new(catalog), config.profile_assets.clone())
    }
}

impl TrustedPromptSource for CatalogPromptSource {
    fn resolve(&self, profile: PromptProfile) -> Result<TrustedSystemPrompt, PromptError> {
        let asset_ref = self
            .profile_assets
            .get(&profile)
            .ok_or_else(|| PromptError::AssetNotFound(format!("profile asset missing for {}", profile.as_str())))?;
        let text = self.catalog.render_asset_text(asset_ref)?;
        TrustedSystemPrompt::try_new(text)
    }

    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError> {
        PromptComposer::new(&self.catalog, &self.profiles)
            .compose(input, &PromptRenderOptions::with_pack_override(Some("context-v2".into())))
    }
}

fn all_profiles() -> [PromptProfile; 4] {
    [
        PromptProfile::WriterPlanner,
        PromptProfile::CharacterThink,
        PromptProfile::StoryGenerator,
        PromptProfile::StoryRepairer,
    ]
}

fn packaged_catalog() -> Result<PromptCatalog, PromptError> {
    let sources = BTreeMap::from([
        (
            "files/story-generator.md.j2",
            include_str!("../../assets/prompts/context-v1/files/story-generator.md.j2"),
        ),
        (
            "files/story-repairer.md.j2",
            include_str!("../../assets/prompts/context-v1/files/story-repairer.md.j2"),
        ),
        (
            "../context-v2/csi/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/writer-planner.md.j2"),
        ),
        (
            "../context-v2/rc/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/writer-planner.md.j2"),
        ),
        (
            "../context-v2/fti/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/writer-planner.md.j2"),
        ),
        (
            "../context-v2/csi/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/csi/character-think.md.j2"),
        ),
        (
            "../context-v2/rc/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/rc/character-think.md.j2"),
        ),
        (
            "../context-v2/fti/character-think.md.j2",
            include_str!("../../assets/prompts/context-v2/fti/character-think.md.j2"),
        ),
    ]);
    load_catalog_bundle(
        include_str!("../../assets/prompts/context-v1/index.yaml"),
        include_str!("../../assets/prompts/context-v1/slots.yaml"),
        &sources,
    )
}

#[cfg(test)]
#[path = "tests/trusted_prompt_source_tests.rs"]
mod tests;
