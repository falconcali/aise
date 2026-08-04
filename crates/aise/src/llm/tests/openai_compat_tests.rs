use super::*;

fn provider_with(base_url: &str) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(LlmConfig {
        base_url: base_url.into(),
        api_key: None,
        model: "test".into(),
        max_concurrent: 1,
        temperature: 0.0,
        ..LlmConfig::default()
    })
}

#[test]
fn endpoint_appends_chat_completions_to_plain_base() {
    let provider = provider_with("https://api.deepseek.com");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/chat/completions");
}

#[test]
fn endpoint_appends_to_v1_base() {
    let provider = provider_with("https://api.deepseek.com/v1");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/v1/chat/completions");
}

#[test]
fn endpoint_keeps_existing_chat_completions_path() {
    let provider = provider_with("https://api.deepseek.com/chat/completions");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/chat/completions");
}
