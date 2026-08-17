use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::ValidatedNarrativeResolution;
use crate::turn::turn_validation::{
    StateChange, ValidatedChangeSet, ValidatedChangeSetParts, ValidationDecision, ValidationResult,
};
use std::collections::BTreeMap;

fn narrative_resolution() -> ValidatedNarrativeResolution {
    ValidatedNarrativeResolution {
        candidate_version: crate::domain::turn::StoryCandidateVersion {
            content_digest: crate::domain::asset::ids::Sha256Digest::from_bytes([0u8; 32]),
            repair_attempt: 0,
        },
        transitions: Vec::new(),
        condition_results: BTreeMap::new(),
        pending_effects: Vec::new(),
        next_graph_revision: 1,
    }
}

#[test]
fn pass_cannot_contain_issues() {
    let change_set = ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text: BoundedText::try_new("story text", "story_text", 100).expect("bounded text"),
        role_changes: Vec::new(),
        relationship_changes: Vec::new(),
        knowledge_mutations: Vec::new(),
        narrative_events: Vec::new(),
        narrative_resolution: narrative_resolution(),
        constraint_change: StateChange::Unchanged,
    })
    .expect("valid change set");
    let result = ValidationResult::pass(change_set);
    assert_eq!(result.decision(), ValidationDecision::Pass);
    assert!(result.issues().is_empty(), "Pass must expose an empty issue slice");
    assert!(result.into_change_set().is_some(), "Pass carries the validated change set");
}

#[test]
fn validated_change_set_has_no_scene_contract() {
    let change_set = ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text: BoundedText::try_new("story text", "story_text", 100).expect("bounded text"),
        role_changes: Vec::new(),
        relationship_changes: Vec::new(),
        knowledge_mutations: Vec::new(),
        narrative_events: Vec::new(),
        narrative_resolution: narrative_resolution(),
        constraint_change: StateChange::Unchanged,
    })
    .expect("valid change set");
    assert_eq!(change_set.narrative_events().len(), 0);
}
