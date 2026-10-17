use super::*;
use crate::core::{
    ActivationRule, BoundedText, CharacterProfile, EntryOrigin, FactChangeOp, FactEntry, MemoryEntry, RetrievalHint,
    RoleActor, RoleBody, RoleEntry, RumorChangeOp, RumorEntry,
};
use serde_json::json;
use std::collections::BTreeMap;

fn query(id: &str) -> SemanticQuery {
    SemanticQuery {
        condition_id: NarrativeConditionId::try_new(id).expect("valid condition id"),
        criterion: BoundedText::try_new(format!("criterion for {id}"), "criterion", 256).expect("valid criterion"),
    }
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        other => panic!("expected invalid output, got {other}"),
    }
}

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

fn seeded_context() -> StoryContext {
    let mut ctx = StoryContext::new();
    let fact_id = FactId::try_new("fact_seal_location").expect("valid fact id");
    ctx.facts.insert(
        fact_id.clone(),
        FactEntry {
            id: fact_id,
            origin: EntryOrigin::Seed,
            body: FactBody {
                content: text("The bronze seal is in the ancestral hall."),
                retrieval: hint("Location of the bronze seal"),
                activation: valid_rule(),
            },
        },
    );
    for id in ["rumor_hall_guarded", "rumor_hall_untouched"] {
        let rumor_id = RumorId::try_new(id).expect("valid rumor id");
        ctx.rumors.insert(
            rumor_id.clone(),
            RumorEntry {
                id: rumor_id,
                origin: EntryOrigin::Seed,
                body: RumorBody {
                    content: text("The ancestral hall is guarded."),
                    retrieval: hint("Hall guard rumor"),
                    activation: valid_rule(),
                },
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
            body: MemoryBody {
                content: text("I plan to inspect the ancestral hall seal."),
                retrieval: hint("Plan for the ancestral hall seal"),
                activation: valid_rule(),
            },
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
                retrieval: hint("Bai Suzhen"),
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

#[test]
fn valid_evaluations_return_exact_truth_map() {
    let queries = [query("a"), query("b"), query("c")];
    let raw = json!({
        "evaluations": [
            {"condition_id": "c", "truth": "unknown"},
            {"condition_id": "a", "truth": "satisfied"},
            {"condition_id": "b", "truth": "unsatisfied"}
        ]
    })
    .to_string();

    let evaluations = parse_narrative_evaluations(&raw, &queries).expect("valid evaluations");

    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("a").expect("valid id")],
        Truth::Satisfied
    );
    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("b").expect("valid id")],
        Truth::Unsatisfied
    );
    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("c").expect("valid id")],
        Truth::Unknown
    );
}

#[test]
fn malformed_json_uses_invalid_json_code() {
    let error = parse_narrative_evaluations("{", &[query("a")]).expect_err("malformed JSON should fail");

    assert_eq!(error_code(error), "extract_narrative_invalid_json");
}

#[test]
fn invalid_truth_uses_invalid_json_code() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"maybe"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("invalid truth should fail");

    assert_eq!(error_code(error), "extract_narrative_invalid_json");
}

#[test]
fn duplicate_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"satisfied"},{"condition_id":"a","truth":"unknown"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("duplicate condition should fail");

    assert_eq!(error_code(error), "extract_narrative_duplicate_condition");
}

#[test]
fn unknown_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"other","truth":"satisfied"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("unknown condition should fail");

    assert_eq!(error_code(error), "extract_narrative_unknown_condition");
}

#[test]
fn missing_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"satisfied"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a"), query("b")]).expect_err("missing condition should fail");

    assert_eq!(error_code(error), "extract_narrative_missing_condition");
}

#[test]
fn parse_world_knowledge_accepts_full_protocol_and_allocates_ids() {
    let ctx = seeded_context();
    let raw = json!({
        "facts": [
            {
                "op": "add",
                "body": {
                    "content": "The bronze seal was taken from the ancestral hall by Bai Suzhen.",
                    "retrieval": "Location and holder of the bronze seal",
                    "activation": {
                        "match": {
                            "keys": ["bronze seal", "ancestral hall"],
                            "secondary_keys": [],
                            "secondary_logic": "and_any",
                            "case_sensitive": false,
                            "match_whole_words": false,
                            "scan_depth": null
                        },
                        "mode": { "enabled": true, "constant": false },
                        "recursion": {
                            "exclude_recursion": false,
                            "prevent_recursion": false,
                            "delay_until_recursion": null
                        },
                        "selection": {
                            "order": 0,
                            "probability": 100,
                            "groups": ["hall.events"],
                            "group_override": false,
                            "group_weight": 100,
                            "use_group_scoring": false
                        }
                    }
                }
            },
            {
                "op": "update",
                "id": "fact_seal_location",
                "body": {
                    "content": "The bronze seal was originally kept in the ancestral hall, and has now been taken by Bai Suzhen.",
                    "retrieval": "History and current holder of the bronze seal",
                    "activation": {
                        "match": { "keys": ["bronze seal", "ancestral hall"] }
                    }
                }
            }
        ],
        "rumors": [
            {
                "op": "add",
                "body": {
                    "content": "Townspeople believe a thief broke into the ancestral hall at night.",
                    "retrieval": "Public rumor about the ancestral hall theft",
                    "activation": {
                        "match": { "keys": ["ancestral hall", "thief"] }
                    }
                }
            },
            {
                "op": "update",
                "id": "rumor_hall_guarded",
                "body": {
                    "content": "Townspeople now believe the ancestral hall guards failed to stop an intruder.",
                    "retrieval": "Public consensus on the ancestral hall guards",
                    "activation": {
                        "match": { "keys": ["ancestral hall", "guards"] }
                    }
                }
            },
            {
                "op": "remove",
                "id": "rumor_hall_untouched"
            }
        ],
        "roles_for_memory": ["bai_suzhen"]
    })
    .to_string();

    let extracted = parse_world_knowledge(&raw, &ctx, &ExtractConfig::default()).expect("world knowledge should parse");

    assert_eq!(extracted.facts.len(), 2);
    assert!(matches!(&extracted.facts[0], FactChangeOp::Add { id, .. } if id.as_str() == "fact_t1_1"));
    assert!(matches!(&extracted.facts[1], FactChangeOp::Update { id, .. } if id.as_str() == "fact_seal_location"));
    assert_eq!(extracted.rumors.len(), 3);
    assert!(matches!(&extracted.rumors[0], RumorChangeOp::Add { id, .. } if id.as_str() == "rumor_t1_1"));
    assert!(matches!(&extracted.rumors[1], RumorChangeOp::Update { id, .. } if id.as_str() == "rumor_hall_guarded"));
    assert!(matches!(&extracted.rumors[2], RumorChangeOp::Remove { id } if id.as_str() == "rumor_hall_untouched"));
    assert_eq!(
        extracted.roles_for_memory.iter().map(RoleId::as_str).collect::<Vec<_>>(),
        vec!["bai_suzhen"]
    );
}

#[test]
fn parse_world_knowledge_rejects_fact_remove_and_malformed_activation_types() {
    let ctx = seeded_context();
    let remove_fact = json!({
        "facts": [{ "op": "remove", "id": "fact_seal_location" }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&remove_fact, &ctx, &ExtractConfig::default()) {
        Ok(_) => panic!("fact remove must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_world_invalid_json");

    let invalid_probability = json!({
        "facts": [{
            "op": "add",
            "body": {
                "content": "Valid content.",
                "retrieval": "Valid hint",
                "activation": {
                    "match": { "keys": ["key"] },
                    "selection": { "probability": 101 }
                }
            }
        }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&invalid_probability, &ctx, &ExtractConfig::default()) {
        Ok(_) => panic!("invalid probability must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_world_invalid_json");

    let invalid_group = json!({
        "facts": [{
            "op": "add",
            "body": {
                "content": "Valid content.",
                "retrieval": "Valid hint",
                "activation": {
                    "match": { "keys": ["key"] },
                    "selection": { "groups": ["INVALID_GROUP"] }
                }
            }
        }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&invalid_group, &ctx, &ExtractConfig::default()) {
        Ok(_) => panic!("invalid group key must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_world_invalid_json");

    let empty_activation_key = json!({
        "facts": [{
            "op": "add",
            "body": {
                "content": "Valid content.",
                "retrieval": "Valid hint",
                "activation": {
                    "match": { "keys": ["   "] }
                }
            }
        }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&empty_activation_key, &ctx, &ExtractConfig::default()) {
        Ok(_) => panic!("empty activation key must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_world_invalid_json");
}

#[test]
fn parse_world_knowledge_covers_all_domain_error_codes() {
    let ctx = seeded_context();
    let config = ExtractConfig::default();

    let unknown_fact = json!({
        "facts": [{
            "op": "update",
            "id": "fact_missing",
            "body": {
                "content": "Updated.",
                "retrieval": "Hint",
                "activation": { "match": { "keys": ["seal"] } }
            }
        }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&unknown_fact, &ctx, &config) {
        Ok(_) => panic!("unknown fact should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_fact_unknown_id");

    let duplicate_fact = json!({
        "facts": [
            {
                "op": "update",
                "id": "fact_seal_location",
                "body": {
                    "content": "Updated 1.",
                    "retrieval": "Hint",
                    "activation": { "match": { "keys": ["seal"] } }
                }
            },
            {
                "op": "update",
                "id": "fact_seal_location",
                "body": {
                    "content": "Updated 2.",
                    "retrieval": "Hint",
                    "activation": { "match": { "keys": ["seal"] } }
                }
            }
        ],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&duplicate_fact, &ctx, &config) {
        Ok(_) => panic!("duplicate fact should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_fact_duplicate_id");

    let unknown_rumor = json!({
        "facts": [],
        "rumors": [{ "op": "remove", "id": "rumor_missing" }],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&unknown_rumor, &ctx, &config) {
        Ok(_) => panic!("unknown rumor should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_rumor_unknown_id");

    let duplicate_rumor = json!({
        "facts": [],
        "rumors": [
            {
                "op": "update",
                "id": "rumor_hall_guarded",
                "body": {
                    "content": "Updated.",
                    "retrieval": "Hint",
                    "activation": { "match": { "keys": ["hall"] } }
                }
            },
            { "op": "remove", "id": "rumor_hall_guarded" }
        ],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&duplicate_rumor, &ctx, &config) {
        Ok(_) => panic!("duplicate rumor should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_rumor_duplicate_id");

    let unknown_role = json!({
        "facts": [],
        "rumors": [],
        "roles_for_memory": ["unknown_role"]
    })
    .to_string();
    let err = match parse_world_knowledge(&unknown_role, &ctx, &config) {
        Ok(_) => panic!("unknown role should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_role_unknown");

    let duplicate_role = json!({
        "facts": [],
        "rumors": [],
        "roles_for_memory": ["bai_suzhen", "bai_suzhen"]
    })
    .to_string();
    let err = match parse_world_knowledge(&duplicate_role, &ctx, &config) {
        Ok(_) => panic!("duplicate role should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_role_duplicate");

    let invalid_activation = json!({
        "facts": [{
            "op": "add",
            "body": {
                "content": "New fact without keys.",
                "retrieval": "Hint",
                "activation": {
                    "match": { "keys": [] },
                    "mode": { "enabled": true, "constant": false }
                }
            }
        }],
        "rumors": [],
        "roles_for_memory": []
    })
    .to_string();
    let err = match parse_world_knowledge(&invalid_activation, &ctx, &config) {
        Ok(_) => panic!("invalid activation should fail"),
        Err(error) => error,
    };
    assert_eq!(error_code(err), "extract_activation_rule_invalid");
}

#[test]
fn parse_role_memory_accepts_add_and_update_and_rejects_all_error_cases() {
    let ctx = seeded_context();
    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");
    let config = ExtractConfig::default();

    let valid_raw = json!({
        "memories": [
            {
                "op": "add",
                "body": {
                    "content": "I secretly took the bronze seal from the ancestral hall before dawn.",
                    "retrieval": "Taking the bronze seal from the ancestral hall",
                    "activation": {
                        "match": { "keys": ["bronze seal", "ancestral hall"] }
                    }
                }
            },
            {
                "op": "update",
                "id": "mem_seal_plan",
                "body": {
                    "content": "I had planned to inspect the ancestral hall seal, and I have now taken it into my possession.",
                    "retrieval": "Plan and acquisition of the ancestral hall seal",
                    "activation": {
                        "match": { "keys": ["bronze seal", "ancestral hall"] }
                    }
                }
            }
        ]
    })
    .to_string();
    let changes = parse_role_memory(&valid_raw, &ctx, &role_id, &config).expect("valid role memory");
    assert_eq!(changes.len(), 2);
    assert!(matches!(
        &changes[0],
        RoleMemoryChangeOp::Add { role_id: r, id, .. } if r.as_str() == "bai_suzhen" && id.as_str() == "mem_t1_1"
    ));
    assert!(matches!(
        &changes[1],
        RoleMemoryChangeOp::Update { role_id: r, id, .. } if r.as_str() == "bai_suzhen" && id.as_str() == "mem_seal_plan"
    ));

    let remove_memory = json!({
        "memories": [{ "op": "remove", "id": "mem_seal_plan" }]
    })
    .to_string();
    let err = parse_role_memory(&remove_memory, &ctx, &role_id, &config).expect_err("memory remove must fail");
    assert_eq!(error_code(err), "extract_memory_invalid_json");

    let mut small_config = config.clone();
    small_config.max_memory_changes_per_role = 1;
    let err = parse_role_memory(&valid_raw, &ctx, &role_id, &small_config).expect_err("exceeded memory limit");
    assert_eq!(error_code(err), "extract_memory_changes_exceeded");

    let unknown_mem = json!({
        "memories": [{
            "op": "update",
            "id": "mem_missing",
            "body": {
                "content": "Updated.",
                "retrieval": "Hint",
                "activation": { "match": { "keys": ["seal"] } }
            }
        }]
    })
    .to_string();
    let err = parse_role_memory(&unknown_mem, &ctx, &role_id, &config).expect_err("unknown memory id");
    assert_eq!(error_code(err), "extract_memory_unknown_id");

    let duplicate_mem = json!({
        "memories": [
            {
                "op": "update",
                "id": "mem_seal_plan",
                "body": {
                    "content": "Updated 1.",
                    "retrieval": "Hint",
                    "activation": { "match": { "keys": ["seal"] } }
                }
            },
            {
                "op": "update",
                "id": "mem_seal_plan",
                "body": {
                    "content": "Updated 2.",
                    "retrieval": "Hint",
                    "activation": { "match": { "keys": ["seal"] } }
                }
            }
        ]
    })
    .to_string();
    let err = parse_role_memory(&duplicate_mem, &ctx, &role_id, &config).expect_err("duplicate memory id");
    assert_eq!(error_code(err), "extract_memory_duplicate_id");

    let invalid_activation = json!({
        "memories": [{
            "op": "add",
            "body": {
                "content": "New memory.",
                "retrieval": "Hint",
                "activation": { "match": { "keys": [] } }
            }
        }]
    })
    .to_string();
    let err = parse_role_memory(&invalid_activation, &ctx, &role_id, &config).expect_err("invalid activation");
    assert_eq!(error_code(err), "extract_activation_rule_invalid");
}
