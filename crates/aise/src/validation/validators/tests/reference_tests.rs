use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{RoleId, StoryId, StoryRevision};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::StoryReadSnapshotParts;
use crate::domain::story_instance::state::InstanceSettings;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).unwrap()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn sample_role(id: &str, controller: RoleController, location: &str) -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new(id).unwrap(),
        controller,
        role_label: bounded(id),
        narrative_function: bounded("role"),
        background: None,
        effective_profile: CharacterProfile {
            name: bounded(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character: None,
        state: StoryRoleState {
            location: LocationKey::from(location),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    }
}

fn sample_snapshot(roles: &[StoryRole], entity_catalog: Vec<KnowledgeEntity>) -> StoryReadSnapshot {
    let mut role_map = BTreeMap::new();
    for role in roles {
        role_map.insert(role.role_id.clone(), StoryRoleView::from(role));
    }
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_profile: StoryProfile {
            premise: bounded("premise"),
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
        roles: role_map,
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
        entity_catalog,
        topic_dictionary: BTreeMap::new(),
        knowledge_snapshot: crate::domain::story_instance::snapshot::KnowledgeSnapshotRef {
            story_id: StoryId::try_new("story-1").unwrap(),
            pack_digest: digest(),
            base_revision: StoryRevision::new(0),
        },
    })
    .unwrap()
}

#[test]
fn has_duplicates_detects_repeats() {
    let values = vec![LocationKey::from("a"), LocationKey::from("b"), LocationKey::from("a")];
    assert!(has_duplicates(&values));
    let unique = vec![LocationKey::from("a"), LocationKey::from("b")];
    assert!(!has_duplicates(&unique));
}

#[test]
fn location_key_resolves_against_role_state_or_catalog() {
    let roles = vec![
        sample_role(
            "protagonist",
            RoleController::Player(PlayerId::try_new("player-1").unwrap()),
            "start",
        ),
        sample_role("npc", RoleController::Ai, "village"),
    ];
    let snapshot = sample_snapshot(&roles, vec![KnowledgeEntity::Location(LocationKey::from("cave"))]);
    assert!(location_key_resolves(&LocationKey::from("village"), &snapshot));
    assert!(location_key_resolves(&LocationKey::from("cave"), &snapshot));
    assert!(!location_key_resolves(&LocationKey::from("unknown"), &snapshot));
}

#[test]
fn reference_validator_is_default_constructible() {
    let _validator = ReferenceValidator;
}
