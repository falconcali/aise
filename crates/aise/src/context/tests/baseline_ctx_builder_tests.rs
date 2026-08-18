use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{LocationKey, PackId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{FactId, StoryId, StoryRevision};
use crate::domain::knowledge::{KnowledgeSourceId, RetrievalHint};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::RelevantWorldKnowledgeItem;
use crate::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeIndexRecord, KnowledgeLookupHit, KnowledgeRecord, SourceKnowledgeQuery,
    TopicKnowledgeQuery,
};
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use std::collections::BTreeMap;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 1024).unwrap()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn fact_id(seq: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(seq).unwrap())
}

fn sample_snapshot() -> StoryReadSnapshot {
    let mut roles = BTreeMap::new();
    roles.insert(
        RoleId::try_new("protagonist").unwrap(),
        StoryRoleView {
            role_id: RoleId::try_new("protagonist").unwrap(),
            controller: RoleController::Player(crate::domain::asset::ids::PlayerId::try_new("player-1").unwrap()),
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
    );
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
        entity_catalog: Vec::new(),
        topic_dictionary: BTreeMap::new(),
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

struct FakeIndexPort {
    records: Vec<KnowledgeIndexRecord>,
}

#[async_trait]
impl KnowledgeReadPort for FakeIndexPort {
    async fn find_by_entities(&self, _query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError> {
        Ok(Vec::new())
    }

    async fn find_by_topics(&self, _query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError> {
        Ok(Vec::new())
    }

    async fn find_by_source_ids(&self, _query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn list_index(&self, _query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
        Ok(self.records.clone())
    }
}

#[tokio::test]
async fn provided_world_knowledge_is_not_indexed() {
    let snapshot = sample_snapshot();
    let relevant = RelevantWorldKnowledge {
        facts: vec![RelevantWorldKnowledgeItem {
            source_id: fact_id("fact_0001"),
            content: bounded("provided fact"),
            source_priority: 0,
            salience: 1,
        }],
        rumors: Vec::new(),
    };
    let port: std::sync::Arc<dyn KnowledgeReadPort> = std::sync::Arc::new(FakeIndexPort {
        records: vec![
            KnowledgeIndexRecord {
                source_id: fact_id("fact_0001"),
                retrieval_hint: RetrievalHint::try_new("hint-a".to_owned()).unwrap(),
            },
            KnowledgeIndexRecord {
                source_id: fact_id("fact_0002"),
                retrieval_hint: RetrievalHint::try_new("hint-b".to_owned()).unwrap(),
            },
        ],
    });
    let config = RetrievalConfig::default();

    let entries = load_knowledge_index(&snapshot, &relevant, &config, &port).await.unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source_id, fact_id("fact_0002"));
}

#[test]
fn role_context_projection_uses_one_story_role_view() {
    let role = StoryRoleView {
        role_id: RoleId::try_new("guard").unwrap(),
        controller: RoleController::Ai,
        role_label: bounded("Guard Captain"),
        narrative_function: bounded("blocks the gate"),
        background: Some(bounded("secret orders")),
        effective_profile: CharacterProfile {
            name: bounded("Guard"),
            appearance: Some(bounded("scarred")),
            personality: Some(bounded("watchful")),
            speaking_style: Some(bounded("formal")),
            dialogue_examples: Vec::new(),
        },
        source_character_id: None,
        state: StoryRoleState {
            location: LocationKey::from("gate"),
            goals: vec![bounded("hold")],
            attributes: BTreeMap::new(),
        },
    };
    let projected = project_role_context(&role);
    assert_eq!(projected.role_id, role.role_id);
    assert_eq!(projected.role_label, role.role_label);
    assert_eq!(projected.narrative_function, role.narrative_function);
    assert_eq!(projected.background, role.background);
    assert_eq!(projected.profile, role.effective_profile);
    assert_eq!(projected.state, role.state);
    assert_eq!(projected.controller, role.controller);
}
