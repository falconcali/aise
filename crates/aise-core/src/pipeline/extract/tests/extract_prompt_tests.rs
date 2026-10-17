use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    ActivationRule, BoundedText, EntryOrigin, FactEntry, MemoryBody, MemoryEntry, MemoryId, NarrativeConditionId,
    RetrievalHint, RoleActor, RoleEntry, RumorEntry, StorySummary, TurnNumber,
};
use crate::prompt::PromptConfig;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

fn bundled_prompt() -> Prompt {
    Prompt::new(PromptConfig {
        directory: Path::new(BUNDLED_PROMPT_DIRECTORY).to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    })
    .expect("bundled prompt loads")
}

fn test_trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 4096,
            max_observation_bytes: 8192,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "extract-prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn query(id: &str, criterion: &str) -> SemanticQuery {
    SemanticQuery {
        condition_id: NarrativeConditionId::try_new(id).expect("valid condition id"),
        criterion: BoundedText::try_new(criterion, "criterion", 256).expect("valid criterion"),
    }
}

fn populated_context(pack: &StoryPack) -> StoryContext {
    let mut story_ctx = StoryContext::new();
    story_ctx.facts = pack
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
    story_ctx.rumors = pack
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
    story_ctx.roles = pack
        .roles
        .iter()
        .map(|(id, body)| {
            let mut memories = body
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
            let secret_id = MemoryId::try_new(format!("secret_mem_{id}")).expect("valid memory id");
            memories.insert(
                secret_id.clone(),
                MemoryEntry {
                    id: secret_id,
                    origin: EntryOrigin::Turn(TurnNumber::new(1)),
                    body: MemoryBody {
                        content: BoundedText::try_new(format!("PRIVATE_SECRET_OF_{id}"), "mem", 256)
                            .expect("valid text"),
                        retrieval: RetrievalHint::try_new(format!("secret hint {id}")).expect("valid hint"),
                        activation: serde_json::from_value::<ActivationRule>(
                            json!({ "match": { "keys": ["secret"] } }),
                        )
                        .expect("valid rule"),
                    },
                },
            );
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

fn render_narrative(story_ctx: &StoryContext) -> String {
    let prompt = bundled_prompt();
    let story_pack = snake_pack();
    let queries = [query("gate_open", "The gate is open")];
    let trace = test_trace();
    evaluate_narrative(
        &prompt,
        EvaluateNarrativePrompt {
            story_pack: &story_pack,
            story_ctx,
            proposal: "The gate opens.",
            queries: &queries,
        },
        trace.root(),
    )
    .expect("prompt should render")
    .messages()[1]
        .content
        .replace("\r\n", "\n")
}

#[test]
fn prompt_uses_opening_without_summary_and_serializes_queries() {
    let story_ctx = StoryContext::new();

    let content = render_narrative(&story_ctx);

    assert!(content.contains("# Story Opening"));
    assert!(!content.contains("# Story Summary"));
    assert!(content.contains("# Current Proposal\n\nThe gate opens."));
    assert!(content.contains(r#"[{"condition_id":"gate_open","criterion":"The gate is open"}]"#));
}

#[test]
fn prompt_uses_summary_instead_of_opening() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(StorySummary {
        text: "Earlier events.".to_owned(),
        covered_through: TurnNumber::new(2),
    });

    let content = render_narrative(&story_ctx);

    assert!(content.contains("# Story Summary\n\nEarlier events."));
    assert!(!content.contains("# Story Opening"));
}

#[test]
fn world_knowledge_prompt_includes_facts_rumors_and_role_catalog_without_leaking_memories() {
    let prompt = bundled_prompt();
    let story_pack = snake_pack();
    let story_ctx = populated_context(&story_pack);
    let config = ExtractConfig::default();
    let trace = test_trace();

    let rendered = extract_world_knowledge(
        &prompt,
        ExtractWorldKnowledgePrompt {
            story_pack: &story_pack,
            story_ctx: &story_ctx,
            proposal: "Bai Suzhen hands the umbrella to Xu Xian.",
            config: &config,
        },
        trace.root(),
    )
    .expect("world knowledge prompt should render");

    let csi = &rendered.messages()[0].content;
    let rc = rendered.messages()[1].content.replace("\r\n", "\n");
    let fti = &rendered.messages()[2].content;

    assert!(csi.contains("ActivationRule"));
    assert!(fti.contains("\"roles_for_memory\""));
    assert!(rc.contains("# Existing Facts"));
    assert!(rc.contains("# Existing Rumors"));
    assert!(rc.contains("# Role Catalog"));
    assert!(rc.contains("\"role_id\":\"bai_suzhen\""));
    assert!(rc.contains("max_fact_changes: 8"));
    assert!(rc.contains("max_rumor_changes: 8"));
    assert!(rc.contains("max_roles_for_memory: 4"));
    for fact_id in story_ctx.facts.keys() {
        assert!(rc.contains(fact_id.as_str()));
    }
    for rumor_id in story_ctx.rumors.keys() {
        assert!(rc.contains(rumor_id.as_str()));
    }
    assert!(!rc.contains("PRIVATE_SECRET_OF_"));
    assert!(!rc.contains("secret_mem_"));
}

#[test]
fn role_memory_prompt_isolates_target_role_memories_and_enforces_perspective_boundary() {
    let prompt = bundled_prompt();
    let story_pack = snake_pack();
    let story_ctx = populated_context(&story_pack);
    let config = ExtractConfig::default();
    let trace = test_trace();
    let target_role_id = RoleId::try_new("bai_suzhen").expect("valid role id");

    let rendered = extract_role_memory(
        &prompt,
        ExtractRoleMemoryPrompt {
            story_pack: &story_pack,
            story_ctx: &story_ctx,
            proposal: "While Bai Suzhen was absent, Fahai secretly spoke with Xu Xian and thought to himself.",
            role_id: &target_role_id,
            config: &config,
        },
        trace.root(),
    )
    .expect("role memory prompt should render");

    let csi = &rendered.messages()[0].content;
    let rc = rendered.messages()[1].content.replace("\r\n", "\n");
    let fti = &rendered.messages()[2].content;

    assert!(csi.contains("personally experienced"));
    assert!(csi.contains("inner thoughts"));
    assert!(csi.contains("off-scene"));
    assert!(csi.contains("subjective viewpoint"));
    assert!(fti.contains("perception boundary"));
    assert!(rc.contains("- role_id: bai_suzhen"));
    assert!(rc.contains("PRIVATE_SECRET_OF_bai_suzhen"));
    assert!(rc.contains("max_memory_changes_per_role: 4"));
    assert!(!rc.contains("PRIVATE_SECRET_OF_xu_xian"));
    assert!(!rc.contains("PRIVATE_SECRET_OF_xiaoqing"));
    assert!(!rc.contains("PRIVATE_SECRET_OF_fahai"));
    for fact_id in story_ctx.facts.keys() {
        assert!(!rc.contains(fact_id.as_str()));
    }
}
