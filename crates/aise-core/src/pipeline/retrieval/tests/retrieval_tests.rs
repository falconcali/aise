use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{
    ActivationRule, BoundedText, CharacterCard, CharacterCardRef, EntryOrigin, FactBody, MemoryBody, MemoryEntry,
    MemoryId, RetrievalHint, RoleEntry, RumorBody, StoryAssetSpec, StoryPack, TurnNumber,
};
use std::collections::BTreeMap;

fn pipeline() -> RetrievalPipeline {
    RetrievalPipeline::new(Arc::new(Store::default()))
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
    let mut story_ctx = StoryContext::new();
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
                    actor: RoleActor::Default,
                    body: body.clone(),
                    memories,
                },
            )
        })
        .collect();
    story_ctx
}

async fn pipeline_with_cards(
    story_ctx: &StoryContext,
    story_pack: StoryPack,
    cards: Vec<(CharacterCardRef, CharacterCard)>,
) -> RetrievalPipeline {
    let store = Arc::new(Store::default());
    if !cards.is_empty() {
        store
            .pack_store()
            .retain(
                &story_ctx.story_id,
                StoryAssetSpec {
                    pack_ref: story_ctx.pack_ref.clone(),
                    pack: story_pack,
                    cards,
                },
            )
            .await
            .expect("assets should be retained");
    }
    RetrievalPipeline::new(store)
}

#[test]
fn fact_only_input_resolves_requested_fact() {
    let story_pack = snake_pack();
    let story_ctx = seeded_context(&story_pack);
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
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
        role_ids: &[],
    };

    let result = pipeline().resolve(&story_ctx, &input);

    assert!(result.is_err());
}

#[tokio::test]
async fn default_role_uses_role_body_identity_profile_and_memories_without_loading_pack() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("bai_suzhen");
    let role_body = story_pack.roles[&selected_role_id].clone();
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
    let pipeline = pipeline();
    let role_ids = [selected_role_id.clone()];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let output = pipeline.retrieve(&input, &story_ctx).await.expect("default role resolves");
    let role = output
        .roles
        .iter()
        .find(|role| role.role_id == selected_role_id)
        .expect("resolved role should be indexed");

    assert_eq!(role.role_id, selected_role_id);
    assert_eq!(role.name, role_body.name);
    assert_eq!(role.aliases, role_body.alias);
    assert_eq!(role.backstory, role_body.backstory);
    assert_eq!(role.retrieval_hint.as_str(), role_body.retrieval.as_str());
    assert_eq!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(&role_body.default_profile).expect("profile should serialize")
    );
    assert_eq!(
        serde_json::to_value(&role.memories).expect("memories should serialize"),
        serde_json::to_value(&story_ctx.roles[&selected_role_id].memories).expect("memories should serialize")
    );
}

#[tokio::test]
async fn card_role_replaces_only_the_effective_profile() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("xu_xian");
    let role_body = story_pack.roles[&selected_role_id].clone();
    let mut card = xu_xian_card();
    card.name = text("Card-only name");
    let card_name = card.name.clone();
    let card_profile = card.profile.clone();
    let card_ref = card.card_ref().expect("card ref should resolve");
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.roles.get_mut(&selected_role_id).expect("role exists").actor = RoleActor::Card(card_ref.clone());
    let pipeline = pipeline_with_cards(&story_ctx, story_pack, vec![(card_ref, card)]).await;
    let role_ids = [selected_role_id.clone()];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let output = pipeline.retrieve(&input, &story_ctx).await.expect("card role resolves");
    let role = output
        .roles
        .iter()
        .find(|role| role.role_id == selected_role_id)
        .expect("resolved role should be indexed");

    assert_eq!(role.name, role_body.name);
    assert_ne!(role.name, card_name);
    assert_eq!(role.aliases, role_body.alias);
    assert_eq!(role.backstory, role_body.backstory);
    assert_eq!(role.retrieval_hint.as_str(), role_body.retrieval.as_str());
    assert_eq!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(card_profile).expect("profile should serialize")
    );
    assert_ne!(
        serde_json::to_value(&role.profile).expect("profile should serialize"),
        serde_json::to_value(&role_body.default_profile).expect("profile should serialize")
    );
    assert_eq!(
        serde_json::to_value(&role.memories).expect("memories should serialize"),
        serde_json::to_value(&story_ctx.roles[&selected_role_id].memories).expect("memories should serialize")
    );
}

#[tokio::test]
async fn role_resolution_preserves_request_order() {
    let story_pack = snake_pack();
    let role_ids = [role_id("xiaoqing"), role_id("bai_suzhen"), role_id("xu_xian")];
    let story_ctx = seeded_context(&story_pack);
    let pipeline = pipeline();
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let output = pipeline.retrieve(&input, &story_ctx).await.expect("roles resolve");

    assert_eq!(
        output.roles.iter().map(|role| &role.role_id).collect::<Vec<_>>(),
        role_ids.iter().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn duplicate_role_input_fails_before_card_lookup() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("xu_xian");
    let card_ref = xu_xian_card().card_ref().expect("card ref should resolve");
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.roles.get_mut(&selected_role_id).expect("role exists").actor = RoleActor::Card(card_ref);
    let pipeline = pipeline();
    let role_ids = [selected_role_id.clone(), selected_role_id];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let error = pipeline
        .retrieve(&input, &story_ctx)
        .await
        .err()
        .expect("duplicate roles should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_duplicate_role_id",
            ..
        }
    ));
}

#[tokio::test]
async fn missing_context_role_fails_the_retrieval() {
    let story_pack = snake_pack();
    let missing_role_id = role_id("missing_role");
    let story_ctx = seeded_context(&story_pack);
    let pipeline = pipeline();
    let role_ids = [missing_role_id];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let error = pipeline
        .retrieve(&input, &story_ctx)
        .await
        .err()
        .expect("missing role should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_role_missing",
            ..
        }
    ));
}

#[tokio::test]
async fn missing_character_card_fails_the_retrieval() {
    let story_pack = snake_pack();
    let selected_role_id = role_id("xu_xian");
    let card_ref = xu_xian_card().card_ref().expect("card ref should resolve");
    let mut story_ctx = seeded_context(&story_pack);
    story_ctx.roles.get_mut(&selected_role_id).expect("role exists").actor = RoleActor::Card(card_ref);
    let pipeline = pipeline();
    let role_ids = [selected_role_id];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
        role_ids: &role_ids,
    };

    let error = pipeline
        .retrieve(&input, &story_ctx)
        .await
        .err()
        .expect("missing card should fail");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_role_card_missing",
            ..
        }
    ));
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
        role_ids: &[],
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
fn non_not_found_card_error_remains_a_retrieval_persistence_error() {
    let error = map_card_error(
        &role_id("xu_xian"),
        PersistenceError::Unavailable {
            message: "offline".to_owned(),
        },
    );

    assert!(matches!(
        error,
        PipelineError::Persistence {
            stage: PipelineStage::Retrieval,
            source: PersistenceError::Unavailable { .. },
        }
    ));
}
