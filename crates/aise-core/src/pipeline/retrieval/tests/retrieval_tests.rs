use super::*;
use crate::core::tests::{snake_pack, story_context, xu_xian_card};
use crate::core::{
    ActivationRule, BoundedText, EntryOrigin, FactBody, MemoryBody, MemoryEntry, MemoryId, RetrievalHint, RoleEntry,
    RoleId, RumorBody, StoryPack, TurnNumber,
};
use std::collections::BTreeMap;

fn pipeline() -> RetrievalPipeline {
    RetrievalPipeline::new()
}

fn fact_id(value: &str) -> FactId {
    FactId::try_new(value).expect("valid fact id")
}

fn rumor_id(value: &str) -> RumorId {
    RumorId::try_new(value).expect("valid rumor id")
}

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("valid role id")
}

fn memory_id(value: &str) -> MemoryId {
    MemoryId::try_new(value).expect("valid memory id")
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid bounded text")
}

fn seeded_context(story_pack: &StoryPack) -> StoryContext {
    let mut story_ctx = story_context();
    story_ctx.facts = story_pack
        .seed_facts
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                FactEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    story_ctx.rumors = story_pack
        .seed_rumors
        .iter()
        .map(|(id, body)| {
            (
                id.clone(),
                RumorEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    story_ctx.roles = story_pack
        .roles
        .iter()
        .map(|(id, body)| {
            let memories = body
                .seed_memories
                .iter()
                .map(|(mem_id, mem_body)| {
                    (
                        mem_id.clone(),
                        MemoryEntry {
                            id: mem_id.clone(),
                            origin: EntryOrigin::Seed,
                            body: mem_body.clone(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            (
                id.clone(),
                RoleEntry {
                    id: id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: body.spec.clone(),
                    profile: body.default_profile.clone(),
                    memories,
                },
            )
        })
        .collect();
    story_ctx
}

#[test]
fn fact_only_input_resolves_requested_fact() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("fact resolves");

    assert_eq!(output.facts.len(), 1);
    assert_eq!(output.facts[0].id, fact_ids[0]);
    assert!(matches!(output.facts[0].origin, EntryOrigin::Seed));
    assert_eq!(output.facts[0].body.content, story_pack.seed_facts[&fact_ids[0]].content);
    assert!(output.rumors.is_empty());
}

#[test]
fn rumor_only_input_resolves_requested_rumor() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let rumor_ids = [rumor_id("rumor_white_dress_woman")];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("rumor resolves");

    assert!(output.facts.is_empty());
    assert_eq!(output.rumors.len(), 1);
    assert_eq!(output.rumors[0].id, rumor_ids[0]);
    assert!(matches!(output.rumors[0].origin, EntryOrigin::Seed));
    assert_eq!(output.rumors[0].body.content, story_pack.seed_rumors[&rumor_ids[0]].content);
}

#[test]
fn mixed_input_resolves_both_knowledge_kinds() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let rumor_ids = [rumor_id("rumor_white_dress_woman")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("mixed input resolves");

    assert_eq!(output.facts.len(), 1);
    assert_eq!(output.rumors.len(), 1);
}

#[test]
fn empty_input_resolves_empty_output() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("empty input resolves");

    assert!(output.facts.is_empty());
    assert!(output.rumors.is_empty());
}

#[test]
fn retrieval_resolves_selected_entries_in_request_order() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [
        fact_id("west_lake_has_broken_bridge"),
        fact_id("bai_is_thousand_year_snake"),
    ];
    let rumor_ids = [
        rumor_id("rumor_bai_marriage_is_omen"),
        rumor_id("rumor_white_dress_woman"),
    ];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("ordered input resolves");

    assert_eq!(
        output.facts.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        fact_ids.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        output.rumors.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        rumor_ids.iter().collect::<Vec<_>>()
    );
}

#[test]
fn retrieval_rejects_missing_fact_id() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("missing_fact")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let error = pipeline().resolve(&story_ctx, &input).err().expect("missing fact fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_fact_missing",
            ..
        }
    ));
}

#[test]
fn retrieval_rejects_missing_rumor_id() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let rumor_ids = [rumor_id("missing_rumor")];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &rumor_ids,
    };

    let error = pipeline().resolve(&story_ctx, &input).err().expect("missing rumor fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_rumor_missing",
            ..
        }
    ));
}

#[test]
fn mixed_input_with_missing_id_returns_no_partial_output() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let rumor_ids = [rumor_id("missing_rumor")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let result = pipeline().resolve(&story_ctx, &input);

    assert!(result.is_err());
}

#[test]
fn runtime_role_keeps_identity_profile_and_memories_without_retrieval_transport() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("bai_suzhen");
    let seed_role = story_pack.roles[&selected_role_id].clone();
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx
        .roles
        .get_mut(&selected_role_id)
        .expect("role exists")
        .memories
        .insert(
            memory_id("dynamic_mem"),
            MemoryEntry {
                id: memory_id("dynamic_mem"),
                origin: EntryOrigin::Turn(TurnNumber::new(1)),
                body: MemoryBody {
                    content: text("Dynamic memory content."),
                    activation: ActivationRule::default(),
                    retrieval: RetrievalHint::try_new("dynamic hint").expect("valid hint"),
                },
            },
        );
    let role = &story_ctx.roles[&selected_role_id];

    assert_eq!(role.id, selected_role_id);
    assert_eq!(role.spec.name, seed_role.spec.name);
    assert_eq!(role.spec.alias, seed_role.spec.alias);
    assert_eq!(role.spec.backstory, seed_role.spec.backstory);
    assert_eq!(role.spec.retrieval.as_str(), seed_role.spec.retrieval.as_str());
    assert_eq!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(&seed_role.default_profile).expect("profile should serialize")
    );
    assert!(role.memories.contains_key(&memory_id("dynamic_mem")));
}

#[test]
fn frozen_card_role_replaces_only_the_effective_profile() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("xu_xian");
    let seed_role = story_pack.roles[&selected_role_id].clone();
    let mut card = xu_xian_card();
    card.name = text("Card-only name");
    let card_name = card.name.clone();
    let card_profile = card.profile.clone();
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.roles.get_mut(&selected_role_id).expect("role exists").profile = card_profile.clone();
    let role = &story_ctx.roles[&selected_role_id];

    assert_eq!(role.spec.name, seed_role.spec.name);
    assert_ne!(role.spec.name, card_name);
    assert_eq!(role.spec.alias, seed_role.spec.alias);
    assert_eq!(role.spec.backstory, seed_role.spec.backstory);
    assert_eq!(role.spec.retrieval.as_str(), seed_role.spec.retrieval.as_str());
    assert_eq!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(card_profile).expect("profile should serialize")
    );
    assert_ne!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(&seed_role.default_profile).expect("profile should serialize")
    );
}

#[test]
fn retrieval_input_serialization_contains_no_role_state() {
    let fact_ids = [fact_id("west_lake_has_broken_bridge")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let serialized = serde_json::to_string(&input).expect("input serializes");

    assert_eq!(serialized, r#"{"fact_ids":["west_lake_has_broken_bridge"],"rumor_ids":[]}"#);
    assert!(!serialized.contains("memories"));
}

#[test]
fn retrieval_output_serialization_contains_no_role_state() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("west_lake_has_broken_bridge")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("fact resolves");
    let serialized = serde_json::to_string(&output).expect("output serializes");

    assert!(!serialized.contains("roles"));
    assert!(!serialized.contains("memories"));
}

#[test]
fn missing_context_role_does_not_affect_knowledge_retrieval() {
    let story_pack = snake_pack();
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.roles.clear();
    let fact_ids = [fact_id("west_lake_has_broken_bridge")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("fact resolves");

    assert_eq!(output.facts[0].id, fact_ids[0]);
}

#[test]
fn retrieval_pipeline_constructs_without_store_or_character_cards() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
    };

    let output = RetrievalPipeline::new()
        .resolve(&story_ctx, &input)
        .expect("empty retrieval succeeds");

    assert!(output.facts.is_empty());
    assert!(output.rumors.is_empty());
}

#[test]
fn dynamically_added_and_updated_knowledge_in_context_is_retrieved() {
    let story_pack = snake_pack();
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.facts.insert(
        fact_id("dynamic_fact"),
        FactEntry {
            id: fact_id("dynamic_fact"),
            origin: EntryOrigin::Turn(TurnNumber::new(2)),
            body: FactBody {
                content: text("Dynamic fact."),
                activation: ActivationRule::default(),
                retrieval: RetrievalHint::try_new("dynamic fact hint").expect("valid hint"),
            },
        },
    );
    story_ctx.rumors.insert(
        rumor_id("dynamic_rumor"),
        RumorEntry {
            id: rumor_id("dynamic_rumor"),
            origin: EntryOrigin::Turn(TurnNumber::new(3)),
            body: RumorBody {
                content: text("Dynamic rumor."),
                activation: ActivationRule::default(),
                retrieval: RetrievalHint::try_new("dynamic rumor hint").expect("valid hint"),
            },
        },
    );

    let fact_ids = [fact_id("dynamic_fact")];
    let rumor_ids = [rumor_id("dynamic_rumor")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_ctx, &input).expect("dynamic knowledge resolves");

    assert!(matches!(
        &output.facts[0].origin,
        EntryOrigin::Turn(t) if t.value() == 2
    ));
    assert_eq!(output.facts[0].body.content.as_str(), "Dynamic fact.");
    assert!(matches!(
        &output.rumors[0].origin,
        EntryOrigin::Turn(t) if t.value() == 3
    ));
    assert_eq!(output.rumors[0].body.content.as_str(), "Dynamic rumor.");
}

#[test]
fn empty_output_serializes_without_private_role_state() {
    let serialized = serde_json::to_string(&RetrievalOutput::default()).expect("output serializes");

    assert_eq!(serialized, r#"{"facts":[],"rumors":[]}"#);
    assert!(!serialized.contains("memories"));
}
