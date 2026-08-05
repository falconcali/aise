use crate::config::LlmConfig;
use crate::llm::message::ChatMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageAccuracy {
    Exact,
    Estimated,
}

impl UsageAccuracy {
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageAccuracy::Exact => "exact",
            UsageAccuracy::Estimated => "estimated",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: u64,
    pub accuracy: UsageAccuracy,
}

#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Other(String),
}

impl FinishReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ContentFilter => "content_filter",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmCharge {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub amount_minor: i64,
    pub price_version: String,
}

#[derive(Debug, Clone)]
pub struct LlmCompletion {
    pub text: String,
    pub finish_reason: Option<FinishReason>,
    pub reasoning_content: Option<String>,
    pub usage: Option<LlmTokenUsage>,
    pub charge: Option<LlmCharge>,
}

pub struct TokenAccountant {
    provider: &'static str,
    model: String,
    price_version: String,
    input_price_per_1k: Option<i64>,
    cached_input_price_per_1k: Option<i64>,
    output_price_per_1k: Option<i64>,
}

impl TokenAccountant {
    pub fn new(config: &LlmConfig, provider: &'static str) -> Self {
        Self {
            provider,
            model: config.model.clone(),
            price_version: "v1".into(),
            input_price_per_1k: config.price_input_per_1k_tokens,
            cached_input_price_per_1k: config.price_cached_input_per_1k_tokens,
            output_price_per_1k: config.price_output_per_1k_tokens,
        }
    }

    pub fn estimate_input_tokens(messages: &[ChatMessage]) -> u64 {
        messages
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .fold(0u64, u64::saturating_add)
    }

    pub fn estimate_output_tokens(text: &str) -> u64 {
        estimate_tokens(text)
    }

    pub fn charge(&self, usage: &LlmTokenUsage) -> Option<LlmCharge> {
        let input_price = self.input_price_per_1k?;
        let output_price = self.output_price_per_1k?;
        let cached_input = usage.cached_input_tokens.unwrap_or(0);
        let non_cached_input = usage.input_tokens.saturating_sub(cached_input);
        let input_cost = price_for_tokens(non_cached_input, input_price);
        let cached_cost = match self.cached_input_price_per_1k {
            Some(price) => price_for_tokens(cached_input, price),
            None => 0,
        };
        let output_cost = price_for_tokens(usage.output_tokens, output_price);
        Some(LlmCharge {
            provider: self.provider.to_owned(),
            model: self.model.clone(),
            input_tokens: usage.input_tokens,
            cached_input_tokens: cached_input,
            output_tokens: usage.output_tokens,
            amount_minor: input_cost.saturating_add(cached_cost).saturating_add(output_cost),
            price_version: self.price_version.clone(),
        })
    }
}

pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64)
        .saturating_add(3)
        .checked_div(4)
        .unwrap_or(0)
        .max(1)
}

fn price_for_tokens(tokens: u64, price_per_1k: i64) -> i64 {
    let price = i128::from(price_per_1k);
    let tokens = i128::from(tokens);
    let amount = tokens.saturating_mul(price).checked_div(1_000).unwrap_or(0);
    amount as i64
}
