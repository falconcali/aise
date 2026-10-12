use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use crate::core::{
    BoundedText, CharacterProfile, PackId, PlayDefinition, RetrievalHint, RoleBody, RoleId, SemanticVersion, StoryPack,
    StoryPackMeta, StoryPackProfile, StoryPackSpec,
};
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_PACK_TEXT_LIMIT: usize = 8192;
const DEFAULT_PLAYER_ROLE: &str = "protagonist";
const DEFAULT_PLAYER_ROLE_NAME: &str = "许仙";
const DEFAULT_PLAYER_ROLE_BACKSTORY: &str = "杭州药铺的年轻伙计，为人老实厚道，在西湖断桥边的雨中遇见白素贞。";
const DEFAULT_PLAYER_ROLE_RETRIEVAL: &str = "许仙 药铺伙计 断桥借伞";
const DEFAULT_PACK_OPENING: &str = "许仙在西湖游船靠岸时，天色忽然阴沉下来。细密的雨丝打湿了青石桥面，行人纷纷避入檐下。一个白衣女子站在断桥边，没有带伞，正望着湖面出神。她回头看向许仙，礼貌地问能否借伞同行。许仙把雨伞递过去时，远处传来一声闷雷，湖心荡开一圈不合时宜的涟漪。女子自称白素贞，并说日后一定归还这把伞。";

#[derive(Debug)]
pub struct PackStoreMem {
    config: PackStoreConfig,
}

impl PackStoreMem {
    pub fn new() -> Self {
        Self::with_config(PackStoreConfig::default())
    }

    pub fn with_config(config: PackStoreConfig) -> Self {
        Self { config }
    }

    fn bounded(value: &str, field: &'static str) -> Result<BoundedText, PersistenceError> {
        Ok(BoundedText::try_new(value, field, DEFAULT_PACK_TEXT_LIMIT)?)
    }

    fn build_default_pack(&self) -> Result<StoryPack, PersistenceError> {
        let player_role = RoleId::try_new(DEFAULT_PLAYER_ROLE)?;
        let story_pack = StoryPack {
            spec: StoryPackSpec::V1,
            meta: StoryPackMeta {
                creator: Self::bounded("aise", "creator")?,
                version: SemanticVersion::try_new("1.0.0")?,
                tags: Vec::new(),
            },
            profile: StoryPackProfile {
                title: Self::bounded("白蛇传", "title")?,
                description: None,
                genres: Vec::new(),
                themes: Vec::new(),
                tones: Vec::new(),
                point_of_view: Self::bounded("第三人称", "point_of_view")?,
                tense: Self::bounded("过去时", "tense")?,
            },
            roles: BTreeMap::from([(player_role.clone(), Self::build_default_player_role()?)]),
            play: PlayDefinition {
                playable_roles: BTreeSet::from([player_role]),
            },
            opening: Self::bounded(DEFAULT_PACK_OPENING, "opening")?,
            seed_facts: BTreeMap::new(),
            seed_rumors: BTreeMap::new(),
            constraints: Vec::new(),
            narrative: None,
        };
        story_pack.validate(&self.config.asset)?;
        Ok(story_pack)
    }

    fn build_default_player_role() -> Result<RoleBody, PersistenceError> {
        Ok(RoleBody {
            name: Self::bounded(DEFAULT_PLAYER_ROLE_NAME, "name")?,
            alias: Vec::new(),
            backstory: Self::bounded(DEFAULT_PLAYER_ROLE_BACKSTORY, "backstory")?,
            activation: Default::default(),
            retrieval: RetrievalHint::try_new(DEFAULT_PLAYER_ROLE_RETRIEVAL)?,
            default_profile: CharacterProfile {
                appearance: None,
                personality: None,
                speaking_style: None,
                dialogue_examples: Vec::new(),
            },
            seed_memories: BTreeMap::new(),
        })
    }
}

impl Default for PackStoreMem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PackStore for PackStoreMem {
    async fn get_pack(&self, _pack_id: &PackId) -> Result<StoryPack, PersistenceError> {
        self.build_default_pack()
    }
}

#[cfg(test)]
#[path = "tests/pack_store_mem_tests.rs"]
mod tests;
