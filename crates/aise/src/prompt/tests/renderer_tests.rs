use super::*;
use crate::prompt::model::PromptRole;
use serde_json::json;

#[test]
fn simple_variable_substitution() {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("greet", "Hello, {{ name }}!").unwrap();

    let vars = HashMap::from([("name".to_string(), json!("World"))]);
    let result = renderer.render("greet", &vars).unwrap();

    assert_eq!(result, "Hello, World!");
}

#[test]
fn conditional_rendering() {
    let mut renderer = PromptRenderer::new();
    renderer
        .add_template("cond", "{% if verbose %}Detailed: {{ detail }}{% else %}Brief{% endif %}")
        .unwrap();

    let verbose_vars = HashMap::from([
        ("verbose".to_string(), json!(true)),
        ("detail".to_string(), json!("full explanation")),
    ]);
    let brief_vars = HashMap::from([
        ("verbose".to_string(), json!(false)),
        ("detail".to_string(), json!("full explanation")),
    ]);

    assert_eq!(renderer.render("cond", &verbose_vars).unwrap(), "Detailed: full explanation");
    assert_eq!(renderer.render("cond", &brief_vars).unwrap(), "Brief");
}

#[test]
fn render_prompt_text_kind_returns_text() {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("text", "You are {{ role }}.").unwrap();

    let vars = HashMap::from([("role".to_string(), json!("a helpful assistant"))]);
    let rendered = renderer.render_prompt("text", &PromptKind::Text, &vars).unwrap();

    assert_eq!(rendered, RenderedPrompt::Text("You are a helpful assistant.".to_string()));
}

#[test]
fn render_prompt_fragment_kind_returns_text() {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("fragment", "fragment: {{ value }}").unwrap();

    let vars = HashMap::from([("value".to_string(), json!(42))]);
    let rendered = renderer.render_prompt("fragment", &PromptKind::Fragment, &vars).unwrap();

    assert_eq!(rendered.as_text(), Some("fragment: 42"));
}

#[test]
fn render_prompt_messages_kind_parses_json_messages() {
    let mut renderer = PromptRenderer::new();
    renderer
        .add_template("messages", r#"[{"role":"system","content":"You are {{ role }}."}]"#)
        .unwrap();

    let vars = HashMap::from([("role".to_string(), json!("a tutor"))]);
    let rendered = renderer.render_prompt("messages", &PromptKind::Messages, &vars).unwrap();

    let messages = rendered.as_messages().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, PromptRole::System);
    assert_eq!(messages[0].content, "You are a tutor.");
}

#[test]
fn render_prompt_few_shot_kind_parses_json_messages() {
    let mut renderer = PromptRenderer::new();
    renderer
        .add_template(
            "few_shot",
            r#"[{"role":"user","content":"example question"},{"role":"assistant","content":"example answer"}]"#,
        )
        .unwrap();

    let rendered = renderer
        .render_prompt("few_shot", &PromptKind::FewShot, &HashMap::new())
        .unwrap();

    let messages = rendered.as_messages().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, PromptRole::User);
    assert_eq!(messages[1].role, PromptRole::Assistant);
}

#[test]
fn missing_template_returns_render_error() {
    let renderer = PromptRenderer::new();

    let err = renderer.render("missing", &HashMap::new()).unwrap_err();

    assert!(matches!(err, PromptError::RenderError(_)));
}

#[test]
fn invalid_message_json_returns_render_error() {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("bad", "not json").unwrap();

    let err = renderer
        .render_prompt("bad", &PromptKind::Messages, &HashMap::new())
        .unwrap_err();

    assert!(matches!(err, PromptError::RenderError(_)));
}

#[test]
fn minijinja_filters_work() {
    let mut renderer = PromptRenderer::new();
    renderer
        .add_template("filters", "{{ name | default('anon') }}: {{ items | join(', ') }}")
        .unwrap();

    let vars = HashMap::from([("items".to_string(), json!(["a", "b", "c"]))]);
    let rendered = renderer.render("filters", &vars).unwrap();

    assert_eq!(rendered, "anon: a, b, c");
}

#[test]
fn render_normalizes_crlf_line_endings() {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("lines", "first\r\nsecond\r\n{{ value }}").unwrap();

    let vars = HashMap::from([("value".to_string(), json!("third"))]);
    let rendered = renderer.render("lines", &vars).unwrap();

    assert_eq!(rendered, "first\nsecond\nthird");
}
