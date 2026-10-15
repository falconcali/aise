use super::*;
use crate::core::tests::snake_pack;
use crate::core::{BoundedText, MemoryBody, MemoryId, ResolvedRole, RetrievalHint, RoleId};
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

#[test]
fn seed_memories_render_in_memory_id_order() {
    let role = role(&[("memory-b", "second", "hint-b"), ("memory-a", "first", "hint-a")]);

    let rendered = seed_memories(&role, &ThinkConfig::default());

    assert!(rendered.find("memory-a").expect("first memory rendered") < rendered.find("memory-b").expect("second"));
    assert!(rendered.contains("content: \"first\""));
    assert!(rendered.contains("retrieval_hint: \"hint-a\""));
}

#[test]
fn seed_memories_respect_count_limit() {
    let role = role(&[("memory-a", "first", "hint-a"), ("memory-b", "second", "hint-b")]);
    let config = ThinkConfig {
        max_seed_memories: 1,
        ..ThinkConfig::default()
    };

    let rendered = seed_memories(&role, &config);

    assert!(rendered.contains("memory-a"));
    assert!(!rendered.contains("memory-b"));
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
    assert!(!rendered.contains("memory-b"));
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
