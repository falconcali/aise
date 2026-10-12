use crate::core::asset::{
    AssetConfig, AssetError, CharacterCard, CharacterMeta, PlayDefinition, StoryPack, StoryPackMeta, StoryPackProfile,
};
use crate::core::knowledge::Recallable;
use crate::core::role::{CharacterProfile, RoleBody};
use crate::core::{BoundedText, NarrativeGraph, NarrativeNode, RoleId};
use std::collections::BTreeMap;
use std::fmt::Display;

impl StoryPack {
    pub fn validate(&self, config: &AssetConfig) -> Result<(), AssetError> {
        validate_pack_meta(&self.meta, config)?;
        validate_pack_profile(&self.profile, config)?;
        check_count("roles", self.roles.len(), config.max_roles)?;
        validate_entries("roles", &self.roles, |role| validate_role(role, config))?;
        validate_play(&self.play, &self.roles)?;
        check_text("opening", &self.opening, config.max_opening_bytes)?;
        check_count("seed_facts", self.seed_facts.len(), config.max_seed_entries)?;
        validate_entries("seed_facts", &self.seed_facts, |fact| validate_recallable(fact, config))?;
        check_count("seed_rumors", self.seed_rumors.len(), config.max_seed_entries)?;
        validate_entries("seed_rumors", &self.seed_rumors, |rumor| validate_recallable(rumor, config))?;
        check_list("constraints", &self.constraints, config.max_list_items, config.max_text_bytes)?;
        validate_narrative(self.narrative.as_ref(), config)
    }
}

impl CharacterCard {
    pub fn validate(&self, config: &AssetConfig) -> Result<(), AssetError> {
        check_text("name", &self.name, config.max_label_bytes)?;
        validate_card_meta(&self.meta, config)?;
        validate_profile(&self.profile, config)
    }
}

fn validate_pack_meta(meta: &StoryPackMeta, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("meta.creator", &meta.creator, config.max_label_bytes)?;
    check_list("meta.tags", &meta.tags, config.max_list_items, config.max_label_bytes)
}

fn validate_card_meta(meta: &CharacterMeta, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("meta.creator", &meta.creator, config.max_label_bytes)?;
    check_list("meta.tags", &meta.tags, config.max_list_items, config.max_label_bytes)
}

fn validate_pack_profile(profile: &StoryPackProfile, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("profile.title", &profile.title, config.max_label_bytes)?;
    check_optional_text("profile.description", profile.description.as_ref(), config.max_text_bytes)?;
    check_list("profile.genres", &profile.genres, config.max_list_items, config.max_label_bytes)?;
    check_list("profile.themes", &profile.themes, config.max_list_items, config.max_label_bytes)?;
    check_list("profile.tones", &profile.tones, config.max_list_items, config.max_label_bytes)?;
    check_text("profile.point_of_view", &profile.point_of_view, config.max_label_bytes)?;
    check_text("profile.tense", &profile.tense, config.max_label_bytes)
}

fn validate_play(play: &PlayDefinition, roles: &BTreeMap<RoleId, RoleBody>) -> Result<(), AssetError> {
    if play.playable_roles.is_empty() {
        return Err(AssetError::NoPlayableRole);
    }
    match play.playable_roles.iter().find(|role_id| !roles.contains_key(*role_id)) {
        Some(role_id) => Err(AssetError::UnknownRole {
            field: "play.playable_roles",
            role_id: role_id.to_string(),
        }),
        None => Ok(()),
    }
}

fn validate_role(role: &RoleBody, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("name", &role.name, config.max_label_bytes)?;
    check_list("alias", &role.alias, config.max_list_items, config.max_label_bytes)?;
    check_text("backstory", &role.backstory, config.max_text_bytes)?;
    validate_profile(&role.default_profile, config)?;
    check_count("seed_memories", role.seed_memories.len(), config.max_seed_memories_per_role)?;
    validate_entries("seed_memories", &role.seed_memories, |memory| {
        validate_recallable(memory, config)
    })
}

fn validate_profile(profile: &CharacterProfile, config: &AssetConfig) -> Result<(), AssetError> {
    check_optional_text("profile.appearance", profile.appearance.as_ref(), config.max_text_bytes)?;
    check_optional_text("profile.personality", profile.personality.as_ref(), config.max_text_bytes)?;
    check_optional_text("profile.speaking_style", profile.speaking_style.as_ref(), config.max_text_bytes)?;
    check_count(
        "profile.dialogue_examples",
        profile.dialogue_examples.len(),
        config.max_list_items,
    )?;
    profile.dialogue_examples.iter().try_for_each(|example| {
        check_text("profile.dialogue_examples.situation", &example.situation, config.max_text_bytes)?;
        check_text("profile.dialogue_examples.response", &example.response, config.max_text_bytes)
    })
}

fn validate_narrative(graph: Option<&NarrativeGraph>, config: &AssetConfig) -> Result<(), AssetError> {
    let Some(graph) = graph else {
        return Ok(());
    };
    graph.validate(&config.narrative)?;
    validate_entries("narrative.nodes", &graph.nodes, |node| validate_narrative_node(node, config))
}

fn validate_narrative_node(node: &NarrativeNode, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("title", &node.title, config.max_label_bytes)?;
    check_optional_text("dramatic_focus", node.dramatic_focus.as_ref(), config.max_text_bytes)?;
    node.constraints
        .iter()
        .try_for_each(|constraint| check_text("constraints.forbid", &constraint.forbid, config.max_text_bytes))
}

fn validate_recallable(item: &impl Recallable, config: &AssetConfig) -> Result<(), AssetError> {
    check_text("content", item.content(), config.max_text_bytes)
}

fn validate_entries<K: Display, V>(
    section: &'static str,
    entries: &BTreeMap<K, V>,
    validate: impl Fn(&V) -> Result<(), AssetError>,
) -> Result<(), AssetError> {
    entries.iter().try_for_each(|(id, entry)| {
        validate(entry).map_err(|source| AssetError::InvalidEntry {
            section,
            id: id.to_string(),
            source: Box::new(source),
        })
    })
}

fn check_text(field: &'static str, text: &BoundedText, maximum: usize) -> Result<(), AssetError> {
    let actual = text.len();
    if actual > maximum {
        return Err(AssetError::TextTooLong { field, actual, maximum });
    }
    Ok(())
}

fn check_optional_text(field: &'static str, text: Option<&BoundedText>, maximum: usize) -> Result<(), AssetError> {
    text.map_or(Ok(()), |text| check_text(field, text, maximum))
}

fn check_list(
    field: &'static str,
    items: &[BoundedText],
    max_items: usize,
    max_item_bytes: usize,
) -> Result<(), AssetError> {
    check_count(field, items.len(), max_items)?;
    items.iter().try_for_each(|item| check_text(field, item, max_item_bytes))
}

fn check_count(field: &'static str, actual: usize, maximum: usize) -> Result<(), AssetError> {
    if actual > maximum {
        return Err(AssetError::TooManyItems { field, actual, maximum });
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/validate_tests.rs"]
mod tests;
