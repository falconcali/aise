use crate::config::{PromptCatalogSourceConfig, PromptModuleConfig};
use crate::prompt::catalog::PromptCatalog;
use crate::prompt::error::PromptError;
use crate::prompt::loader::{load_catalog, load_catalog_bundle};
use crate::prompt::model::AssetRef;
use crate::prompt::profile::{PromptProfile, TrustedSystemPrompt};
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait TrustedPromptSource: Send + Sync {
    fn resolve(&self, profile: PromptProfile) -> Result<TrustedSystemPrompt, PromptError>;
}

pub struct CatalogPromptSource {
    catalog: Arc<PromptCatalog>,
    profile_assets: BTreeMap<PromptProfile, AssetRef>,
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
        Ok(Self {
            catalog,
            profile_assets,
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
            "files/writer-planner.md.j2",
            include_str!("../../assets/prompts/context-v1/files/writer-planner.md.j2"),
        ),
        (
            "files/character-think.md.j2",
            include_str!("../../assets/prompts/context-v1/files/character-think.md.j2"),
        ),
        (
            "files/story-generator.md.j2",
            include_str!("../../assets/prompts/context-v1/files/story-generator.md.j2"),
        ),
        (
            "files/story-repairer.md.j2",
            include_str!("../../assets/prompts/context-v1/files/story-repairer.md.j2"),
        ),
    ]);
    load_catalog_bundle(
        include_str!("../../assets/prompts/context-v1/index.yaml"),
        include_str!("../../assets/prompts/context-v1/slots.yaml"),
        &sources,
    )
}
