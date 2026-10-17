use super::*;
use crate::core::{CharacterProfile, EntryOrigin, FactEntry, MemoryEntry, RoleActor, RoleBody, RoleEntry, RumorEntry};
use serde_json::json;
use std::collections::BTreeMap;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid bounded text")
}

fn hint(value: &str) -> RetrievalHint {
    RetrievalHint::try_new(value).expect("valid retrieval hint")
}

fn valid_rule() -> ActivationRule {
    serde_json::from_value(json!({
        "match": {
            "keys": ["bronze seal", "ancestral hall"]
        }
    }))
    .expect("valid activation rule")
}

fn fact_body(content: &str, retrieval: &str) -> FactBody {
    FactBody {
        content: text(content),
        retrieval: hint(retrieval),
        activation: valid_rule(),
    }
}

fn rumor_body(content: &str, retrieval: &str) -> RumorBody {
    RumorBody {
        content: text(content),
        retrieval: hint(retrieval),
        activation: valid_rule(),
    }
}

fn memory_body(content: &str, retrieval: &str) -> MemoryBody {
    MemoryBody {
        content: text(content),
        retrieval: hint(retrieval),
        activation: valid_rule(),
    }
}

fn seeded_context() -> StoryContext {
    let mut ctx = StoryContext::new();
    let fact_id = FactId::try_new("fact_seal_location").expect("valid fact id");
    ctx.facts.insert(
        fact_id.clone(),
        FactEntry {
            id: fact_id,
            origin: EntryOrigin::Seed,
            body: fact_body("The bronze seal is in the ancestral hall.", "Seal location"),
        },
    );
    for id in ["rumor_hall_guarded", "rumor_hall_untouched"] {
        let rumor_id = RumorId::try_new(id).expect("valid rumor id");
        ctx.rumors.insert(
            rumor_id.clone(),
            RumorEntry {
                id: rumor_id,
                origin: EntryOrigin::Seed,
                body: rumor_body("The ancestral hall is heavily guarded.", "Hall rumor"),
            },
        );
    }
    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");
    let mem_id = MemoryId::try_new("mem_seal_plan").expect("valid memory id");
    let mut memories = BTreeMap::new();
    memories.insert(
        mem_id.clone(),
        MemoryEntry {
            id: mem_id,
            origin: EntryOrigin::Seed,
            body: memory_body("I plan to inspect the ancestral hall seal.", "Seal plan"),
        },
    );
    ctx.roles.insert(
        role_id.clone(),
        RoleEntry {
            id: role_id,
            actor: RoleActor::Default,
            body: RoleBody {
                name: text("Bai Suzhen"),
                alias: vec![text("White Snake")],
                backstory: text("Immortal spirit."),
                retrieval: hint("Bai Suzhen protagonist"),
                default_profile: CharacterProfile {
                    appearance: None,
                    personality: None,
                    speaking_style: None,
                    dialogue_examples: Vec::new(),
                },
                seed_memories: BTreeMap::new(),
            },
            memories,
        },
    );
    ctx
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        other => panic!("expected invalid output, got {other}"),
    }
}

#[test]
fn projections_expose_expected_fields_without_role_memories_in_catalog() {
    let ctx = seeded_context();
    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");

    let facts = serde_json::to_value(project_facts(&ctx)).expect("facts serialize");
    let rumors = serde_json::to_value(project_rumors(&ctx)).expect("rumors serialize");
    let catalog = serde_json::to_value(project_role_catalog(&ctx)).expect("catalog serialize");
    let memories =
        serde_json::to_value(project_role_memories(&ctx, &role_id).expect("role memories project")).expect("serialize");

    assert_eq!(facts[0]["id"], "fact_seal_location");
    assert_eq!(rumors.as_array().expect("array").len(), 2);
    assert_eq!(catalog[0]["role_id"], "bai_suzhen");
    assert_eq!(catalog[0]["name"], "Bai Suzhen");
    assert_eq!(catalog[0]["aliases"], json!(["White Snake"]));
    assert_eq!(catalog[0]["retrieval_hint"], "Bai Suzhen protagonist");
    assert!(catalog[0].get("memories").is_none());
    assert_eq!(memories[0]["id"], "mem_seal_plan");
}

#[test]
fn project_role_memories_rejects_unknown_role() {
    let ctx = seeded_context();
    let unknown = RoleId::try_new("unknown").expect("valid role id");

    let err = match project_role_memories(&ctx, &unknown) {
        Ok(_) => panic!("unknown role should fail"),
        Err(error) => error,
    };

    assert_eq!(error_code(err), "extract_role_unknown");
}

#[test]
fn deterministic_id_allocation_increments_on_collision() {
    let mut ctx = seeded_context();
    let next_turn = TurnNumber::new(1);
    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");

    let first_fact = allocate_fact_id(&ctx, &next_turn, 1).expect("fact id");
    assert_eq!(first_fact.as_str(), "fact_t1_1");
    ctx.facts.insert(
        first_fact.clone(),
        FactEntry {
            id: first_fact,
            origin: EntryOrigin::Turn(next_turn.clone()),
            body: fact_body("Existing", "Hint"),
        },
    );
    let collided_fact = allocate_fact_id(&ctx, &next_turn, 1).expect("collided fact id");
    assert_eq!(collided_fact.as_str(), "fact_t1_2");

    let first_rumor = allocate_rumor_id(&ctx, &next_turn, 1).expect("rumor id");
    assert_eq!(first_rumor.as_str(), "rumor_t1_1");
    ctx.rumors.insert(
        first_rumor.clone(),
        RumorEntry {
            id: first_rumor,
            origin: EntryOrigin::Turn(next_turn.clone()),
            body: rumor_body("Existing", "Hint"),
        },
    );
    let collided_rumor = allocate_rumor_id(&ctx, &next_turn, 1).expect("collided rumor id");
    assert_eq!(collided_rumor.as_str(), "rumor_t1_2");

    let first_mem = allocate_memory_id(&ctx, &role_id, &next_turn, 1).expect("memory id");
    assert_eq!(first_mem.as_str(), "mem_t1_1");
    ctx.roles.get_mut(&role_id).expect("role exists").memories.insert(
        first_mem.clone(),
        MemoryEntry {
            id: first_mem,
            origin: EntryOrigin::Turn(next_turn.clone()),
            body: memory_body("Existing", "Hint"),
        },
    );
    let collided_mem = allocate_memory_id(&ctx, &role_id, &next_turn, 1).expect("collided memory id");
    assert_eq!(collided_mem.as_str(), "mem_t1_2");
}

#[test]
fn resolve_world_knowledge_validates_limits_and_ids() {
    let ctx = seeded_context();
    let config = ExtractConfig {
        max_fact_changes: 1,
        max_rumor_changes: 1,
        max_roles_for_memory: 1,
        ..ExtractConfig::default()
    };

    let err = match resolve_world_knowledge(
        &ctx,
        &config,
        vec![
            FactProposal::Add {
                body: fact_body("A", "H"),
            },
            FactProposal::Add {
                body: fact_body("B", "H"),
            },
        ],
        Vec::new(),
        Vec::new(),
    ) {
        Ok(_) => panic!("exceeded facts should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_fact_changes_exceeded");

    let err = match resolve_world_knowledge(
        &ctx,
        &config,
        Vec::new(),
        vec![
            RumorProposal::Add {
                body: rumor_body("A", "H"),
            },
            RumorProposal::Add {
                body: rumor_body("B", "H"),
            },
        ],
        Vec::new(),
    ) {
        Ok(_) => panic!("exceeded rumors should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_rumor_changes_exceeded");

    let err = match resolve_world_knowledge(
        &ctx,
        &config,
        Vec::new(),
        Vec::new(),
        vec![
            RoleId::try_new("bai_suzhen").expect("id"),
            RoleId::try_new("xu_xian").expect("id"),
        ],
    ) {
        Ok(_) => panic!("exceeded roles should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_roles_for_memory_exceeded");
}

#[test]
fn validate_activation_rule_rejects_empty_keys_when_enabled_and_not_constant() {
    let mut rule = ActivationRule::default();
    rule.mode.enabled = true;
    rule.mode.constant = false;
    rule.match_rule.keys.clear();

    let err = validate_activation_rule(&rule).expect_err("empty keys should fail");
    assert_eq!(error_code(err), "extract_activation_rule_invalid");

    rule.mode.constant = true;
    validate_activation_rule(&rule).expect("constant rule without keys is valid");
}
