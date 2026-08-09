use crate::config::PromptModuleConfig;
use crate::prompt::asset::{CompiledPromptAsset, PromptAssetManifest};
use crate::prompt::catalog::{PromptCatalog, PromptCatalogParts};
use crate::prompt::error::PromptError;
use crate::prompt::loader::load_catalog;
use crate::prompt::model::{AssetRef, AssetStatus, PromptKind};
use crate::prompt::profile::{PromptProfile, TrustedSystemPrompt};
use crate::prompt::renderer::PromptRenderer;
use crate::prompt::resolver::PromptResolver;
use crate::prompt::slot::SlotRegistry;
use std::collections::{BTreeMap, HashMap};
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
        let catalog = match load_catalog(&config.catalog_path) {
            Ok(catalog) => Arc::new(catalog),
            Err(_) => Arc::new(builtin_catalog()?),
        };
        let profile_assets = if config.profile_assets.is_empty() {
            builtin_profile_assets()
        } else {
            config.profile_assets.clone()
        };
        Self::new(catalog, profile_assets)
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

fn all_profiles() -> [PromptProfile; 5] {
    [
        PromptProfile::WriterPlanner,
        PromptProfile::CharacterThink,
        PromptProfile::StoryGenerator,
        PromptProfile::StoryRepairer,
        PromptProfile::NarrativeValidator,
    ]
}

fn builtin_profile_assets() -> BTreeMap<PromptProfile, AssetRef> {
    let mut assets = BTreeMap::new();
    for profile in all_profiles() {
        assets.insert(profile, AssetRef::new(format!("builtin/{}", profile.as_str())));
    }
    assets
}

fn builtin_catalog() -> Result<PromptCatalog, PromptError> {
    let mut renderer = PromptRenderer::new();
    let mut assets = HashMap::new();
    for (profile, body) in [
        (
            PromptProfile::WriterPlanner,
            "You are the writer planner for an interactive story. Plan retrieval and character focus for this turn.",
        ),
        (
            PromptProfile::CharacterThink,
            "You are the narrative director simulating a character's thoughts. Stay inside the character's viewpoint.",
        ),
        (
            PromptProfile::StoryGenerator,
            "You are the story generator. Write the next beat of the interactive story consistent with the plan.",
        ),
        (
            PromptProfile::StoryRepairer,
            "You are the story repairer. Revise the previous proposal to fix the reported validation issues.",
        ),
        (
            PromptProfile::NarrativeValidator,
            "You are the narrative validator. Verify the proposal is consistent with the story world and plan.",
        ),
    ] {
        let asset_id = AssetRef::new(format!("builtin/{}", profile.as_str()));
        renderer.add_template(asset_id.as_str(), body)?;
        assets.insert(
            asset_id.clone(),
            CompiledPromptAsset {
                manifest: PromptAssetManifest {
                    asset_id: asset_id.clone(),
                    kind: PromptKind::Text,
                    source_path: asset_id.as_str().to_owned(),
                    input_schema_ref: None,
                    output_contract_ref: None,
                    labels: HashMap::new(),
                    hash: None,
                    status: AssetStatus::Active,
                },
                source_anchor: asset_id.as_str().to_owned(),
                resolved_hash: format!("builtin:{}", profile.as_str()),
                template_name: asset_id.as_str().to_owned(),
            },
        );
    }
    Ok(PromptCatalog::from_parts(PromptCatalogParts {
        assets,
        slots: SlotRegistry::default(),
        packs: HashMap::new(),
        raw_packs: HashMap::new(),
        resolver: PromptResolver {
            default_pack: "default".to_string(),
        },
        policies: Vec::new(),
        loaded_at: chrono::Utc::now(),
        renderer,
    }))
}
