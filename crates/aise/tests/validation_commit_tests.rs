use aise::core::turn_validation::{StateChange, ValidationDecision};
use aise::domain::ids::StoryRevision;

#[test]
fn state_change_and_validation_decision_contracts() {
    let unchanged: StateChange<StoryRevision> = StateChange::Unchanged;
    assert!(matches!(unchanged, StateChange::Unchanged));
    let replaced = StateChange::Replace(StoryRevision::new(2));
    assert!(matches!(replaced, StateChange::Replace(_)));
    assert_eq!(ValidationDecision::Pass, ValidationDecision::Pass);
    assert_ne!(ValidationDecision::Pass, ValidationDecision::Reject);
}
