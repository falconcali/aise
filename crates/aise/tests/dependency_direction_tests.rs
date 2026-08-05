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
            "core",
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
