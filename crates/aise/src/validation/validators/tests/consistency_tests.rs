use super::*;
use crate::domain::asset::ids::{LocationKey, SceneKey};

#[test]
fn current_scene_and_location_resolve_without_catalog_entries() {
    let scene = SceneKey::from("scene_1");
    let location = LocationKey::from("village");

    assert!(scene_key_resolves(&scene, &scene, &[]));
    assert!(location_key_resolves(&location, &location, &[]));
}

#[test]
fn unknown_scene_and_location_still_require_catalog_entries() {
    let current_scene = SceneKey::from("scene_1");
    let current_location = LocationKey::from("village");

    assert!(!scene_key_resolves(&SceneKey::from("scene_2"), &current_scene, &[]));
    assert!(!location_key_resolves(&LocationKey::from("forest"), &current_location, &[],));
}
