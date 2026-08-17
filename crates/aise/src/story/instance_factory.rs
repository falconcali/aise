use crate::domain::asset::frozen_ref::FrozenCharacterCardRef;
use crate::domain::asset::ids::{FactKey, PackId, PlayerId};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{ConstraintId, RoleId, StoryId};
use crate::domain::knowledge::fact::{Proposition, WorldFact};
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::{KnowledgeSource, allocate_knowledge_ids};
use crate::domain::knowledge::rumor::{Claim, SharedRumor, TruthValue};
use crate::domain::knowledge::{KnowledgeEntry, KnowledgeIdHighWater, KnowledgeKind, KnowledgeSourceId};
use crate::domain::narrative_graph::condition::{
    ConditionEvalContext, NarrativeNodeState, RoleControllerKind, evaluate_condition,
};
use crate::domain::narrative_graph::definition::{NarrativeError, NarrativeGraphDefinition, NarrativeLimits};
use crate::domain::narrative_graph::effect::{NarrativeEffectId, NarrativeTransitionKind};
use crate::domain::narrative_graph::state::{NarrativeRuntimeState, PendingNarrativeEffect};
use crate::domain::narrative_graph::state_view::{NarrativeStateView, NarrativeStateViewError};
use crate::domain::story_instance::constraint::{ActiveStoryConstraint, StoryConstraintSource};
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState};
use crate::domain::story_instance::state::{InstanceSettings, RelationshipState};
use crate::persistence::asset_store::{AssetStore, FrozenStoryPack};
use crate::persistence::store::{MaterializedStoryInstanceSpec, Store, StoreError};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CreateStoryInstanceSpec {
    pub pack_id: PackId,
    pub player_id: PlayerId,
    pub player_role_id: RoleId,
    pub role_profile_selections: BTreeMap<RoleId, FrozenCharacterCardRef>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StoryInstantiationLimits {
    pub max_roles: usize,
    pub max_role_bytes: usize,
    pub max_facts: usize,
    pub max_rumors: usize,
    pub max_memories: usize,
    pub max_relationships: usize,
    pub max_opening_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryInstantiationError {
    #[error("story pack was not found")]
    PackNotFound,
    #[error("story role was not found: {role_id}")]
    RoleNotFound { role_id: RoleId },
    #[error("story role is not playable: {role_id}")]
    RoleNotPlayable { role_id: RoleId },
    #[error("character card was not found")]
    CharacterCardNotFound,
    #[error("character card reference does not match stored content")]
    CharacterCardReferenceMismatch,
    #[error("story role profile selection is duplicated: {role_id}")]
    DuplicateRoleProfileSelection { role_id: RoleId },
    #[error("story materialization reference is invalid: {code}")]
    InvalidReference { code: &'static str },
    #[error("story instantiation limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
    #[error("story store operation failed")]
    Store(StoreError),
}

pub struct StoryInstanceFactory {
    asset_store: Arc<dyn AssetStore>,
    store: Arc<dyn Store>,
    limits: StoryInstantiationLimits,
    narrative_limits: NarrativeLimits,
}

impl StoryInstanceFactory {
    pub fn new(
        asset_store: Arc<dyn AssetStore>,
        store: Arc<dyn Store>,
        limits: StoryInstantiationLimits,
        narrative_limits: NarrativeLimits,
    ) -> Self {
        Self {
            asset_store,
            store,
            limits,
            narrative_limits,
        }
    }

    pub async fn create(&self, spec: CreateStoryInstanceSpec) -> Result<StoryInfo, StoryInstantiationError> {
        let frozen = self.asset_store.load_pack(&spec.pack_id).await.map_err(|error| match error {
            StoreError::NotFound => StoryInstantiationError::PackNotFound,
            other => StoryInstantiationError::Store(other),
        })?;
        let materialized = self.materialize(&frozen, &spec).await?;
        self.store
            .create_story_instance(&materialized)
            .await
            .map_err(StoryInstantiationError::Store)
    }

    async fn materialize(
        &self,
        frozen: &FrozenStoryPack,
        spec: &CreateStoryInstanceSpec,
    ) -> Result<MaterializedStoryInstanceSpec, StoryInstantiationError> {
        let pack = &frozen.pack;
        if !pack.roles.contains_key(&spec.player_role_id) {
            return Err(StoryInstantiationError::RoleNotFound {
                role_id: spec.player_role_id.clone(),
            });
        }
        if !pack.play.playable_role_ids.contains(&spec.player_role_id) {
            return Err(StoryInstantiationError::RoleNotPlayable {
                role_id: spec.player_role_id.clone(),
            });
        }
        enforce_limit(pack.roles.len(), self.limits.max_roles, "max_roles")?;
        if spec.role_profile_selections.len() > pack.roles.len() {
            return Err(StoryInstantiationError::LimitExceeded { limit: "max_roles" });
        }
        for role_id in spec.role_profile_selections.keys() {
            if !pack.roles.contains_key(role_id) {
                return Err(StoryInstantiationError::RoleNotFound {
                    role_id: role_id.clone(),
                });
            }
        }
        enforce_limit(frozen.resolved_world_book.facts.len(), self.limits.max_facts, "max_facts")?;
        enforce_limit(frozen.resolved_world_book.rumors.len(), self.limits.max_rumors, "max_rumors")?;
        let memory_count = pack.roles.values().try_fold(0usize, |count, role| {
            count
                .checked_add(role.seed_memories.len())
                .ok_or(StoryInstantiationError::LimitExceeded { limit: "max_memories" })
        })?;
        enforce_limit(memory_count, self.limits.max_memories, "max_memories")?;
        let relationship_count = pack.roles.values().try_fold(0usize, |count, role| {
            count
                .checked_add(role.initial_relationships.len())
                .ok_or(StoryInstantiationError::LimitExceeded {
                    limit: "max_relationships",
                })
        })?;
        enforce_limit(relationship_count, self.limits.max_relationships, "max_relationships")?;
        let story_id = StoryId::try_new(format!("story-{}", uuid::Uuid::new_v4())).map_err(|_| {
            StoryInstantiationError::Store(StoreError::ConstraintViolation {
                constraint: "story_id".into(),
            })
        })?;
        let mut resolved_cards = BTreeMap::new();
        for (role_id, reference) in &spec.role_profile_selections {
            let frozen_card = self.asset_store.load_character(reference).await.map_err(|error| match error {
                StoreError::NotFound => StoryInstantiationError::CharacterCardNotFound,
                other => StoryInstantiationError::Store(other),
            })?;
            if &frozen_card.frozen_ref() != reference {
                return Err(StoryInstantiationError::CharacterCardReferenceMismatch);
            }
            if resolved_cards
                .insert(role_id.clone(), (reference.clone(), frozen_card))
                .is_some()
            {
                return Err(StoryInstantiationError::DuplicateRoleProfileSelection {
                    role_id: role_id.clone(),
                });
            }
        }
        let mut roles = BTreeMap::new();
        for (role_id, definition) in &pack.roles {
            let controller = if role_id == &spec.player_role_id {
                RoleController::Player(spec.player_id.clone())
            } else {
                RoleController::Ai
            };
            let (effective_profile, source_character) = match resolved_cards.get(role_id) {
                Some((reference, frozen_card)) => (frozen_card.card.profile.clone(), Some(reference.clone())),
                None => (definition.default_profile.clone(), None),
            };
            let role = StoryRole {
                role_id: role_id.clone(),
                controller,
                role_label: definition.role_label.clone(),
                narrative_function: definition.narrative_function.clone(),
                background: definition.background.clone(),
                effective_profile,
                source_character,
                state: StoryRoleState {
                    location: definition.initial_state.location.clone(),
                    goals: definition.initial_state.goals.clone(),
                    attributes: definition.initial_state.attributes.clone(),
                },
            };
            let role_bytes = role.compact_byte_len().map_err(|_| StoryInstantiationError::InvalidReference {
                code: "role_serialization_failed",
            })?;
            if role_bytes > self.limits.max_role_bytes {
                return Err(StoryInstantiationError::LimitExceeded {
                    limit: "max_role_bytes",
                });
            }
            roles.insert(role_id.clone(), role);
        }
        let relationships = materialize_relationships(pack, &roles)?;
        let (knowledge, knowledge_id_high_water) = materialize_knowledge(frozen, spec.created_at_ms)?;
        let active_constraints = pack
            .constraints
            .iter()
            .map(|(key, definition)| {
                Ok(ActiveStoryConstraint {
                    id: ConstraintId::try_new(format!("{}:seed:constraint:{}", story_id.as_str(), key.as_str()))
                        .map_err(|_| StoryInstantiationError::InvalidReference {
                            code: "constraint_id_invalid",
                        })?,
                    source: StoryConstraintSource {
                        pack_id: frozen.pack_id.clone(),
                        constraint_key: key.clone(),
                    },
                    scope: definition.scope.clone(),
                    requirement: definition.requirement.clone(),
                    lifecycle: definition.lifecycle.clone(),
                })
            })
            .collect::<Result<Vec<_>, StoryInstantiationError>>()?;
        let opening = pack.start.opening.clone();
        enforce_limit(opening.as_str().len(), self.limits.max_opening_bytes, "max_opening_bytes")?;
        let narrative_state = bootstrap_narrative_state(&pack.narrative, &roles, &relationships, self.narrative_limits)
            .map_err(|_| StoryInstantiationError::InvalidReference {
                code: "narrative_bootstrap_failed",
            })?;
        Ok(MaterializedStoryInstanceSpec {
            story_id,
            pack: frozen.frozen_ref(),
            settings: InstanceSettings::default(),
            roles,
            relationships,
            knowledge,
            knowledge_id_high_water,
            narrative_state,
            fact_values: BTreeMap::new(),
            active_constraints,
            opening,
            created_at_ms: spec.created_at_ms,
        })
    }
}

struct BootstrapNarrativeStateView<'a> {
    roles: &'a BTreeMap<RoleId, StoryRole>,
    relationships: &'a [RelationshipState],
}

impl NarrativeStateView for BootstrapNarrativeStateView<'_> {
    fn fact_value(&self, _fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }

    fn role_attribute(
        &self,
        role_id: &RoleId,
        attribute: &BoundedText,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        let role = self.roles.get(role_id).ok_or_else(|| NarrativeStateViewError::UnknownRole {
            role_id: role_id.as_str().to_owned(),
        })?;
        Ok(role
            .state
            .attributes
            .iter()
            .find(|(key, _)| key.as_str() == attribute.as_str())
            .map(|(_, value)| value))
    }

    fn relationship_trust(
        &self,
        source_role_id: &RoleId,
        target_role_id: &RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        if !self.roles.contains_key(source_role_id) {
            return Err(NarrativeStateViewError::UnknownRole {
                role_id: source_role_id.as_str().to_owned(),
            });
        }
        if !self.roles.contains_key(target_role_id) {
            return Err(NarrativeStateViewError::UnknownRole {
                role_id: target_role_id.as_str().to_owned(),
            });
        }
        Ok(self
            .relationships
            .iter()
            .find(|relationship| {
                &relationship.source_role_id == source_role_id && &relationship.target_role_id == target_role_id
            })
            .map(|relationship| relationship.trust))
    }

    fn role_controller(&self, role_id: &RoleId) -> Result<RoleControllerKind, NarrativeStateViewError> {
        let role = self.roles.get(role_id).ok_or_else(|| NarrativeStateViewError::UnknownRole {
            role_id: role_id.as_str().to_owned(),
        })?;
        Ok(if role.is_player_controlled() {
            RoleControllerKind::Player
        } else {
            RoleControllerKind::Ai
        })
    }
}

fn bootstrap_narrative_state(
    definition: &NarrativeGraphDefinition,
    roles: &BTreeMap<RoleId, StoryRole>,
    relationships: &[RelationshipState],
    limits: NarrativeLimits,
) -> Result<NarrativeRuntimeState, NarrativeError> {
    let view = BootstrapNarrativeStateView { roles, relationships };
    let mut state = NarrativeRuntimeState::initial();
    let semantic_results = BTreeMap::new();
    let mut activated = Vec::new();
    for entry in &definition.entry_nodes {
        let node = definition
            .nodes
            .get(entry)
            .ok_or_else(|| NarrativeError::MissingReference { key: entry.to_string() })?;
        let eval_ctx = ConditionEvalContext {
            state: &state,
            view: &view,
            semantic_results: &semantic_results,
            current_turn: 0,
            limits,
        };
        if evaluate_condition(&node.activate_when, &eval_ctx, 0)?
            == crate::domain::narrative_graph::condition::NarrativeTruthValue::Satisfied
        {
            activated.push(entry.clone());
        }
    }
    if activated.is_empty() {
        return Ok(state);
    }
    state.graph_revision = 1;
    for node_key in activated {
        state.node_states.insert(node_key.clone(), NarrativeNodeState::Active);
        let node = definition
            .nodes
            .get(&node_key)
            .ok_or_else(|| NarrativeError::MissingReference {
                key: node_key.to_string(),
            })?;
        for (effect_index, effect_definition) in node.effects.on_activate.iter().enumerate() {
            let effect_id =
                NarrativeEffectId::for_transition(&node_key, NarrativeTransitionKind::Activate, 1, effect_index as u32);
            state.pending_effects.insert(
                effect_id.clone(),
                PendingNarrativeEffect {
                    effect_id,
                    source_node: node_key.clone(),
                    source_transition: NarrativeTransitionKind::Activate,
                    source_graph_revision: 1,
                    created_by_turn: None,
                    effect_index: effect_index as u32,
                    expires_after_turn: None,
                    definition: effect_definition.clone(),
                },
            );
        }
    }
    Ok(state)
}

fn materialize_relationships(
    pack: &crate::domain::asset::story_pack::StoryPack,
    roles: &BTreeMap<RoleId, StoryRole>,
) -> Result<Vec<RelationshipState>, StoryInstantiationError> {
    let mut relationships = Vec::new();
    let mut keys = BTreeSet::new();
    for (source_role_id, role) in &pack.roles {
        if !roles.contains_key(source_role_id) {
            return Err(StoryInstantiationError::InvalidReference {
                code: "relationship_source_missing",
            });
        }
        for seed in &role.initial_relationships {
            if !roles.contains_key(&seed.target_role_id) {
                return Err(StoryInstantiationError::InvalidReference {
                    code: "relationship_target_missing",
                });
            }
            let relationship = RelationshipState {
                source_role_id: source_role_id.clone(),
                target_role_id: seed.target_role_id.clone(),
                kind: seed.kind.clone(),
                trust: seed.trust,
            };
            if !keys.insert(relationship.key()) {
                return Err(StoryInstantiationError::InvalidReference {
                    code: "duplicate_relationship",
                });
            }
            relationships.push(relationship);
        }
    }
    relationships.sort_by_key(RelationshipState::key);
    Ok(relationships)
}

fn materialize_knowledge(
    frozen: &FrozenStoryPack,
    created_at_ms: i64,
) -> Result<(Vec<KnowledgeEntry>, KnowledgeIdHighWater), StoryInstantiationError> {
    let source = KnowledgeSource::Seed {
        pack_id: frozen.pack_id.clone(),
        pack_digest: frozen.digest.clone(),
    };
    let fact_count = frozen.resolved_world_book.facts.len();
    let rumor_count = frozen.resolved_world_book.rumors.len();
    let mut memory_seeds = frozen
        .pack
        .roles
        .iter()
        .flat_map(|(role_id, role)| role.seed_memories.iter().map(move |seed| (role_id.clone(), seed)))
        .collect::<Vec<_>>();
    memory_seeds.sort_by(|(left_role, left_seed), (right_role, right_seed)| {
        (left_role, &left_seed.memory_key).cmp(&(right_role, &right_seed.memory_key))
    });
    let addition_kinds = std::iter::repeat_n(KnowledgeKind::Fact, fact_count)
        .chain(std::iter::repeat_n(KnowledgeKind::Rumor, rumor_count))
        .chain(std::iter::repeat_n(KnowledgeKind::Memory, memory_seeds.len()))
        .collect::<Vec<_>>();
    let allocation = allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &addition_kinds).map_err(|_| {
        StoryInstantiationError::LimitExceeded {
            limit: "knowledge_id_allocation",
        }
    })?;
    let mut ids = allocation.assigned.into_iter();
    let mut entries = Vec::new();
    for (key, seed) in &frozen.resolved_world_book.facts {
        let KnowledgeSourceId::Fact(id) = ids.next().ok_or(StoryInstantiationError::LimitExceeded {
            limit: "knowledge_id_allocation",
        })?
        else {
            return Err(StoryInstantiationError::InvalidReference {
                code: "knowledge_id_allocation_kind_mismatch",
            });
        };
        let proposition = seed.proposition.as_ref().map(|value| Proposition {
            subject: value.subject.clone(),
            predicate: value.predicate.clone(),
            value: value.value.clone(),
        });
        entries.push(KnowledgeEntry::Fact(WorldFact {
            id,
            key: Some(key.clone()),
            text: seed.content.clone(),
            proposition,
            retrieval_hint: seed.retrieval_hint.clone(),
            entities: canonical(seed.entities.clone()),
            topics: canonical(seed.topics.clone()),
            salience: seed.salience,
            source: source.clone(),
        }));
    }
    for (key, seed) in &frozen.resolved_world_book.rumors {
        let KnowledgeSourceId::Rumor(id) = ids.next().ok_or(StoryInstantiationError::LimitExceeded {
            limit: "knowledge_id_allocation",
        })?
        else {
            return Err(StoryInstantiationError::InvalidReference {
                code: "knowledge_id_allocation_kind_mismatch",
            });
        };
        let claim = seed.claim.as_ref().map(|value| Claim {
            subject: value.subject.clone(),
            predicate: value.predicate.clone(),
            value: value.value.clone(),
        });
        entries.push(KnowledgeEntry::Rumor(SharedRumor {
            id,
            key: Some(key.clone()),
            content: seed.content.clone(),
            claim,
            retrieval_hint: seed.retrieval_hint.clone(),
            entities: canonical(seed.entities.clone()),
            topics: canonical(seed.topics.clone()),
            salience: seed.salience,
            source_role_id: None,
            truth_value: TruthValue::Unverified,
            source: source.clone(),
        }));
    }
    for (role_id, seed) in memory_seeds {
        let KnowledgeSourceId::Memory(id) = ids.next().ok_or(StoryInstantiationError::LimitExceeded {
            limit: "knowledge_id_allocation",
        })?
        else {
            return Err(StoryInstantiationError::InvalidReference {
                code: "knowledge_id_allocation_kind_mismatch",
            });
        };
        let entities = canonical(vec![crate::domain::asset::entity::KnowledgeEntity::Role(role_id.clone())]);
        entries.push(KnowledgeEntry::Memory(MemoryEntry {
            id,
            owner: role_id,
            kind: seed.kind.clone(),
            content: seed.content.clone(),
            entities,
            topics: canonical(seed.topics.clone()),
            salience: seed.salience,
            source: source.clone(),
            created_at_ms,
        }));
    }
    Ok((entries, allocation.new_high_water))
}

fn canonical<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values.dedup();
    values
}

fn enforce_limit(actual: usize, maximum: usize, limit: &'static str) -> Result<(), StoryInstantiationError> {
    if actual > maximum {
        return Err(StoryInstantiationError::LimitExceeded { limit });
    }
    Ok(())
}
