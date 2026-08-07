use aise::core::story_proposal::{StoryProposal, WorldFactEvidenceRef};

#[test]
fn null_optional_fields_default_to_empty() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":null,"character_changes":null,"world_change":null,"memory_changes":null,"summary_change":null}"#,
    )
    .expect("explicit null on optional fields must be accepted");
    assert_eq!(proposal.story_text, "hello");
    assert!(proposal.events.is_empty());
    assert!(proposal.character_changes.is_empty());
    assert!(proposal.world_change.add_facts.is_empty());
    assert!(proposal.memory_changes.is_empty());
    assert!(proposal.summary_change.is_none());
}

#[test]
fn missing_optional_fields_default_to_empty() {
    let proposal: StoryProposal = serde_json::from_str(r#"{"story_text":"hello"}"#).expect("minimal proposal");
    assert_eq!(proposal.story_text, "hello");
    assert!(proposal.events.is_empty());
    assert!(proposal.character_changes.is_empty());
    assert!(proposal.world_change.add_facts.is_empty());
    assert!(proposal.memory_changes.is_empty());
    assert!(proposal.summary_change.is_none());
}

#[test]
fn world_change_null_add_facts_default_to_empty() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[],"world_change":{"add_facts":null},"memory_changes":[],"summary_change":null}"#,
    )
    .expect("world_change with null add_facts must be accepted");
    assert!(proposal.world_change.add_facts.is_empty());
}

#[test]
fn nested_character_change_null_vectors_default_to_empty() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[{"character_id":"char-1","goal_updates":null,"health_delta":null,"affinity_deltas":null}],"world_change":{"add_facts":[]},"memory_changes":[],"summary_change":null}"#,
    )
    .expect("nested null vectors must be accepted");
    let change = &proposal.character_changes[0];
    assert!(change.goal_updates.is_empty());
    assert!(change.health_delta.is_none());
    assert!(change.affinity_deltas.is_empty());
}

#[test]
fn world_facts_parse_as_objects_with_evidence() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[{"kind":"action","summary":"swing the stick"}],"character_changes":[],"world_change":{"add_facts":[{"text":"the player hides behind the door","evidence":[{"proposed_event":{"event_index":0}}]},{"text":"the inn is near the port","evidence":[{"snapshot_fact":"fact-1"}]}]},"memory_changes":[],"summary_change":null}"#,
    )
    .expect("object-form world facts with evidence must be accepted");
    let facts = &proposal.world_change.add_facts;
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].text, "the player hides behind the door");
    assert_eq!(facts[0].evidence, vec![WorldFactEvidenceRef::ProposedEvent { event_index: 0 }]);
    assert_eq!(facts[1].text, "the inn is near the port");
    assert_eq!(facts[1].evidence, vec![WorldFactEvidenceRef::SnapshotFact("fact-1".into())]);
}

#[test]
fn world_fact_missing_or_null_evidence_defaults_to_empty() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[],"world_change":{"add_facts":[{"text":"first fact"},{"text":"second fact","evidence":null}]},"memory_changes":[],"summary_change":null}"#,
    )
    .expect("world facts without evidence must default to empty evidence");
    let facts = &proposal.world_change.add_facts;
    assert_eq!(facts.len(), 2);
    assert!(facts[0].evidence.is_empty());
    assert!(facts[1].evidence.is_empty());
}

#[test]
fn summary_change_plain_string_is_wrapped_as_object() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[],"world_change":{"add_facts":[]},"memory_changes":[],"summary_change":"the updated summary"}"#,
    )
    .expect("plain-string summary_change must be accepted");
    let summary = proposal.summary_change.expect("summary_change must be present");
    assert_eq!(summary.text, "the updated summary");
}

#[test]
fn summary_change_empty_string_defaults_to_none() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[],"world_change":{"add_facts":[]},"memory_changes":[],"summary_change":""}"#,
    )
    .expect("empty-string summary_change must be accepted");
    assert!(proposal.summary_change.is_none());
}

#[test]
fn add_facts_plain_strings_default_to_empty_evidence() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[],"character_changes":[],"world_change":{"add_facts":["the door is locked","the torch is lit"]},"memory_changes":[],"summary_change":null}"#,
    )
    .expect("plain-string world facts must be accepted");
    let facts = &proposal.world_change.add_facts;
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].text, "the door is locked");
    assert!(facts[0].evidence.is_empty());
    assert_eq!(facts[1].text, "the torch is lit");
}
