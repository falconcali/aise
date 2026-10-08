use super::*;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const BUNDLED_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts");
const BASELINE_PROMPT_ID: &str = "baseline.process_player_input";
const BASELINE_TEMPLATE_FILE: &str = "baseline-process-player-input.md.j2";
const PLAN_PROMPT_ID: &str = "plan.process_story_plan";

fn config(dir: &Path) -> PromptConfig {
    PromptConfig {
        directory: dir.to_path_buf(),
        max_prompts: 16,
        max_template_bytes: 64 * 1024,
        max_total_template_bytes: 1024 * 1024,
    }
}

fn bundled_prompt() -> Prompt {
    Prompt::new(config(Path::new(BUNDLED_PROMPT_DIRECTORY))).expect("bundled prompt loads")
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
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "prompt-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn render(prompt: &Prompt, spec: PromptSpec<'_>) -> Result<RenderedPrompt, PromptError> {
    let trace = test_trace();
    prompt.render(spec, trace.root())
}

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aise-prompt-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

fn player_input_vars(input: &str) -> PromptVars {
    PromptVars::from([
        ("player_input".to_owned(), Value::String(input.to_owned())),
        ("story_summary".to_owned(), Value::String(String::new())),
        ("story_opening".to_owned(), Value::String(String::new())),
        ("recent_story".to_owned(), Value::String(String::new())),
    ])
}

fn bundled_template(layer: PromptLayer) -> String {
    let path = Path::new(BUNDLED_PROMPT_DIRECTORY)
        .join(layer.as_str())
        .join(BASELINE_TEMPLATE_FILE);
    let source = fs::read_to_string(path).expect("read bundled template");
    source.trim_end_matches(['\r', '\n']).to_owned()
}

fn copy_bundled_assets(dir: &Path) {
    fs::copy(Path::new(BUNDLED_PROMPT_DIRECTORY).join("index.toml"), dir.join("index.toml")).expect("copy manifest");
    for layer in PromptLayer::ORDERED {
        let source_dir = Path::new(BUNDLED_PROMPT_DIRECTORY).join(layer.as_str());
        let target_dir = dir.join(layer.as_str());
        fs::create_dir_all(&target_dir).expect("create layer dir");
        for entry in fs::read_dir(source_dir).expect("read layer dir") {
            let entry = entry.expect("read layer entry");
            let source = entry.path();
            if source.is_file() {
                fs::copy(&source, target_dir.join(entry.file_name())).expect("copy template");
            }
        }
    }
}

#[test]
fn bundled_catalog_has_expected_prompts() {
    let prompt = bundled_prompt();
    let mut prompt_ids = prompt.prompt_ids().collect::<Vec<_>>();
    prompt_ids.sort_unstable();
    assert_eq!(
        prompt_ids,
        vec![
            BASELINE_PROMPT_ID,
            "generate.generate_story",
            PLAN_PROMPT_ID,
            "summary.summarize_story"
        ]
    );
}

#[test]
fn renders_baseline_three_messages_in_order() {
    let prompt = bundled_prompt();
    let rendered = render(
        &prompt,
        PromptSpec::new(BASELINE_PROMPT_ID, player_input_vars("I open the door.")),
    )
    .expect("render");
    let messages = rendered.messages();
    let roles = messages.iter().map(|message| message.role.clone()).collect::<Vec<_>>();
    assert_eq!(
        roles,
        vec![ChatMessageRole::System, ChatMessageRole::User, ChatMessageRole::System]
    );
    assert!(messages[1].content.contains("I open the door."));
    assert_eq!(messages[0].content, bundled_template(PromptLayer::Csi));
    assert_eq!(messages[2].content, bundled_template(PromptLayer::Fti));
}

#[test]
fn same_vars_feed_all_layers() {
    let dir = fixture_dir("same-vars");
    fs::write(
        dir.join("index.toml"),
        "[[prompts]]\nid = \"shared\"\ncsi = \"c.j2\"\nrc = \"r.j2\"\nfti = \"f.j2\"\n",
    )
    .expect("write manifest");
    fs::write(dir.join("c.j2"), "csi {{ v }}").expect("write csi");
    fs::write(dir.join("r.j2"), "rc {{ v }}").expect("write rc");
    fs::write(dir.join("f.j2"), "fti {{ v }}").expect("write fti");
    let prompt = Prompt::new(config(&dir)).expect("fixture prompt loads");
    let rendered = render(
        &prompt,
        PromptSpec::new("shared", PromptVars::from([("v".to_owned(), json!("value"))])),
    )
    .expect("render");
    let contents = rendered
        .messages()
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["csi value", "rc value", "fti value"]);
}

#[test]
fn unknown_prompt_id_is_not_found() {
    let prompt = bundled_prompt();
    let error = render(&prompt, PromptSpec::new("unknown.prompt", PromptVars::new())).expect_err("render should fail");
    assert!(matches!(error, PromptError::PromptNotFound { ref prompt_id } if prompt_id == "unknown.prompt"));
}

#[test]
fn missing_player_input_fails() {
    let prompt = bundled_prompt();
    let error =
        render(&prompt, PromptSpec::new(BASELINE_PROMPT_ID, PromptVars::new())).expect_err("render should fail");
    assert!(matches!(
        error,
        PromptError::TemplateRender {
            layer: PromptLayer::Rc,
            ..
        }
    ));
}

#[test]
fn renders_after_resource_directory_deleted() {
    let dir = fixture_dir("deleted-dir");
    copy_bundled_assets(&dir);
    let prompt = Prompt::new(config(&dir)).expect("copied prompt loads");
    fs::remove_dir_all(&dir).expect("delete fixture dir");
    let rendered =
        render(&prompt, PromptSpec::new(BASELINE_PROMPT_ID, player_input_vars("hello"))).expect("render after delete");
    assert_eq!(rendered.messages().len(), 3);
}

#[test]
fn rendered_prompt_exposes_id_and_messages() {
    let prompt = bundled_prompt();
    let rendered = render(&prompt, PromptSpec::new(BASELINE_PROMPT_ID, player_input_vars("wave"))).expect("render");
    assert_eq!(rendered.prompt_id(), BASELINE_PROMPT_ID);
    let expected = rendered.messages().to_vec();
    assert_eq!(rendered.into_messages(), expected);
}

#[test]
fn prompt_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Prompt>();
}
