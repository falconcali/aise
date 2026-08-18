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

fn parts(story_text: &str) -> ValidatedChangeSetParts {
    ValidatedChangeSetParts {
        story_text: BoundedText::try_new(story_text, "story_text", 100).expect("bounded text"),
        new_roles: Vec::new(),
        role_changes: Vec::new(),
        relationship_operations: Vec::new(),
        knowledge_mutations: Vec::new(),
        knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::zero(),
        next_role_id_high_water: crate::domain::ids::RoleIdHighWater::zero(),
        narrative_events: Vec::new(),
        narrative_resolution: narrative_resolution(),
        constraint_change: StateChange::Unchanged,
    }
}

#[test]
fn pass_cannot_contain_issues() {
    let change_set = ValidatedChangeSet::new(parts("story text")).expect("valid change set");
    let result = ValidationResult::pass(change_set);
    assert_eq!(result.decision(), ValidationDecision::Pass);
    assert!(result.issues().is_empty(), "Pass must expose an empty issue slice");
    assert!(result.into_change_set().is_some(), "Pass carries the validated change set");
}

#[test]
fn validated_change_set_has_no_scene_contract() {
    let change_set = ValidatedChangeSet::new(parts("story text")).expect("valid change set");
    assert_eq!(change_set.narrative_events().len(), 0);
}
