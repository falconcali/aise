use aise::core::story_proposal::StoryProposal;

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
