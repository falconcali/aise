use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_TOKENS: &[&str] = &[
    "crate::runtime::pipeline",
    "crate::runtime::turn_budget",
    "crate::runtime::turn_execution_ctx",
    "crate::runtime::event",
    "crate::runtime::trace",
    "StoryDraft",
    "lock_turn",
    "turn_lock",
];

const BUSINESS_DIRS: &[&str] = &["context", "planning", "character", "story", "validation"];
const PIPELINE_DIRS: &[&str] = &["context", "planning", "character", "story", "validation"];

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn module_of(path: &Path) -> Option<&str> {
    let rel = path.strip_prefix(src_root()).ok()?;
    let first = rel.components().next()?.as_os_str().to_str()?;
    if first.ends_with(".rs") { None } else { Some(first) }
}

fn forbidden_imports(module: &str) -> Vec<String> {
    match module {
        "core" => [
            "runtime",
            "context",
            "planning",
            "character",
            "story",
            "validation",
            "llm",
            "persistence",
        ]
        .iter()
        .map(|m| format!("crate::{m}"))
        .collect(),
        "domain" => [
            "runtime",
            "context",
            "planning",
            "character",
            "story",
            "validation",
            "llm",
            "persistence",
        ]
        .iter()
        .map(|m| format!("crate::{m}"))
        .collect(),
        "llm" => [
            "runtime",
            "context",
            "planning",
            "character",
            "story",
            "validation",
            "persistence",
        ]
        .iter()
        .map(|m| format!("crate::{m}"))
        .collect(),
        "runtime" => [
            "context",
            "planning",
            "character",
            "story",
            "validation",
            "llm",
            "persistence",
        ]
        .iter()
        .map(|m| format!("crate::{m}"))
        .collect(),
        _ if PIPELINE_DIRS.contains(&module) => ["runtime"]
            .into_iter()
            .chain(PIPELINE_DIRS.iter().filter(|m| **m != module).copied())
            .map(|m| format!("crate::{m}"))
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn no_backwards_dependencies_or_dead_paths() {
    let mut files = Vec::new();
    walk_rs_files(&src_root(), &mut files);
    assert!(!files.is_empty(), "source files found for static checks");

    let mut violations = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read source file");
        let rel = path.strip_prefix(src_root()).unwrap().display().to_string();

        for token in FORBIDDEN_TOKENS {
            if content.contains(token) {
                violations.push(format!("{rel}: forbidden token {token:?}"));
            }
        }

        let module = module_of(&path);
        if let Some(module) = module {
            for forbidden in forbidden_imports(module) {
                if content.contains(&forbidden) {
                    violations.push(format!("{rel}: module {module:?} must not import {forbidden}"));
                }
            }
            if BUSINESS_DIRS.contains(&module) && content.contains("LlmProvider") {
                violations.push(format!(
                    "{rel}: business module {module:?} must not depend on concrete LlmProvider"
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "dependency direction violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn core_has_no_outer_transitive_dependency() {
    let mut files = Vec::new();
    walk_rs_files(&src_root().join("core"), &mut files);
    walk_rs_files(&src_root().join("domain"), &mut files);
    let mut violations = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read source file");
        let rel = path.strip_prefix(src_root()).unwrap().display().to_string();
        for outer in ["crate::llm", "crate::persistence", "crate::runtime", "crate::server"] {
            if content.contains(&format!("{outer}::")) {
                violations.push(format!("{rel}: core/domain must not transitively depend on {outer}"));
            }
        }
        for pipeline in [
            "crate::context",
            "crate::planning",
            "crate::character",
            "crate::story",
            "crate::validation",
        ] {
            if content.contains(&format!("{pipeline}::")) {
                violations.push(format!("{rel}: core/domain must not depend on pipeline module {pipeline}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "core/domain transitive dependency violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn adapter_transport_errors_stay_private() {
    let public_faces = [
        "core",
        "domain",
        "persistence/store.rs",
        "llm/error.rs",
        "llm/provider.rs",
        "llm/gateway.rs",
    ];
    let mut violations = Vec::new();
    for face in public_faces {
        let path = src_root().join(face);
        if path.is_dir() {
            let mut files = Vec::new();
            walk_rs_files(&path, &mut files);
            for file in files {
                let content = fs::read_to_string(&file).expect("read public surface source file");
                for token in ["reqwest::Error", "sqlx::Error"] {
                    if content.contains(token) {
                        violations.push(format!("{face}: public error surface leaks {token}"));
                    }
                }
            }
        } else {
            let content = fs::read_to_string(&path).expect("read public error surface");
            for token in ["reqwest::Error", "sqlx::Error"] {
                if content.contains(token) {
                    violations.push(format!("{face}: public error surface leaks {token}"));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "transport errors must stay adapter-private:\n{}",
        violations.join("\n")
    );
}

#[test]
fn public_error_types_are_self_contained_and_static() {
    fn assert_static<T: Send + Sync + 'static>() {}
    assert_static::<aise::llm::error::LlmProviderError>();
    assert_static::<aise::llm::error::LlmError>();
    assert_static::<aise::persistence::StoreError>();
    assert_static::<aise::core::turn_error::TurnExecutionError>();
}

fn server_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates dir")
        .join("aise-server")
        .join("src")
}

#[test]
fn no_legacy_patterns_in_server_src() {
    let mut files = Vec::new();
    walk_rs_files(&server_src_root(), &mut files);
    assert!(!files.is_empty(), "aise-server source files found for static checks");
    let mut violations = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read server source file");
        for token in ["StoryDraft", "lock_turn", "turn_lock"] {
            if content.contains(token) {
                violations.push(format!("{}: forbidden legacy token {token:?}", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "legacy pattern violations in aise-server:\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_create_story_in_turn_execution_path() {
    let files = [
        src_root().join("engine.rs"),
        server_src_root().join("api").join("turn.rs"),
    ];
    for path in files {
        let content = fs::read_to_string(&path).expect("read turn execution file");
        assert!(
            !content.contains("create_story"),
            "{} must not auto-create stories during turn execution",
            path.display()
        );
    }
}

#[test]
fn no_legacy_summary_delta_patterns() {
    let mut files = Vec::new();
    walk_rs_files(&src_root().join("context"), &mut files);
    walk_rs_files(&src_root().join("persistence"), &mut files);
    let mut violations = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read context or persistence source file");
        if content.contains("summary_delta") {
            violations.push(format!("{}: legacy summary_delta pattern", path.display()));
        }
        for line in content.lines() {
            if line.contains("story_text") && line.contains("current_scene") {
                violations.push(format!("{}: legacy story_text/current_scene coupling", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "legacy summary coupling violations:\n{}",
        violations.join("\n")
    );
}
