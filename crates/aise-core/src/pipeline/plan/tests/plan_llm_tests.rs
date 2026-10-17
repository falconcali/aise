use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    ActivatedKnowledge, EntryOrigin, FactEntry, MemoryEntry, PlayerRoleContext, RoleActor, RoleEntry, RoleId,
    RumorEntry, StoryContext, StoryPack,
};
use crate::pipeline::plan::plan_candidates::{PlanCandidates, collect_candidates};
use crate::pipeline::plan::plan_roles::collect_role_catalog;
use serde_json::json;
use std::collections::BTreeMap;

fn candidate_pack() -> StoryPack {
    let mut pack = snake_pack();
    let fact_body = pack.seed_facts.values().next().expect("fixture fact").clone();
    let rumor_body = pack.seed_rumors.values().next().expect("fixture rumor").clone();
    pack.seed_facts = ["fact-a", "fact-b"]
        .into_iter()
        .map(|id| (FactId::try_new(id).expect("valid fact id"), fact_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack.seed_rumors = ["rumor-a", "rumor-b"]
        .into_iter()
        .map(|id| (RumorId::try_new(id).expect("valid rumor id"), rumor_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack
}

fn candidate_context(pack: &StoryPack) -> StoryContext {
    let mut ctx = StoryContext::new();
    ctx.facts = pack
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
    ctx.rumors = pack
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
    ctx.roles = pack
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
    ctx
}

fn candidates(ctx: &StoryContext) -> PlanCandidates<'_> {
    collect_candidates(ctx, &ActivatedKnowledge::default(), 8)
}

fn config(max_story_goal_bytes: usize) -> PlanConfig {
    PlanConfig {
        max_retrieval_candidates_per_kind: 8,
        max_story_goal_bytes,
        max_player_role_context_bytes: 16_384,
        max_roles_for_generation: 16,
        max_character_think_requests: 8,
        max_thinking_focus_bytes: 512,
    }
}

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("valid role id")
}

fn player_role(pack: &StoryPack) -> PlayerRoleContext {
    let role_id = role_id("xu_xian");
    let role = &pack.roles[&role_id];
    PlayerRoleContext {
        role_id,
        name: role.name.clone(),
        aliases: role.alias.clone(),
        backstory: role.backstory.clone(),
        profile: role.default_profile.clone(),
    }
}

fn parse(raw: &str, pack: &StoryPack, config: &PlanConfig) -> Result<PlanOutput, PipelineError> {
    let raw = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.entry("roles_for_generation").or_insert_with(|| json!([]));
                object.entry("character_think_requests").or_insert_with(|| json!([]));
            }
            value.to_string()
        }
        Err(_) => raw.to_owned(),
    };
    let ctx = candidate_context(pack);
    let candidates = candidates(&ctx);
    let player_role = player_role(pack);
    let role_catalog = collect_role_catalog(&ctx, &player_role.role_id);
    parse_plan_output(&raw, &candidates, &role_catalog, player_role, config)
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        other => panic!("expected invalid output, got {other}"),
    }
}

#[test]
fn valid_json_produces_typed_plan_output() {
    let pack = candidate_pack();
    let raw = json!({
        "story_goal": "Continue the encounter.",
        "facts_to_retrieve": ["fact-a"],
        "rumors_to_retrieve": ["rumor-b"]
    })
    .to_string();

    let output = parse(&raw, &pack, &config(512)).expect("valid output");

    assert_eq!(output.story_goal.as_str(), "Continue the encounter.");
    assert_eq!(output.facts_to_retrieve[0].as_str(), "fact-a");
    assert_eq!(output.rumors_to_retrieve[0].as_str(), "rumor-b");
}

#[test]
fn output_rejects_plain_text_story_goal() {
    let pack = candidate_pack();
    let raw = "Continue the encounter.";

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn output_rejects_json_with_surrounding_prose() {
    let pack = candidate_pack();
    let raw = r#"Here is the plan: {"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn missing_field_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn output_rejects_unknown_fields() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[],"reason":"because"}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn output_rejects_empty_story_goal() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":" \n ","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_story_goal_empty");
}

#[test]
fn output_rejects_oversized_story_goal() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"界界","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(5)).expect_err("invalid output");

    assert!(matches!(
        error,
        PipelineError::InvalidOutput {
            code: "plan_story_goal_too_long",
            message,
            ..
        } if message == "story goal size 6 exceeds maximum 5"
    ));
}

#[test]
fn invalid_id_is_rejected_as_invalid_json() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[""],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn output_rejects_unknown_fact_id() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-x"],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_unknown_fact_id");
}

#[test]
fn output_rejects_unknown_rumor_id() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":["rumor-x"]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_unknown_rumor_id");
}

#[test]
fn cross_kind_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["rumor-a"],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_unknown_fact_id");
}

#[test]
fn output_rejects_duplicate_fact_id() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-a","fact-a"],"rumors_to_retrieve":[]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_duplicate_fact_id");
}

#[test]
fn output_rejects_duplicate_rumor_id() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":["rumor-a","rumor-a"]}"#;

    let error = parse(raw, &pack, &config(512)).expect_err("invalid output");

    assert_eq!(error_code(error), "plan_duplicate_rumor_id");
}

#[test]
fn empty_selections_are_valid() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let output = parse(raw, &pack, &config(512)).expect("valid output");

    assert!(output.facts_to_retrieve.is_empty());
    assert!(output.rumors_to_retrieve.is_empty());
}

#[test]
fn valid_output_preserves_selected_id_order() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-b","fact-a"],"rumors_to_retrieve":["rumor-b","rumor-a"]}"#;

    let output = parse(raw, &pack, &config(512)).expect("valid output");

    assert_eq!(
        output.facts_to_retrieve.iter().map(FactId::as_str).collect::<Vec<_>>(),
        ["fact-b", "fact-a"]
    );
    assert_eq!(
        output.rumors_to_retrieve.iter().map(RumorId::as_str).collect::<Vec<_>>(),
        ["rumor-b", "rumor-a"]
    );
}

fn role_selection_output(roles: serde_json::Value, requests: serde_json::Value) -> String {
    json!({
        "story_goal": "Continue.",
        "facts_to_retrieve": [],
        "rumors_to_retrieve": [],
        "roles_for_generation": roles,
        "character_think_requests": requests
    })
    .to_string()
}

#[test]
fn missing_role_selection_field_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{
        "story_goal":"Continue.",
        "facts_to_retrieve":[],
        "rumors_to_retrieve":[],
        "roles_for_generation":[]
    }"#;
    let ctx = candidate_context(&pack);
    let candidates = candidates(&ctx);
    let player_role = player_role(&pack);
    let role_catalog = collect_role_catalog(&ctx, &player_role.role_id);

    let error = parse_plan_output(raw, &candidates, &role_catalog, player_role, &config(512))
        .expect_err("missing field should fail");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn output_rejects_too_many_generation_roles() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!(["bai_suzhen", "xu_xian"]), json!([]));
    let mut config = config(512);
    config.max_roles_for_generation = 1;

    let error = parse(&raw, &pack, &config).expect_err("too many roles should fail");

    assert_eq!(error_code(error), "plan_too_many_generation_roles");
}

#[test]
fn output_rejects_duplicate_generation_role_id() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!(["bai_suzhen", "bai_suzhen"]), json!([]));

    let error = parse(&raw, &pack, &config(512)).expect_err("duplicate role should fail");

    assert_eq!(error_code(error), "plan_duplicate_generation_role_id");
}

#[test]
fn output_rejects_unknown_generation_role_id() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!(["ghost"]), json!([]));

    let error = parse(&raw, &pack, &config(512)).expect_err("unknown role should fail");

    assert_eq!(error_code(error), "plan_unknown_generation_role_id");
}

#[test]
fn output_rejects_too_many_character_think_requests() {
    let pack = candidate_pack();
    let raw = role_selection_output(
        json!([]),
        json!([{"role_id":"bai_suzhen","thinking_focus":"Choose a response."}]),
    );
    let mut config = config(512);
    config.max_character_think_requests = 0;

    let error = parse(&raw, &pack, &config).expect_err("too many think requests should fail");

    assert_eq!(error_code(error), "plan_too_many_character_think_requests");
}

#[test]
fn output_rejects_duplicate_character_think_role_id() {
    let pack = candidate_pack();
    let raw = role_selection_output(
        json!([]),
        json!([
            {"role_id":"bai_suzhen","thinking_focus":"Choose a response."},
            {"role_id":"bai_suzhen","thinking_focus":"Assess the danger."}
        ]),
    );

    let error = parse(&raw, &pack, &config(512)).expect_err("duplicate think role should fail");

    assert_eq!(error_code(error), "plan_duplicate_character_think_role_id");
}

#[test]
fn output_rejects_unknown_character_think_role_id() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!([]), json!([{"role_id":"ghost","thinking_focus":"Choose a response."}]));

    let error = parse(&raw, &pack, &config(512)).expect_err("unknown think role should fail");

    assert_eq!(error_code(error), "plan_unknown_character_think_role_id");
}

#[test]
fn output_rejects_player_character_think_target() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!([]), json!([{"role_id":"xu_xian","thinking_focus":"Choose a response."}]));

    let error = parse(&raw, &pack, &config(512)).expect_err("player think should fail");

    assert_eq!(error_code(error), "plan_player_character_think_forbidden");
}

#[test]
fn output_rejects_player_generation_role() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!(["xu_xian"]), json!([]));

    let error = parse(&raw, &pack, &config(512)).expect_err("player generation role should fail");

    assert_eq!(error_code(error), "plan_player_generation_role_forbidden");
}

#[test]
fn output_rejects_empty_thinking_focus() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!([]), json!([{"role_id":"bai_suzhen","thinking_focus":" \n "}]));

    let error = parse(&raw, &pack, &config(512)).expect_err("empty focus should fail");

    assert_eq!(error_code(error), "plan_thinking_focus_empty");
}

#[test]
fn output_rejects_oversized_thinking_focus() {
    let pack = candidate_pack();
    let raw = role_selection_output(json!([]), json!([{"role_id":"bai_suzhen","thinking_focus":"界界"}]));
    let mut config = config(512);
    config.max_thinking_focus_bytes = 5;

    let error = parse(&raw, &pack, &config).expect_err("oversized focus should fail");

    assert_eq!(error_code(error), "plan_thinking_focus_too_long");
}

#[test]
fn valid_role_selections_preserve_order_and_trim_thinking_focus() {
    let pack = candidate_pack();
    let raw = role_selection_output(
        json!(["xiaoqing", "bai_suzhen"]),
        json!([
            {"role_id":"bai_suzhen","thinking_focus":"  Choose a response.  "},
            {"role_id":"fahai","thinking_focus":"Assess the danger."}
        ]),
    );

    let output = parse(&raw, &pack, &config(512)).expect("valid role selections");

    assert_eq!(
        output.roles_for_generation.iter().map(RoleId::as_str).collect::<Vec<_>>(),
        ["xiaoqing", "bai_suzhen"]
    );
    assert_eq!(
        output
            .character_think_requests
            .iter()
            .map(|request| request.role_id.as_str())
            .collect::<Vec<_>>(),
        ["bai_suzhen", "fahai"]
    );
    assert_eq!(output.character_think_requests[0].thinking_focus.as_str(), "Choose a response.");
}
