use super::*;
use crate::core::tests::snake_pack;
use crate::core::{
    BoundedText, EntryOrigin, MemoryBody, MemoryId, ResolvedRole, RetrievalHint, RoleId, RumorBody, RumorEntry, RumorId,
};
use std::collections::BTreeMap;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn memory(content: &str, hint: &str) -> MemoryBody {
    MemoryBody {
        content: text(content),
        activation: Default::default(),
        retrieval: RetrievalHint::try_new(hint).expect("valid retrieval hint"),
    }
}

fn role(memories: &[(&str, &str, &str)]) -> ResolvedRole {
    let pack = snake_pack();
    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");
    let body = pack.roles[&role_id].clone();
    ResolvedRole {
        role_id,
        name: body.name,
        aliases: body.alias,
        backstory: body.backstory,
        retrieval_hint: body.retrieval,
        profile: body.default_profile,
        seed_memories: memories
            .iter()
            .map(|(id, content, hint)| (MemoryId::try_new(*id).expect("valid memory id"), memory(content, hint)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn rumor(id: &str, content: &str, hint: &str) -> RumorEntry {
    RumorEntry {
        id: RumorId::try_new(id).expect("valid rumor id"),
        origin: EntryOrigin::Seed,
        body: RumorBody {
            content: text(content),
            activation: Default::default(),
            retrieval: RetrievalHint::try_new(hint).expect("valid retrieval hint"),
        },
    }
}

#[test]
fn seed_memories_render_only_content_in_memory_id_order() {
    let role = role(&[("memory-b", "second", "hint-b"), ("memory-a", "first", "hint-a")]);

    let rendered = seed_memories(&role, &ThinkConfig::default());

    assert_eq!(rendered, "- \"first\"\n- \"second\"");
    assert!(!rendered.contains("memory-a"));
    assert!(!rendered.contains("hint-a"));
}

#[test]
fn seed_memories_respect_count_limit() {
    let role = role(&[("memory-a", "first", "hint-a"), ("memory-b", "second", "hint-b")]);
    let config = ThinkConfig {
        max_seed_memories: 1,
        ..ThinkConfig::default()
    };

    let rendered = seed_memories(&role, &config);

    assert!(rendered.contains("first"));
    assert!(!rendered.contains("second"));
}

#[test]
fn seed_memories_respect_byte_limit_without_partial_item() {
    let role = role(&[("memory-a", "first", "hint-a"), ("memory-b", "second", "hint-b")]);
    let first = seed_memories(
        &role,
        &ThinkConfig {
            max_seed_memories: 1,
            ..ThinkConfig::default()
        },
    );
    let config = ThinkConfig {
        max_seed_memory_bytes: first.len(),
        ..ThinkConfig::default()
    };

    let rendered = seed_memories(&role, &config);

    assert_eq!(rendered, first);
    assert!(!rendered.contains("second"));
}

#[test]
fn seed_memories_stop_when_first_item_exceeds_budget() {
    let role = role(&[("memory-a", "first", "hint-a"), ("memory-b", "second", "hint-b")]);
    let config = ThinkConfig {
        max_seed_memory_bytes: 1,
        ..ThinkConfig::default()
    };

    assert!(seed_memories(&role, &config).is_empty());
}

#[test]
fn zero_memory_limits_render_empty_context() {
    let role = role(&[("memory-a", "first", "hint-a")]);

    assert!(
        seed_memories(
            &role,
            &ThinkConfig {
                max_seed_memories: 0,
                ..ThinkConfig::default()
            }
        )
        .is_empty()
    );
    assert!(
        seed_memories(
            &role,
            &ThinkConfig {
                max_seed_memory_bytes: 0,
                ..ThinkConfig::default()
            }
        )
        .is_empty()
    );
}

#[test]
fn rumors_render_activated_then_retrieved_content_only() {
    let activated = [rumor("rumor-a", "first", "hint-a")];
    let retrieved = [rumor("rumor-b", "second", "hint-b")];

    let rendered = rumors(&activated, &retrieved, &ThinkConfig::default());

    assert_eq!(rendered, "- \"first\"\n- \"second\"");
    assert!(!rendered.contains("rumor-a"));
    assert!(!rendered.contains("hint-a"));
}

#[test]
fn rumors_respect_byte_limit_without_partial_item() {
    let activated = [rumor("rumor-a", "first", "hint-a")];
    let retrieved = [rumor("rumor-b", "second", "hint-b")];
    let first = rumors(&activated, &[], &ThinkConfig::default());
    let config = ThinkConfig {
        max_rumor_bytes: first.len(),
        ..ThinkConfig::default()
    };

    let rendered = rumors(&activated, &retrieved, &config);

    assert_eq!(rendered, first);
    assert!(!rendered.contains("second"));
}

#[test]
fn zero_rumor_budget_renders_empty_context() {
    let activated = [rumor("rumor-a", "first", "hint-a")];
    let config = ThinkConfig {
        max_rumor_bytes: 0,
        ..ThinkConfig::default()
    };

    assert!(rumors(&activated, &[], &config).is_empty());
}
