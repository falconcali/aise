use super::*;

#[test]
fn name_returns_variant_name_field() {
    let preamble = PromptPolicy::Preamble {
        name: "preamble".to_string(),
        content: "always be safe".to_string(),
        position: PreamblePosition::Prepend,
    };
    let guard = PromptPolicy::RuntimeGuard {
        name: "guard".to_string(),
        description: "extra runtime checks".to_string(),
    };
    let validator = PromptPolicy::PostValidator {
        name: "validator".to_string(),
    };

    assert_eq!(preamble.name(), "preamble");
    assert_eq!(guard.name(), "guard");
    assert_eq!(validator.name(), "validator");
}

#[test]
fn preamble_prepend_adds_content_before_text() {
    let policy = PromptPolicy::Preamble {
        name: "safety".to_string(),
        content: "be careful".to_string(),
        position: PreamblePosition::Prepend,
    };

    let rendered = policy.apply_to_text("answer the user").unwrap();

    assert_eq!(rendered, "be careful\nanswer the user");
}

#[test]
fn preamble_append_adds_content_after_text() {
    let policy = PromptPolicy::Preamble {
        name: "footer".to_string(),
        content: "stay concise".to_string(),
        position: PreamblePosition::Append,
    };

    let rendered = policy.apply_to_text("answer the user").unwrap();

    assert_eq!(rendered, "answer the user\nstay concise");
}

#[test]
fn runtime_guard_does_not_modify_text() {
    let policy = PromptPolicy::RuntimeGuard {
        name: "guard".to_string(),
        description: "runtime only".to_string(),
    };

    assert!(policy.apply_to_text("hello").is_none());
}

#[test]
fn post_validator_does_not_modify_text() {
    let policy = PromptPolicy::PostValidator {
        name: "validator".to_string(),
    };

    assert!(policy.apply_to_text("hello").is_none());
}
