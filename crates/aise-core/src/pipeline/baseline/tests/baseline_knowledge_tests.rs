use super::*;
use crate::core::tests::{demo_pack, snake_pack};
use crate::core::{
    ActivationRule, BoundedText, IdempotencyKey, NarrativeDirection, NarrativeNodeId, StorySummary, Turn,
    TurnEvaluation, TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use serde_json::{Value, json};

fn test_trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "baseline-knowledge-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn rule(value: Value) -> ActivationRule {
    serde_json::from_value(value).expect("test rule should deserialize")
}

fn fact_id(value: &str) -> FactId {
    FactId::try_new(value).expect("test fact id should be valid")
}

fn rumor_id(value: &str) -> RumorId {
    RumorId::try_new(value).expect("test rumor id should be valid")
}

fn contribution(text: &str) -> PlayerContribution {
    PlayerContribution {
        raw: text.to_owned(),
        processed: text.to_owned(),
    }
}

fn empty_narrative() -> NarrativeProjection {
    NarrativeProjection {
        directions: Vec::new(),
        constraints: Vec::new(),
        queries: Vec::new(),
    }
}

fn turn(number: u64, segment: &str) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("turn-{number}")).expect("test key should be valid"),
        player_contribution: contribution("继续"),
        turn_segment: TurnSegment::new(segment.to_owned()),
        world_change: WorldChange {},
        turn_evaluation: TurnEvaluation {},
        turn_status: TurnStatus::Accepted,
    }
}

fn pack_with_rules() -> StoryPack {
    let mut pack = snake_pack();
    for body in pack.seed_facts.values_mut() {
        body.activation = ActivationRule::default();
    }
    for body in pack.seed_rumors.values_mut() {
        body.activation = ActivationRule::default();
    }
    let fact = pack
        .seed_facts
        .get_mut(&fact_id("bai_is_thousand_year_snake"))
        .expect("snake pack should define bai_is_thousand_year_snake");
    fact.activation = rule(json!({ "match": { "keys": ["白蛇", "蛇妖"] } }));
    let fact = pack
        .seed_facts
        .get_mut(&fact_id("fahai_knows_identity"))
        .expect("snake pack should define fahai_knows_identity");
    fact.activation = rule(json!({ "match": { "keys": ["法海"] } }));
    let rumor = pack
        .seed_rumors
        .get_mut(&rumor_id("rumor_fahai_never_misses"))
        .expect("snake pack should define rumor_fahai_never_misses");
    rumor.activation = rule(json!({ "match": { "keys": ["法海"] } }));
    pack
}

fn activate_with(
    pack: &StoryPack,
    story_ctx: &StoryContext,
    player_text: &str,
    narrative: &NarrativeProjection,
) -> ActivatedKnowledge {
    let trace = test_trace();
    let player_contribution = contribution(player_text);
    activate_knowledge(
        KnowledgeScope {
            story_pack: pack,
            story_ctx,
            player_contribution: &player_contribution,
            narrative,
        },
        &ActivationConfig::default(),
        trace.root(),
    )
}

fn fact_ids(knowledge: &ActivatedKnowledge) -> Vec<&str> {
    knowledge.facts.iter().map(|fact| fact.id.as_str()).collect()
}

#[test]
fn player_contribution_activates_matching_facts_and_rumors() {
    let knowledge = activate_with(&pack_with_rules(), &StoryContext::new(), "法海在寺门前等我", &empty_narrative());

    assert_eq!(fact_ids(&knowledge), ["fahai_knows_identity"]);
    assert_eq!(knowledge.rumors.len(), 1);
    assert_eq!(knowledge.rumors[0].id.as_str(), "rumor_fahai_never_misses");
    assert!(matches!(knowledge.facts[0].origin, EntryOrigin::Seed));
}

#[test]
fn example_pack_rules_activate_and_recurse() {
    let knowledge = activate_with(&snake_pack(), &StoryContext::new(), "法海说白娘子是蛇妖", &empty_narrative());

    let facts = fact_ids(&knowledge);
    assert_eq!(facts[..2], ["bai_is_thousand_year_snake", "fahai_knows_identity"]);
    assert!(facts.contains(&"fahai_fears_calamity"));
}

#[test]
fn example_constant_entry_is_always_active() {
    let knowledge = activate_with(&demo_pack(), &StoryContext::new(), "我坐在火炉边", &empty_narrative());

    assert!(fact_ids(&knowledge).contains(&"lodge_is_remote"));
}

#[test]
fn unmatched_story_activates_nothing() {
    let knowledge = activate_with(&pack_with_rules(), &StoryContext::new(), "今天天气不错", &empty_narrative());

    assert!(knowledge.facts.is_empty());
    assert!(knowledge.rumors.is_empty());
}

#[test]
fn opening_is_scanned_when_summary_is_absent() {
    let mut pack = pack_with_rules();
    pack.opening = BoundedText::try_new("法海在山门前等候。", "test", usize::MAX).expect("test opening should fit");

    let knowledge = activate_with(&pack, &StoryContext::new(), "继续", &empty_narrative());

    assert_eq!(fact_ids(&knowledge), ["fahai_knows_identity"]);
    assert_eq!(knowledge.rumors.len(), 1);
}

#[test]
fn summary_replaces_opening_in_scan_buffer() {
    let mut pack = pack_with_rules();
    pack.opening = BoundedText::try_new("法海在山门前等候。", "test", usize::MAX).expect("test opening should fit");
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(StorySummary {
        text: "雨后的西湖十分平静。".to_owned(),
        covered_through: TurnNumber::new(1),
    });

    let knowledge = activate_with(&pack, &story_ctx, "继续", &empty_narrative());

    assert!(knowledge.facts.is_empty());
    assert!(knowledge.rumors.is_empty());
}

#[test]
fn recent_story_and_narrative_directions_are_scanned() {
    let mut story_ctx = StoryContext::new();
    story_ctx.recent_turns.push_back(turn(1, "西湖边有一条白蛇游过。"));
    let narrative = NarrativeProjection {
        directions: vec![NarrativeDirection {
            node_id: NarrativeNodeId::try_new("pursuit").expect("test node id should be valid"),
            dramatic_focus: BoundedText::try_new("法海开始追查", "test", usize::MAX).expect("test text should fit"),
        }],
        ..empty_narrative()
    };

    let knowledge = activate_with(&pack_with_rules(), &story_ctx, "继续", &narrative);

    assert_eq!(fact_ids(&knowledge), ["fahai_knows_identity", "bai_is_thousand_year_snake"]);
}

#[test]
fn summary_is_only_visible_beyond_recent_story_depth() {
    let summary = StorySummary {
        text: "此前，白蛇来到钱塘。".to_owned(),
        covered_through: TurnNumber::new(3),
    };
    let mut shallow = StoryContext::new();
    shallow.summary = Some(summary.clone());
    let mut deep = shallow.clone();
    deep.recent_turns.push_back(turn(4, "雨停了。"));
    deep.recent_turns.push_back(turn(5, "许仙回到药铺。"));

    let near = activate_with(&pack_with_rules(), &shallow, "继续", &empty_narrative());
    let far = activate_with(&pack_with_rules(), &deep, "继续", &empty_narrative());

    assert_eq!(fact_ids(&near), ["bai_is_thousand_year_snake"]);
    assert!(far.facts.is_empty());
}
