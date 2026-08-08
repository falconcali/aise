use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::StoryRoleKey;
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::ids::{CharacterId, ConstraintId, StoryId, StoryRevision};
use crate::domain::knowledge::fact::WorldFact;
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::CurrentPerception;
use crate::domain::knowledge::rumor::SharedRumor;
use crate::domain::narrative::{StoryEvent, StorySummary, StoryTurn};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::state::{CharacterInstanceState, RelationshipState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentScene {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryConstraint {
    pub id: ConstraintId,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    pack: FrozenStoryPackRef,
    story_profile: StoryProfile,
    role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    character_cards: BTreeMap<CharacterId, CharacterCard>,
    character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    world_facts: Vec<WorldFact>,
    shared_rumors: Vec<SharedRumor>,
    memories: Vec<MemoryEntry>,
    current_perceptions: Vec<CurrentPerception>,
    current_scene: CurrentScene,
    relationships: Vec<RelationshipState>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    canonical_events: Vec<StoryEvent>,
    recent_turns: Vec<StoryTurn>,
    story_summary: StorySummary,
    active_constraints: Vec<StoryConstraint>,
}

impl StoryReadSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        story_id: StoryId,
        base_revision: StoryRevision,
        pack: FrozenStoryPackRef,
        story_profile: StoryProfile,
        role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
        role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
        character_cards: BTreeMap<CharacterId, CharacterCard>,
        character_states: BTreeMap<CharacterId, CharacterInstanceState>,
        world_facts: Vec<WorldFact>,
        shared_rumors: Vec<SharedRumor>,
        memories: Vec<MemoryEntry>,
        current_perceptions: Vec<CurrentPerception>,
        current_scene: CurrentScene,
        relationships: Vec<RelationshipState>,
        narrative_definition: NarrativeGraphDefinition,
        narrative_state: NarrativeRuntimeState,
        canonical_events: Vec<StoryEvent>,
        recent_turns: Vec<StoryTurn>,
        story_summary: StorySummary,
        active_constraints: Vec<StoryConstraint>,
    ) -> Self {
        Self {
            story_id,
            base_revision,
            pack,
            story_profile,
            role_definitions,
            role_bindings,
            character_cards,
            character_states,
            world_facts,
            shared_rumors,
            memories,
            current_perceptions,
            current_scene,
            relationships,
            narrative_definition,
            narrative_state,
            canonical_events,
            recent_turns,
            story_summary,
            active_constraints,
        }
    }

    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn base_revision(&self) -> StoryRevision {
        self.base_revision
    }

    pub fn pack(&self) -> &FrozenStoryPackRef {
        &self.pack
    }

    pub fn story_profile(&self) -> &StoryProfile {
        &self.story_profile
    }

    pub fn role_definitions(&self) -> &BTreeMap<StoryRoleKey, StoryRole> {
        &self.role_definitions
    }

    pub fn role_binding(&self, key: &StoryRoleKey) -> Option<&RoleBinding> {
        self.role_bindings.get(key)
    }

    pub fn role_bindings(&self) -> &BTreeMap<StoryRoleKey, RoleBinding> {
        &self.role_bindings
    }

    pub fn character_cards(&self) -> &BTreeMap<CharacterId, CharacterCard> {
        &self.character_cards
    }

    pub fn character_states(&self) -> &BTreeMap<CharacterId, CharacterInstanceState> {
        &self.character_states
    }

    pub fn world_facts(&self) -> &[WorldFact] {
        &self.world_facts
    }

    pub fn shared_rumors(&self) -> &[SharedRumor] {
        &self.shared_rumors
    }

    pub fn memories(&self) -> &[MemoryEntry] {
        &self.memories
    }

    pub fn current_perceptions(&self) -> &[CurrentPerception] {
        &self.current_perceptions
    }

    pub fn character_memory(&self, id: &CharacterId) -> impl Iterator<Item = &MemoryEntry> {
        self.memories.iter().filter(move |entry| &entry.owner == id)
    }

    pub fn current_scene(&self) -> &CurrentScene {
        &self.current_scene
    }

    pub fn relationships(&self) -> &[RelationshipState] {
        &self.relationships
    }

    pub fn narrative_definition(&self) -> &NarrativeGraphDefinition {
        &self.narrative_definition
    }

    pub fn narrative_state(&self) -> &NarrativeRuntimeState {
        &self.narrative_state
    }

    pub fn canonical_events(&self) -> &[StoryEvent] {
        &self.canonical_events
    }

    pub fn recent_turns(&self) -> &[StoryTurn] {
        &self.recent_turns
    }

    pub fn story_summary(&self) -> &StorySummary {
        &self.story_summary
    }

    pub fn active_constraints(&self) -> &[StoryConstraint] {
        &self.active_constraints
    }

    pub fn graph_revision(&self) -> u64 {
        self.narrative_state.graph_revision
    }
}
