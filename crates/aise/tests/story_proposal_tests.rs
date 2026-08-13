use aise::domain::turn::proposal::{ProposedKnowledgeChange, StoryProposal, WorldFactEvidenceRef};

#[test]
fn omitted_collections_default_to_empty() {
    let proposal: StoryProposal =
        serde_json::from_str(r#"{"story_text":"hello","scene_change":null,"summary_text":null}"#)
            .expect("minimal strict proposal");
    assert_eq!(proposal.story_text, "hello");
    assert!(proposal.events.is_empty());
    assert!(proposal.character_changes.is_empty());
    assert!(proposal.relationship_changes.is_empty());
    assert!(proposal.knowledge_changes.is_empty());
    assert!(proposal.perceptions.is_empty());
}

#[test]
fn omitted_optional_fields_default_to_none() {
    let proposal: StoryProposal = serde_json::from_str(r#"{"story_text":"hello"}"#).expect("optional fields");
    assert!(proposal.scene_change.is_none());
    assert!(proposal.summary_text.is_none());
}

#[test]
fn empty_story_text_fails_proposal_bounds() {
    let proposal: StoryProposal = serde_json::from_str(r#"{"story_text":"  "}"#).expect("proposal shape");

    assert!(!proposal.is_within_bounds(8, 1024, 8192));
}

#[test]
fn unknown_top_level_and_nested_fields_are_rejected() {
    assert!(
        serde_json::from_str::<StoryProposal>(
            r#"{"story_text":"hello","scene_change":null,"summary_text":null,"constraints":[]}"#,
        )
        .is_err()
    );
    assert!(serde_json::from_str::<StoryProposal>(
        r#"{"story_text":"hello","character_changes":[{"character_id":"char-1","location":null,"goals":null,"attribute_updates":{},"health_delta":1}],"scene_change":null,"summary_text":null}"#,
    )
    .is_err());
}

#[test]
fn character_patch_uses_the_final_contract() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","character_changes":[{"character_id":"char-1","location":null,"goals":[],"attribute_updates":{}}],"scene_change":null,"summary_text":null}"#,
    )
    .expect("strict character patch");
    let change = &proposal.character_changes[0];
    assert_eq!(change.character_id.as_str(), "char-1");
    assert_eq!(change.goals, Some(Vec::new()));
}

#[test]
fn fact_changes_parse_with_typed_evidence() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"hello","events":[{"kind":"action","summary":"swing"}],"knowledge_changes":[{"kind":"fact","content":"the door is locked","proposition":null,"entities":[],"topics":[],"salience":50,"evidence":[{"proposed_event":{"event_index":0}}]}],"scene_change":null,"summary_text":null}"#,
    )
    .expect("strict fact change");
    let ProposedKnowledgeChange::Fact { evidence, .. } = &proposal.knowledge_changes[0] else {
        panic!("fact expected");
    };
    assert_eq!(evidence, &[WorldFactEvidenceRef::ProposedEvent { event_index: 0 }]);
}

#[test]
fn semantic_keys_accept_unicode_model_output() {
    let proposal: StoryProposal = serde_json::from_str(
        r#"{"story_text":"继续","relationship_changes":[{"source_character_id":"char-1","target_character_id":"char-2","kind":"信任关系","trust_delta":1}],"knowledge_changes":[{"kind":"memory","owner":"char-1","memory_kind":"亲眼所见","content":"发现纸条","entities":[],"topics":["木屋","纸条"],"salience":50,"source_event_index":null}],"scene_change":null,"summary_text":null}"#,
    )
    .expect("unicode semantic keys");

    let ProposedKnowledgeChange::Memory {
        memory_kind, topics, ..
    } = &proposal.knowledge_changes[0]
    else {
        panic!("memory expected");
    };
    assert_eq!(proposal.relationship_changes[0].kind.as_str(), "信任关系");
    assert_eq!(memory_kind.as_str(), "亲眼所见");
    assert_eq!(topics.iter().map(|topic| topic.as_str()).collect::<Vec<_>>(), ["木屋", "纸条"]);
}

#[test]
fn semantic_keys_reject_blank_and_control_characters() {
    assert!(serde_json::from_str::<StoryProposal>(
        r#"{"story_text":"继续","knowledge_changes":[{"kind":"fact","content":"发现纸条","proposition":null,"entities":[],"topics":["   "],"salience":50,"evidence":[]}]}"#,
    )
    .is_err());
    assert!(serde_json::from_str::<StoryProposal>(
        "{\"story_text\":\"继续\",\"knowledge_changes\":[{\"kind\":\"fact\",\"content\":\"发现纸条\",\"proposition\":null,\"entities\":[],\"topics\":[\"纸条\\u0000\"],\"salience\":50,\"evidence\":[]}]}"
    )
    .is_err());
}

#[test]
fn summary_boundary_and_constraint_fields_are_rejected() {
    assert!(
        serde_json::from_str::<StoryProposal>(
            r#"{"story_text":"hello","scene_change":null,"summary_text":{"text":"past","summarized_through":1}}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<StoryProposal>(
            r#"{"story_text":"hello","scene_change":null,"summary_text":null,"constraint_change":[]}"#,
        )
        .is_err()
    );
}
