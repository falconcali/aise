use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::{LocationKey, PlayerId};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{FactId, MemoryId, RoleId, RumorId, TurnNumber};
use crate::domain::knowledge::{KnowledgeSource, KnowledgeSourceId};
use crate::domain::story_instance::role::{RoleController, StoryRoleState};
use std::collections::BTreeMap;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value.to_owned(), "text", 256).unwrap()
}

fn limits() -> RetrievedContextLimits {
    RetrievedContextLimits {
        max_role_audiences: 8,
        max_items_per_audience: 8,
        max_tokens_per_audience: 10_000,
        max_total_items: 32,
        max_total_tokens: 10_000,
        max_item_bytes: 4096,
    }
}

fn item(id: KnowledgeSourceId, body: &str) -> RetrievedKnowledgeItem {
    RetrievedKnowledgeItem::from_parts(
        id,
        text(body),
        KnowledgeSource::CommittedTurn {
            turn_number: TurnNumber::try_new(1).unwrap(),
        },
        RelevanceRank {
            match_level: MatchLevel::Entity,
            signal_priority: 0,
            salience: 50,
        },
        BTreeMap::new(),
    )
}

fn role_view(id: &str) -> RoleContextView {
    RoleContextView {
        role_id: RoleId::try_new(id).unwrap(),
        role_label: text(id),
        narrative_function: text("narrative-function"),
        background: None,
        profile: CharacterProfile {
            name: text(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        state: StoryRoleState {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        controller: RoleController::Player(PlayerId::try_new("player-1").unwrap()),
    }
}

fn fact_id(sequence: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(format!("fact_{sequence}")).unwrap())
}

fn rumor_id(sequence: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Rumor(RumorId::try_new(format!("rumor_{sequence}")).unwrap())
}

fn memory_id(sequence: &str) -> KnowledgeSourceId {
    KnowledgeSourceId::Memory(MemoryId::try_new(format!("memory_{sequence}")).unwrap())
}

#[test]
fn retrieved_context_error_partition_variants_map_to_partition_invalid_code() {
    for error in [
        RetrievedContextError::InvalidKind,
        RetrievedContextError::InvalidMemoryOwner,
        RetrievedContextError::InvalidRole,
        RetrievedContextError::ConflictingDuplicate,
    ] {
        assert_eq!(error.turn_code(), "retrieval_partition_invalid");
    }
}

#[test]
fn retrieved_context_error_limit_variants_map_to_context_limit_code() {
    for error in [
        RetrievedContextError::CountLimit {
            limit: "max_total_items",
        },
        RetrievedContextError::ItemByteLimit,
        RetrievedContextError::AudienceTokenLimit,
        RetrievedContextError::TotalTokenLimit,
        RetrievedContextError::ArithmeticOverflow,
    ] {
        assert_eq!(error.turn_code(), "retrieval_context_limit");
    }
}

#[test]
fn retrieval_partition_invalid_rejects_memory_returned_for_writer_delivery() {
    let world = RetrievedWorldKnowledge {
        facts: Vec::new(),
        rumors: vec![item(memory_id("0001"), "leaked memory")],
    };
    let err = RetrievedContext::try_new(world, BTreeMap::new(), limits()).unwrap_err();
    assert!(matches!(err, RetrievedContextError::InvalidKind));
    assert_eq!(err.turn_code(), "retrieval_partition_invalid");
}

#[test]
fn retrieval_partition_invalid_rejects_fact_returned_for_character_delivery() {
    let role_id = RoleId::try_new("npc-a").unwrap();
    let mut characters = BTreeMap::new();
    characters.insert(
        role_id,
        RetrievedCharacterContext {
            role: None,
            known_rumors: vec![item(fact_id("0001"), "leaked fact")],
            memories: Vec::new(),
        },
    );
    let err = RetrievedContext::try_new(RetrievedWorldKnowledge::default(), characters, limits()).unwrap_err();
    assert!(matches!(err, RetrievedContextError::InvalidKind));
    assert_eq!(err.turn_code(), "retrieval_partition_invalid");
}

#[test]
fn retrieval_partition_invalid_rejects_memory_in_known_rumors_bucket() {
    let role_id = RoleId::try_new("npc-a").unwrap();
    let mut characters = BTreeMap::new();
    characters.insert(
        role_id,
        RetrievedCharacterContext {
            role: None,
            known_rumors: vec![item(memory_id("0002"), "leaked memory")],
            memories: Vec::new(),
        },
    );
    let err = RetrievedContext::try_new(RetrievedWorldKnowledge::default(), characters, limits()).unwrap_err();
    assert!(matches!(err, RetrievedContextError::InvalidKind));
    assert_eq!(err.turn_code(), "retrieval_partition_invalid");
}

#[test]
fn retrieval_partition_invalid_rejects_role_view_id_mismatch() {
    let role_id = RoleId::try_new("npc-a").unwrap();
    let mut characters = BTreeMap::new();
    characters.insert(
        role_id,
        RetrievedCharacterContext {
            role: Some(role_view("npc-b")),
            known_rumors: Vec::new(),
            memories: Vec::new(),
        },
    );
    let err = RetrievedContext::try_new(RetrievedWorldKnowledge::default(), characters, limits()).unwrap_err();
    assert!(matches!(err, RetrievedContextError::InvalidRole));
    assert_eq!(err.turn_code(), "retrieval_partition_invalid");
}

#[test]
fn retrieval_partition_invalid_rejects_conflicting_duplicate_ids() {
    let world = RetrievedWorldKnowledge {
        facts: vec![item(fact_id("0001"), "version a"), item(fact_id("0001"), "version b")],
        rumors: Vec::new(),
    };
    let err = RetrievedContext::try_new(world, BTreeMap::new(), limits()).unwrap_err();
    assert!(matches!(err, RetrievedContextError::ConflictingDuplicate));
    assert_eq!(err.turn_code(), "retrieval_partition_invalid");
}

#[test]
fn retrieved_context_accepts_correctly_partitioned_content() {
    let role_id = RoleId::try_new("npc-a").unwrap();
    let mut characters = BTreeMap::new();
    characters.insert(
        role_id.clone(),
        RetrievedCharacterContext {
            role: Some(role_view("npc-a")),
            known_rumors: vec![item(rumor_id("0001"), "a rumor")],
            memories: vec![item(memory_id("0002"), "a memory")],
        },
    );
    let world = RetrievedWorldKnowledge {
        facts: vec![item(fact_id("0003"), "a fact")],
        rumors: Vec::new(),
    };
    let context = RetrievedContext::try_new(world, characters, limits()).expect("valid partition");
    assert_eq!(context.total_items(), 3);
    assert!(context.character(&role_id).is_some());
}
