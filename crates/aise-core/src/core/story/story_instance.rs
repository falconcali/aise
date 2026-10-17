use crate::core::{
    BoundedText, CoreError, FactEntry, FactId, NarrativeState, PackRef, PlayerId, RoleEntry, RoleId, RumorEntry,
    RumorId, StoryId, StoryPack,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct StoryPackSnapshot {
    pack_ref: PackRef,
    pack: StoryPack,
}

impl StoryPackSnapshot {
    pub(crate) fn from_pack(pack: StoryPack) -> Result<Self, CoreError> {
        let pack_ref = pack.pack_ref()?;
        Ok(Self { pack_ref, pack })
    }

    pub(crate) fn pack_ref(&self) -> &PackRef {
        &self.pack_ref
    }

    pub(crate) fn into_parts(self) -> (PackRef, StoryPack) {
        (self.pack_ref, self.pack)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StoryInstanceSpec {
    story_id: StoryId,
    pack_ref: PackRef,
    opening: Arc<BoundedText>,
    roles: BTreeMap<RoleId, RoleEntry>,
    facts: BTreeMap<FactId, FactEntry>,
    rumors: BTreeMap<RumorId, RumorEntry>,
    player_id: PlayerId,
    player_role: RoleId,
    narrative: NarrativeState,
}

impl StoryInstanceSpec {
    pub(crate) fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub(crate) fn into_parts(self) -> StoryInstanceParts {
        StoryInstanceParts {
            story_id: self.story_id,
            pack_ref: self.pack_ref,
            opening: self.opening,
            roles: self.roles,
            facts: self.facts,
            rumors: self.rumors,
            player_id: self.player_id,
            player_role: self.player_role,
            narrative: self.narrative,
        }
    }
}

pub(crate) struct StoryInstanceParts {
    pub(crate) story_id: StoryId,
    pub(crate) pack_ref: PackRef,
    pub(crate) opening: Arc<BoundedText>,
    pub(crate) roles: BTreeMap<RoleId, RoleEntry>,
    pub(crate) facts: BTreeMap<FactId, FactEntry>,
    pub(crate) rumors: BTreeMap<RumorId, RumorEntry>,
    pub(crate) player_id: PlayerId,
    pub(crate) player_role: RoleId,
    pub(crate) narrative: NarrativeState,
}

pub(crate) struct StoryCreateBundle {
    snapshot: StoryPackSnapshot,
    instance: StoryInstanceSpec,
}

impl StoryCreateBundle {
    pub(crate) fn assemble(
        snapshot: StoryPackSnapshot,
        story_id: StoryId,
        opening: Arc<BoundedText>,
        roles: BTreeMap<RoleId, RoleEntry>,
        facts: BTreeMap<FactId, FactEntry>,
        rumors: BTreeMap<RumorId, RumorEntry>,
        player_id: PlayerId,
        player_role: RoleId,
        narrative: NarrativeState,
    ) -> Self {
        let instance = StoryInstanceSpec {
            story_id,
            pack_ref: snapshot.pack_ref().clone(),
            opening,
            roles,
            facts,
            rumors,
            player_id,
            player_role,
            narrative,
        };
        Self { snapshot, instance }
    }

    pub(crate) fn into_parts(self) -> (StoryPackSnapshot, StoryInstanceSpec) {
        (self.snapshot, self.instance)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInstanceInfo {
    pub story_id: StoryId,
    pub player_id: PlayerId,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub life_cycle: StoryLifeCycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryLifeCycle {
    Active,
    Cancelled,
    Completed,
}
