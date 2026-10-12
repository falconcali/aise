use crate::core::Sha256Digest;
use crate::core::asset::{AssetConfig, AssetError, CharacterCard, StoryPack};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

const STORY_PACK_ASSET: &str = "story_pack";
const CHARACTER_CARD_ASSET: &str = "character_card";

impl StoryPack {
    pub fn from_json(json: &str, config: &AssetConfig) -> Result<Self, AssetError> {
        let story_pack: Self = parse_document(STORY_PACK_ASSET, json, config)?;
        story_pack.validate(config)?;
        Ok(story_pack)
    }

    pub fn digest(&self) -> Result<Sha256Digest, AssetError> {
        canonical_digest(STORY_PACK_ASSET, self)
    }
}

impl CharacterCard {
    pub fn from_json(json: &str, config: &AssetConfig) -> Result<Self, AssetError> {
        let card: Self = parse_document(CHARACTER_CARD_ASSET, json, config)?;
        card.validate(config)?;
        Ok(card)
    }

    pub fn digest(&self) -> Result<Sha256Digest, AssetError> {
        canonical_digest(CHARACTER_CARD_ASSET, self)
    }
}

fn parse_document<T: DeserializeOwned>(asset: &'static str, json: &str, config: &AssetConfig) -> Result<T, AssetError> {
    if json.len() > config.max_document_bytes {
        return Err(AssetError::DocumentTooLarge {
            asset,
            actual: json.len(),
            maximum: config.max_document_bytes,
        });
    }
    serde_json::from_str(json).map_err(|error| AssetError::MalformedDocument {
        asset,
        message: error.to_string(),
    })
}

fn canonical_digest<T: Serialize>(asset: &'static str, value: &T) -> Result<Sha256Digest, AssetError> {
    let canonical = serde_json::to_vec(value).map_err(|error| AssetError::DigestFailed {
        asset,
        message: error.to_string(),
    })?;
    Sha256Digest::try_new(format!("{:x}", Sha256::digest(&canonical))).map_err(|error| AssetError::DigestFailed {
        asset,
        message: error.to_string(),
    })
}

#[cfg(test)]
#[path = "tests/document_tests.rs"]
mod tests;
