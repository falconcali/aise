use crate::core::{
    CharacterCard, EntryOrigin, FactEntry, MemoryEntry, NarrativeGraph, RoleEntry, RoleId, RumorEntry, StoryContext,
    StoryId, StoryLifeCycle, StoryPack, StoryPackSnapshot, TurnCheckEvaluation, TurnEvaluation, TurnNumber,
};
use crate::persistence::Store;
use std::collections::VecDeque;
use std::sync::Arc;

pub(crate) const SNAKE_PACK_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack_v1.json"));
const DEMO_PACK_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/demo_pack_v1.json"));
const XU_XIAN_CARD_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/xu_xian_card_v1.json"));

pub(crate) fn snake_pack() -> StoryPack {
    serde_json::from_str(SNAKE_PACK_JSON).expect("snake pack example should import")
}

pub(crate) fn demo_pack() -> StoryPack {
    serde_json::from_str(DEMO_PACK_JSON).expect("demo pack example should import")
}

pub(crate) fn xu_xian_card() -> CharacterCard {
    serde_json::from_str(XU_XIAN_CARD_JSON).expect("character card example should import")
}

pub(crate) fn turn_evaluation() -> TurnEvaluation {
    TurnEvaluation {
        proposal_version: 1,
        repair_rounds: 0,
        checks: vec![TurnCheckEvaluation {
            name: "test_check".to_owned(),
            passed: true,
            issue: None,
        }],
        scores: Vec::new(),
        choice: None,
    }
}

pub(crate) fn story_context() -> StoryContext {
    let pack = snake_pack();
    let pack_ref = pack.pack_ref().expect("default pack ref should compute");
    let roles = pack
        .roles
        .iter()
        .map(|(role_id, seed_role)| {
            let memories = seed_role
                .seed_memories
                .iter()
                .map(|(memory_id, body)| {
                    (
                        memory_id.clone(),
                        MemoryEntry {
                            id: memory_id.clone(),
                            origin: EntryOrigin::Seed,
                            body: body.clone(),
                        },
                    )
                })
                .collect();
            (
                role_id.clone(),
                RoleEntry {
                    id: role_id.clone(),
                    origin: EntryOrigin::Seed,
                    spec: seed_role.spec.clone(),
                    profile: seed_role.default_profile.clone(),
                    memories,
                },
            )
        })
        .collect();
    let facts = pack
        .seed_facts
        .iter()
        .map(|(fact_id, body)| {
            (
                fact_id.clone(),
                FactEntry {
                    id: fact_id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    let rumors = pack
        .seed_rumors
        .iter()
        .map(|(rumor_id, body)| {
            (
                rumor_id.clone(),
                RumorEntry {
                    id: rumor_id.clone(),
                    origin: EntryOrigin::Seed,
                    body: body.clone(),
                },
            )
        })
        .collect();
    StoryContext {
        story_id: StoryId::try_new("test-story").expect("test story id should be valid"),
        pack_ref,
        opening: Arc::new(pack.opening.clone()),
        roles,
        facts,
        rumors,
        player: RoleId::try_new("xu_xian").expect("test player role should be valid"),
        turn_number: TurnNumber::new(0),
        summary: None,
        recent_turns: VecDeque::new(),
        life_cycle: StoryLifeCycle::Active,
        narrative: pack.narrative.as_ref().map(NarrativeGraph::initial_state).unwrap_or_default(),
    }
}

pub(crate) async fn store_with_default_pack() -> Store {
    let store = Store::default();
    let story_ctx = story_context();
    store
        .pack_store()
        .retain(
            &story_ctx.story_id,
            StoryPackSnapshot::from_pack(snake_pack()).expect("default pack snapshot should build"),
        )
        .await
        .expect("default pack should be retained");
    store
}
