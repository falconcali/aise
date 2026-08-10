use crate::core::turn_validation::{
    StateChange, ValidatedChangeSet, ValidatedChangeSetParts, ValidationDecision, ValidationResult,
};
use crate::domain::asset::validation::BoundedText;
use crate::domain::story_instance::snapshot::NarrativeConditionStateView;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn pass_cannot_contain_issues() {
    let change_set = ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text: BoundedText::try_new("story text", "story_text", 100).expect("bounded text"),
        events: Vec::new(),
        character_changes: Vec::new(),
        relationship_changes: Vec::new(),
        knowledge_additions: Vec::new(),
        current_perceptions: Vec::new(),
        scene_change: StateChange::Unchanged,
        narrative_changes: Vec::new(),
        condition_state: NarrativeConditionStateView {
            occurred_event_keys: BTreeSet::new(),
            player_action_event_keys: BTreeSet::new(),
            fact_values: BTreeMap::new(),
        },
        constraint_change: StateChange::Unchanged,
        summary_change: StateChange::Unchanged,
    })
    .expect("valid change set");
    let result = ValidationResult::pass(change_set);
    assert_eq!(result.decision(), ValidationDecision::Pass);
    assert!(result.issues().is_empty(), "Pass must expose an empty issue slice");
    assert!(result.into_change_set().is_some(), "Pass carries the validated change set");
}
