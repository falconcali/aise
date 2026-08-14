use super::*;
use crate::domain::asset::ids::LocationKey;

#[test]
fn has_duplicates_detects_repeats() {
    let values = vec![LocationKey::from("a"), LocationKey::from("b"), LocationKey::from("a")];
    assert!(has_duplicates(&values));
    let unique = vec![LocationKey::from("a"), LocationKey::from("b")];
    assert!(!has_duplicates(&unique));
}

#[test]
fn location_key_resolves_against_current_or_catalog() {
    let current = LocationKey::from("current");
    let other = LocationKey::from("other");
    assert!(location_key_resolves(&current, &current, &[]));
    assert!(!location_key_resolves(&other, &current, &[]));
}

#[test]
fn reference_validator_is_default_constructible() {
    let _validator = ReferenceValidator;
}
