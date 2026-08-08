use aise::domain::asset::ids::{
    LocationKey, PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey, StoryRoleKey,
};
use aise::domain::ids::StoryRevision;
use aise::domain::ids::{CharacterId, StoryId};
use aise::domain::story_instance::binding::{RoleBinding, StoryInstanceBinding};
use aise::domain::story_instance::snapshot::StoryReadSnapshot;
use aise::domain::story_instance::state::CharacterInstanceState;
use std::collections::BTreeMap;

#[test]
fn binding_resolves_character_for_role() {
    let story_id = StoryId::try_new("story-1").unwrap();
    let role_key = StoryRoleKey::from("protagonist");
    let character_id = CharacterId::from("char-1");
    let binding = StoryInstanceBinding {
        story_id: story_id.clone(),
        pack_id: PackId::from("pack-1"),
        revision: StoryRevision::new(3),
        role_bindings: vec![RoleBinding {
            role_key: role_key.clone(),
            player_id: Some(PlayerId::from("player-1")),
            character_id: character_id.clone(),
            bound_at_ms: 0,
        }],
    };
    assert_eq!(binding.character_id_for_role(&role_key), Some(&character_id));
    assert_eq!(binding.character_id_for_role(&StoryRoleKey::from("missing")), None);
}

#[test]
fn role_binding_captures_player_and_character() {
    let role_key = StoryRoleKey::from("protagonist");
    let character_id = CharacterId::from("char-9");
    let binding = RoleBinding {
        role_key: role_key.clone(),
        player_id: Some(PlayerId::from("player-9")),
        character_id: character_id.clone(),
        bound_at_ms: 42,
    };
    assert_eq!(binding.role_key, role_key);
    assert!(binding.player_id.is_some());
    assert_eq!(binding.character_id, character_id);
}

#[test]
fn snapshot_accessors_roundtrip() {
    let story_id = StoryId::try_new("story-2").unwrap();
    let pack_ref = aise::domain::asset::frozen_ref::FrozenStoryPackRef {
        pack_id: PackId::from("pack-2"),
        pack_key: StoryPackKey::from("demo"),
        version: SemanticVersion::try_new("0.1.0").unwrap(),
        digest: Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
    };
    let role_key = StoryRoleKey::from("protagonist");
    let character_id = CharacterId::from("char-2");
    let mut role_bindings = BTreeMap::new();
    role_bindings.insert(
        role_key.clone(),
        RoleBinding {
            role_key: role_key.clone(),
            player_id: None,
            character_id: character_id.clone(),
            bound_at_ms: 0,
        },
    );
    let mut character_states = BTreeMap::new();
    character_states.insert(
        character_id.clone(),
        CharacterInstanceState {
            character_id,
            role_key,
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    );
    let snapshot = StoryReadSnapshot::new(
        story_id.clone(),
        StoryRevision::new(5),
        pack_ref,
        aise::domain::asset::story_pack::StoryProfile {
            premise: aise::domain::asset::validation::BoundedText::try_new("p", "test", 4096).unwrap(),
            language: aise::domain::asset::validation::BoundedText::try_new("zh-CN", "test", 4096).unwrap(),
            genre: Vec::new(),
            themes: Vec::new(),
            style: aise::domain::asset::story_pack::StoryStyle {
                tone: Vec::new(),
                point_of_view: aise::domain::asset::validation::BoundedText::try_new("third", "test", 4096).unwrap(),
                tense: aise::domain::asset::validation::BoundedText::try_new("past", "test", 4096).unwrap(),
            },
        },
        BTreeMap::new(),
        role_bindings,
        BTreeMap::new(),
        character_states,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        aise::domain::story_instance::snapshot::CurrentScene { text: "scene".into() },
        Vec::new(),
        aise::domain::narrative_graph::definition::NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        aise::domain::narrative_graph::state::NarrativeRuntimeState::initial(),
        Vec::new(),
        Vec::new(),
        aise::domain::narrative::StorySummary { text: String::new() },
        Vec::new(),
    );
    assert_eq!(snapshot.story_id(), &story_id);
    assert_eq!(snapshot.base_revision().get(), 5);
    assert_eq!(snapshot.pack().pack_id.as_str(), "pack-2");
    assert!(snapshot.role_binding(&StoryRoleKey::from("protagonist")).is_some());
    assert!(snapshot.world_facts().is_empty());
    assert!(snapshot.recent_turns().is_empty());
}
