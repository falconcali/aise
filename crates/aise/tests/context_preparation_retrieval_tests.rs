use aise::config::{PlannerConfig, RetrievalConfig};
use aise::context::TextMatcher;
use aise::domain::asset::ids::TopicKey;
use aise::domain::asset::validation::BoundedText;
use aise::domain::asset::world_book::{TopicDefinition, TopicDictionaryError, validate_topic_dictionary};
use aise::domain::ids::CharacterId;
use aise::domain::knowledge::KnowledgeKind;
use aise::domain::text::estimate_text_tokens;
use aise::domain::turn::{
    CandidateRetrieverKind, CharacterThinkRequest, RetrievalAudience, RetrievalPlan, RetrievalRequest,
    RetrievalRequestOrigin, WriterPlan, WriterStoryGoal,
};
use aise::planning::planner_output::PlannerOutput;
use aise::planning::retrieval_plan_builder::RetrievalPlanBuilder;
use std::collections::BTreeMap;

#[test]
fn topic_dictionary_rejects_normalized_alias_collisions() {
    let mut dictionary = BTreeMap::new();
    dictionary.insert(
        TopicKey::from("gate"),
        TopicDefinition {
            label: BoundedText::try_new("Gate", "label", 64).unwrap(),
            aliases: vec![BoundedText::try_new("the gate", "alias", 64).unwrap()],
        },
    );
    dictionary.insert(
        TopicKey::from("door"),
        TopicDefinition {
            label: BoundedText::try_new("Door", "label", 64).unwrap(),
            aliases: vec![BoundedText::try_new(" The  Gate ", "alias", 64).unwrap()],
        },
    );
    let err = validate_topic_dictionary(&dictionary).expect_err("collision");
    assert!(matches!(err, TopicDictionaryError::AliasCollision { .. }));
}

#[test]
fn topic_matcher_handles_ascii_boundaries_and_chinese_aliases() {
    let mut dictionary = BTreeMap::new();
    dictionary.insert(
        TopicKey::from("cat"),
        TopicDefinition {
            label: BoundedText::try_new("cat", "label", 64).unwrap(),
            aliases: Vec::new(),
        },
    );
    dictionary.insert(
        TopicKey::from("temple"),
        TopicDefinition {
            label: BoundedText::try_new("古寺", "label", 64).unwrap(),
            aliases: vec![BoundedText::try_new("山门", "alias", 64).unwrap()],
        },
    );
    let matcher = TextMatcher;
    assert!(matcher.match_topics("concatenate", &dictionary).is_empty());
    assert_eq!(matcher.match_topics("a cat sat", &dictionary), vec![TopicKey::from("cat")]);
    assert_eq!(matcher.match_topics("走近山门", &dictionary), vec![TopicKey::from("temple")]);
}

#[test]
fn planner_output_rejects_provider_and_budget_fields() {
    for payload in [
        r#"{"story_goal":{"summary":"x"},"provider":"entity"}"#,
        r#"{"story_goal":{"summary":"x"},"budget":10}"#,
        r#"{"story_goal":{"summary":"x"},"top_k":3}"#,
        r#"{"story_goal":{"summary":"x"},"retriever":"bm25"}"#,
        r#"{"story_goal":{"summary":"x"},"narrative_plan":{}}"#,
        r#"{"story_goal":{"summary":"x"},"active_constraints":[]}"#,
    ] {
        assert!(serde_json::from_str::<PlannerOutput>(payload).is_err(), "must reject {payload}");
    }
}

#[test]
fn planner_cannot_replace_narrative_plan_or_constraints() {
    assert!(serde_json::from_str::<PlannerOutput>(
        r#"{"story_goal":{"summary":"x"},"narrative_plan":{"active_nodes":[]}}"#,
    )
    .is_err());
    assert!(
        serde_json::from_str::<PlannerOutput>(r#"{"story_goal":{"summary":"x"},"active_story_constraints":[]}"#,)
            .is_err()
    );
}

#[test]
fn automatic_requests_run_when_planner_gaps_are_empty() {
    let plan = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: BoundedText::try_new("goal", "goal", 64).unwrap(),
        },
        retrieval_plan: RetrievalPlan {
            requests: vec![RetrievalRequest {
                audience: RetrievalAudience::GlobalWriter,
                target_source_id: None,
                knowledge_kinds: vec![KnowledgeKind::Fact],
                entities: Vec::new(),
                topics: vec![TopicKey::from("gate")],
                query_text: None,
                authorized_memory_owners: Vec::new(),
                reason: BoundedText::try_new("automatic", "reason", 64).unwrap(),
                origin: RetrievalRequestOrigin::Automatic,
                signal_priority: 0,
            }],
        },
        character_think_requests: Vec::new(),
    };
    assert!(!plan.retrieval_plan.requests.is_empty());
    assert!(plan.character_think_requests.is_empty());
    let _ = RetrievalPlanBuilder::new(RetrievalConfig::default(), PlannerConfig::default());
}

#[test]
fn retrieval_plan_merge_is_deterministic() {
    let mut left = vec![
        RetrievalRequest {
            audience: RetrievalAudience::GlobalWriter,
            target_source_id: None,
            knowledge_kinds: vec![KnowledgeKind::Fact],
            entities: Vec::new(),
            topics: vec![TopicKey::from("b")],
            query_text: None,
            authorized_memory_owners: Vec::new(),
            reason: BoundedText::try_new("b", "reason", 64).unwrap(),
            origin: RetrievalRequestOrigin::Automatic,
            signal_priority: 1,
        },
        RetrievalRequest {
            audience: RetrievalAudience::GlobalWriter,
            target_source_id: None,
            knowledge_kinds: vec![KnowledgeKind::Fact],
            entities: Vec::new(),
            topics: vec![TopicKey::from("a")],
            query_text: None,
            authorized_memory_owners: Vec::new(),
            reason: BoundedText::try_new("a", "reason", 64).unwrap(),
            origin: RetrievalRequestOrigin::Automatic,
            signal_priority: 0,
        },
    ];
    let mut right = left.clone();
    right.reverse();
    left.sort_by(|a, b| a.signal_priority.cmp(&b.signal_priority).then_with(|| a.topics.cmp(&b.topics)));
    right.sort_by(|a, b| a.signal_priority.cmp(&b.signal_priority).then_with(|| a.topics.cmp(&b.topics)));
    let left_json = serde_json::to_string(&RetrievalPlan { requests: left }).unwrap();
    let right_json = serde_json::to_string(&RetrievalPlan { requests: right }).unwrap();
    assert_eq!(left_json, right_json);
}

#[test]
fn context_and_llm_accounting_share_one_token_estimator() {
    assert_eq!(estimate_text_tokens("abcd"), 1);
    assert_eq!(estimate_text_tokens("abcde"), 2);
}

#[test]
fn retrieval_and_character_think_are_enabled_from_plan_collections() {
    let empty = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: BoundedText::try_new("goal", "goal", 64).unwrap(),
        },
        retrieval_plan: RetrievalPlan::default(),
        character_think_requests: Vec::new(),
    };
    assert!(empty.retrieval_plan.requests.is_empty());
    assert!(empty.character_think_requests.is_empty());
    let filled = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: BoundedText::try_new("goal", "goal", 64).unwrap(),
        },
        retrieval_plan: RetrievalPlan {
            requests: vec![RetrievalRequest {
                audience: RetrievalAudience::GlobalWriter,
                target_source_id: None,
                knowledge_kinds: vec![KnowledgeKind::Rumor],
                entities: Vec::new(),
                topics: Vec::new(),
                query_text: Some(BoundedText::try_new("q", "q", 64).unwrap()),
                authorized_memory_owners: Vec::new(),
                reason: BoundedText::try_new("gap", "reason", 64).unwrap(),
                origin: RetrievalRequestOrigin::Planner,
                signal_priority: 4,
            }],
        },
        character_think_requests: vec![CharacterThinkRequest {
            character_id: CharacterId::from("c-1"),
            reason: BoundedText::try_new("present", "reason", 64).unwrap(),
        }],
    };
    assert_eq!(filled.retrieval_plan.requests.len(), 1);
    assert_eq!(filled.character_think_requests.len(), 1);
    assert!(matches!(CandidateRetrieverKind::Bm25, CandidateRetrieverKind::Bm25));
}
