use crate::domain::asset::validation::BoundedText;
use crate::domain::story_instance::snapshot::NarrativeConditionStateView;
use crate::domain::story_instance::state::CurrentScene;
use crate::turn::turn_validation::{
    StateChange, ValidatedChangeSet, ValidatedChangeSetParts, ValidationDecision, ValidationResult,
};
use std::collections::{BTreeMap, BTreeSet};

fn current_scene() -> CurrentScene {
    CurrentScene {
        scene_key: crate::domain::asset::ids::SceneKey::from("scene_1"),
        location_key: crate::domain::asset::ids::LocationKey::from("village"),
        time: BoundedText::try_new("morning", "time", 100).expect("bounded text"),
        description: BoundedText::try_new("scene", "description", 100).expect("bounded text"),
        present_character_ids: Vec::new(),
    }
}

#[test]
fn pass_cannot_contain_issues() {
    let change_set = ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text: BoundedText::try_new("story text", "story_text", 100).expect("bounded text"),
        character_changes: Vec::new(),
        relationship_changes: Vec::new(),
        knowledge_mutations: Vec::new(),
        current_scene: current_scene(),
        narrative_events: Vec::new(),
        narrative_changes: Vec::new(),
        condition_state: NarrativeConditionStateView {
            occurred_event_keys: BTreeSet::new(),
            player_action_event_keys: BTreeSet::new(),
            fact_values: BTreeMap::new(),
        },
        constraint_change: StateChange::Unchanged,
    })
    .expect("valid change set");
    let result = ValidationResult::pass(change_set);
    assert_eq!(result.decision(), ValidationDecision::Pass);
    assert!(result.issues().is_empty(), "Pass must expose an empty issue slice");
    assert!(result.into_change_set().is_some(), "Pass carries the validated change set");
}
