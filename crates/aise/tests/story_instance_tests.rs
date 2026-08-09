use aise::domain::CurrentScene as DomainScene;
use aise::domain::asset::ids::{LocationKey, PackId, PlayerId, SceneKey, StoryRoleKey};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{CharacterId, StoryId, StoryRevision};
use aise::domain::story_instance::binding::{RoleBinding, StoryInstanceBinding};
use aise::domain::story_instance::state::{CharacterInstanceState, CurrentScene};
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
}

#[test]
fn current_scene_is_structured() {
    let scene = CurrentScene {
        scene_key: SceneKey::from("scene_1"),
        location_key: LocationKey::from("village"),
        time: BoundedText::try_new("morning", "time", 64).unwrap(),
        description: BoundedText::try_new("The village wakes.", "desc", 256).unwrap(),
        present_character_ids: vec![CharacterId::from("char-1")],
    };
    let domain: DomainScene = scene.clone();
    assert_eq!(domain.scene_key.as_str(), "scene_1");
    assert_eq!(domain.present_character_ids.len(), 1);
    let mut states = BTreeMap::new();
    states.insert(
        CharacterId::from("char-1"),
        CharacterInstanceState {
            character_id: CharacterId::from("char-1"),
            role_key: StoryRoleKey::from("protagonist"),
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    );
    assert_eq!(states.len(), 1);
}
