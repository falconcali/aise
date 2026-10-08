use super::*;
use std::fs;
use std::path::{Path, PathBuf};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aise-prompt-loader-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

fn config(dir: &Path) -> PromptConfig {
    PromptConfig {
        directory: dir.to_path_buf(),
        max_prompts: 4,
        max_template_bytes: 1024,
        max_total_template_bytes: 4096,
    }
}

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, content).expect("write fixture");
}

fn entry(id: &str, csi: &str, rc: &str, fti: &str) -> String {
    format!("[[prompts]]\nid = \"{id}\"\ncsi = \"{csi}\"\nrc = \"{rc}\"\nfti = \"{fti}\"\n")
}

fn write_templates(dir: &Path, content: &str) {
    write(dir, "c.j2", content);
    write(dir, "r.j2", content);
    write(dir, "f.j2", content);
}

fn load_error(config: &PromptConfig) -> PromptError {
    load_catalog(config).err().expect("load should fail")
}

#[test]
fn rejects_zero_max_prompts() {
    let mut config = config(Path::new("unused"));
    config.max_prompts = 0;
    let error = load_error(&config);
    assert!(matches!(
        error,
        PromptError::InvalidConfig {
            field: "max_prompts",
            ..
        }
    ));
}

#[test]
fn rejects_zero_max_template_bytes() {
    let mut config = config(Path::new("unused"));
    config.max_template_bytes = 0;
    let error = load_error(&config);
    assert!(matches!(
        error,
        PromptError::InvalidConfig {
            field: "max_template_bytes",
            ..
        }
    ));
}

#[test]
fn rejects_zero_max_total_template_bytes() {
    let mut config = config(Path::new("unused"));
    config.max_total_template_bytes = 0;
    let error = load_error(&config);
    assert!(matches!(
        error,
        PromptError::InvalidConfig {
            field: "max_total_template_bytes",
            ..
        }
    ));
}

#[test]
fn rejects_missing_manifest() {
    let dir = fixture_dir("missing-manifest");
    let error = load_error(&config(&dir));
    assert!(matches!(error, PromptError::ManifestRead { .. }));
    assert_eq!(error.kind(), "manifest_read");
}

#[test]
fn rejects_unknown_root_field() {
    let dir = fixture_dir("unknown-root-field");
    write(
        &dir,
        MANIFEST_FILE_NAME,
        &format!("extra = 1\n{}", entry("p", "c.j2", "r.j2", "f.j2")),
    );
    assert!(matches!(load_error(&config(&dir)), PromptError::ManifestParse { .. }));
}

#[test]
fn rejects_unknown_entry_field() {
    let dir = fixture_dir("unknown-entry-field");
    write(
        &dir,
        MANIFEST_FILE_NAME,
        &format!("{}extra = \"x\"\n", entry("p", "c.j2", "r.j2", "f.j2")),
    );
    assert!(matches!(load_error(&config(&dir)), PromptError::ManifestParse { .. }));
}

#[test]
fn rejects_missing_layer_field() {
    let dir = fixture_dir("missing-layer-field");
    write(
        &dir,
        MANIFEST_FILE_NAME,
        "[[prompts]]\nid = \"p\"\ncsi = \"c.j2\"\nrc = \"r.j2\"\n",
    );
    assert!(matches!(load_error(&config(&dir)), PromptError::ManifestParse { .. }));
}

#[test]
fn rejects_empty_prompt_id() {
    let dir = fixture_dir("empty-prompt-id");
    write(&dir, MANIFEST_FILE_NAME, &entry("  ", "c.j2", "r.j2", "f.j2"));
    write_templates(&dir, "x");
    assert!(matches!(load_error(&config(&dir)), PromptError::EmptyPromptId { index: 0 }));
}

#[test]
fn rejects_duplicate_prompt_id() {
    let dir = fixture_dir("duplicate-prompt-id");
    write(
        &dir,
        MANIFEST_FILE_NAME,
        &format!("{}{}", entry("p", "c.j2", "r.j2", "f.j2"), entry("p", "c.j2", "r.j2", "f.j2")),
    );
    write_templates(&dir, "x");
    let error = load_error(&config(&dir));
    assert!(matches!(error, PromptError::DuplicatePromptId { ref prompt_id } if prompt_id == "p"));
}

#[test]
fn rejects_too_many_prompts() {
    let dir = fixture_dir("too-many-prompts");
    write(
        &dir,
        MANIFEST_FILE_NAME,
        &format!("{}{}", entry("a", "c.j2", "r.j2", "f.j2"), entry("b", "c.j2", "r.j2", "f.j2")),
    );
    let mut config = config(&dir);
    config.max_prompts = 1;
    assert!(matches!(
        load_error(&config),
        PromptError::TooManyPrompts { count: 2, limit: 1 }
    ));
}

#[test]
fn rejects_empty_template_path() {
    let dir = fixture_dir("empty-template-path");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "", "r.j2", "f.j2"));
    assert!(matches!(
        load_error(&config(&dir)),
        PromptError::InvalidTemplatePath {
            layer: PromptLayer::Csi,
            reason: "empty",
            ..
        }
    ));
}

#[test]
fn rejects_absolute_template_path() {
    let dir = fixture_dir("absolute-template-path");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "c.j2", "/abs/r.j2", "f.j2"));
    write_templates(&dir, "x");
    assert!(matches!(
        load_error(&config(&dir)),
        PromptError::InvalidTemplatePath {
            layer: PromptLayer::Rc,
            reason: "absolute",
            ..
        }
    ));
}

#[test]
fn rejects_parent_component_path() {
    let dir = fixture_dir("parent-component-path");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "c.j2", "r.j2", "../f.j2"));
    write_templates(&dir, "x");
    assert!(matches!(
        load_error(&config(&dir)),
        PromptError::InvalidTemplatePath {
            layer: PromptLayer::Fti,
            reason: "parent_component",
            ..
        }
    ));
}

#[test]
fn rejects_missing_template_file() {
    let dir = fixture_dir("missing-template-file");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "c.j2", "missing.j2", "f.j2"));
    write_templates(&dir, "x");
    let error = load_error(&config(&dir));
    match error {
        PromptError::TemplateRead {
            prompt_id, layer, path, ..
        } => {
            assert_eq!(prompt_id, "p");
            assert_eq!(layer, PromptLayer::Rc);
            assert!(path.ends_with("missing.j2"));
        }
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn rejects_oversized_template() {
    let dir = fixture_dir("oversized-template");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "c.j2", "r.j2", "f.j2"));
    write_templates(&dir, &"x".repeat(100));
    let mut config = config(&dir);
    config.max_template_bytes = 64;
    assert!(matches!(
        load_error(&config),
        PromptError::TemplateTooLarge {
            layer: PromptLayer::Csi,
            limit: 64,
            ..
        }
    ));
}

#[test]
fn rejects_total_template_bytes_exceeded() {
    let dir = fixture_dir("total-template-bytes");
    write(&dir, MANIFEST_FILE_NAME, &entry("p", "c.j2", "r.j2", "f.j2"));
    write_templates(&dir, &"x".repeat(40));
    let mut config = config(&dir);
    config.max_total_template_bytes = 100;
    assert!(matches!(
        load_error(&config),
        PromptError::TotalTemplateBytesExceeded { bytes: 120, limit: 100 }
    ));
}

#[test]
fn loads_bundled_assets() {
    let mut config = config(Path::new(BUNDLED_PROMPT_DIRECTORY));
    config.max_template_bytes = 64 * 1024;
    config.max_total_template_bytes = 1024 * 1024;
    let catalog = load_catalog(&config).expect("bundled assets load");
    assert_eq!(catalog.prompts.len(), 4);
    for prompt in &catalog.prompts {
        assert!(
            [
                "baseline.process_player_input",
                "plan.process_story_plan",
                "generate.generate_story",
                "summary.summarize_story",
            ]
            .contains(&&*prompt.id)
        );
        for layer in PromptLayer::ORDERED {
            assert!(!prompt.source(layer).is_empty());
        }
    }
}
