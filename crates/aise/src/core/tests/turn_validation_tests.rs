use crate::core::turn_validation::{
    StateChange, StoryStateChanges, ValidatedChangeSet, ValidationDecision, ValidationResult,
};

#[test]
fn pass_cannot_contain_issues() {
    let change_set = ValidatedChangeSet::new(
        "story text".into(),
        Vec::new(),
        Vec::new(),
        StateChange::Unchanged,
        Vec::new(),
        StoryStateChanges {
            scene_change: StateChange::Unchanged,
            constraint_change: StateChange::Unchanged,
            summary_change: StateChange::Unchanged,
        },
    )
    .expect("valid change set");
    let result = ValidationResult::pass(change_set);
    assert_eq!(result.decision(), ValidationDecision::Pass);
    assert!(result.issues().is_empty(), "Pass must expose an empty issue slice");
    assert!(result.into_change_set().is_some(), "Pass carries the validated change set");
}
