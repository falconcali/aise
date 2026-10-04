use super::*;
use serde_json::{Value, json};
use std::sync::Arc;

fn loaded(csi: &str, rc: &str, fti: &str) -> LoadedPrompt {
    LoadedPrompt {
        id: Arc::from("test.prompt"),
        csi: csi.to_owned(),
        rc: rc.to_owned(),
        fti: fti.to_owned(),
    }
}

fn vars(entries: &[(&str, Value)]) -> PromptVars {
    entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn render_rc(rc: &str, vars: &PromptVars) -> Result<String, PromptError> {
    let mut renderer = PromptRenderer::new();
    let names = renderer.compile(&loaded("csi", rc, "fti")).expect("compile");
    renderer.render(&names, "test.prompt", PromptLayer::Rc, vars)
}

#[test]
fn template_name_joins_prompt_id_and_layer() {
    assert_eq!(
        template_name("baseline.process_player_input", PromptLayer::Csi),
        "baseline.process_player_input/csi"
    );
}

#[test]
fn rejects_template_syntax_error() {
    let mut renderer = PromptRenderer::new();
    let error = renderer
        .compile(&loaded("ok", "{{ broken", "ok"))
        .err()
        .expect("compile should fail");
    assert!(matches!(
        error,
        PromptError::TemplateCompile {
            layer: PromptLayer::Rc,
            ..
        }
    ));
}

#[test]
fn strict_mode_fails_on_missing_variable() {
    let error = render_rc("{{ missing }}", &PromptVars::new()).expect_err("render should fail");
    assert!(matches!(
        error,
        PromptError::TemplateRender {
            layer: PromptLayer::Rc,
            ..
        }
    ));
}

#[test]
fn ignores_unreferenced_variables() {
    let rendered = render_rc("{{ v }}", &vars(&[("v", json!("a")), ("unused", json!("b"))])).expect("render");
    assert_eq!(rendered, "a");
}

#[test]
fn does_not_escape_html() {
    let rendered = render_rc("{{ v }}", &vars(&[("v", json!("<a&b>"))])).expect("render");
    assert_eq!(rendered, "<a&b>");
}

#[test]
fn renders_non_string_values() {
    let values = vars(&[
        ("n", json!(42)),
        ("b", json!(true)),
        ("a", json!([1, 2])),
        ("o", json!({"k": "v"})),
        ("z", Value::Null),
    ]);
    let rendered = render_rc("{{ n }}|{{ b }}|{{ a }}|{{ o }}|{{ z }}", &values).expect("render");
    assert!(rendered.starts_with("42|true|"));
}
