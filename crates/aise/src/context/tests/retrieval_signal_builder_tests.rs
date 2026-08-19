use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{LocationKey, PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey, TopicKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::asset::validation::BoundedText;
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{RoleId, StoryId, StoryRevision};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshot, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use std::collections::BTreeMap;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 1024).unwrap()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn snapshot() -> StoryReadSnapshot {
    let role_id = RoleId::try_new("protagonist").unwrap();
    let roles = BTreeMap::from([(
        role_id.clone(),
        StoryRoleView {
            role_id,
            controller: RoleController::Player(PlayerId::try_new("player-1").unwrap()),
            role_label: bounded("Protagonist"),
            narrative_function: bounded("hero"),
            background: None,
            effective_profile: CharacterProfile {
                name: bounded("Hero"),
                appearance: None,
                personality: None,
                speaking_style: None,
                dialogue_examples: Vec::new(),
            },
            source_character_id: None,
            state: StoryRoleState {
                location: LocationKey::from("hall"),
                goals: Vec::new(),
                attributes: BTreeMap::new(),
            },
        },
    )]);
    let topic_dictionary = BTreeMap::from([(
        TopicKey::try_new("storm").unwrap(),
        TopicDefinition {
            label: bounded("storm"),
            aliases: vec![bounded("tempest")],
        },
    )]);
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_title: bounded("Untitled Story"),
        story_profile: StoryProfile {
            language: bounded("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            style: StoryStyle {
                tone: Vec::new(),
                point_of_view: bounded("third"),
                tense: bounded("past"),
            },
        },
        instance_settings: InstanceSettings::default(),
        roles,
        relationships: Vec::new(),
        narrative_definition: NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        narrative_state: NarrativeRuntimeState::initial(),
        fact_values: BTreeMap::new(),
        story_continuity: StoryContinuity::try_new(
            StorySummary {
                text: bounded(""),
                summarized_through: None,
            },
            Vec::new(),
            StoryContinuityLimits {
                max_summary_bytes: 256,
                max_recent_segments: 4,
                max_recent_segment_bytes: 128,
                max_recent_segment_tokens: 32,
            },
        )
        .unwrap(),
        active_constraints: Vec::new(),
        entity_catalog: vec![KnowledgeEntity::Location(LocationKey::from("gate"))],
        topic_dictionary,
        knowledge_snapshot: KnowledgeSnapshotRef {
            story_id: StoryId::try_new("story-1").unwrap(),
            pack_digest: digest(),
            base_revision: StoryRevision::new(0),
            knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::zero(),
        },
        role_id_high_water: crate::domain::ids::RoleIdHighWater::zero(),
    })
    .unwrap()
}

#[test]
fn contribution_entity_and_topic_signals_use_player_contribution_origin() {
    let signals = RetrievalSignalBuilder::new(ContextPreparationConfig::default())
        .build(&snapshot(), "I approach the gate during the storm")
        .unwrap();
    let entity = signals
        .entities
        .iter()
        .find(|signal| signal.entity == KnowledgeEntity::Location(LocationKey::from("gate")))
        .unwrap();
    let topic = signals
        .topics
        .iter()
        .find(|signal| signal.topic == TopicKey::try_new("storm").unwrap())
        .unwrap();
    assert_eq!(entity.origin, RetrievalSignalOrigin::PlayerContribution);
    assert_eq!(topic.origin, RetrievalSignalOrigin::PlayerContribution);
    assert_eq!(serde_json::to_value(entity).unwrap()["origin"], "player_contribution");
    assert_eq!(serde_json::to_value(topic).unwrap()["origin"], "player_contribution");
}
