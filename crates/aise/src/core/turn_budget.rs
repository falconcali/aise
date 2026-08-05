use crate::config::TurnConfig;
use crate::error::AiseError;

#[derive(Debug, Clone)]
pub struct TurnBudget {
    limits: TurnBudgetLimits,
    usage: TurnBudgetUsage,
}

#[derive(Debug, Clone)]
pub struct TurnBudgetLimits {
    pub max_repair_rounds: u32,
    pub max_llm_calls: u32,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_retrieved_items: usize,
    pub max_context_tokens: u64,
    pub max_character_thoughts: usize,
    pub max_validation_issues: usize,
    pub max_trace_spans: usize,
}

impl Default for TurnBudgetLimits {
    fn default() -> Self {
        Self {
            max_repair_rounds: 3,
            max_llm_calls: 8,
            max_input_tokens: 8_192,
            max_output_tokens: 2_048,
            max_total_tokens: 10_240,
            max_retrieved_items: 5,
            max_context_tokens: 8_192,
            max_character_thoughts: 8,
            max_validation_issues: 32,
            max_trace_spans: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TurnBudgetUsage {
    llm_calls: u32,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    repair_rounds: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LlmReservation {
    requested_output: u64,
}

impl LlmReservation {
    pub fn max_output_tokens(&self) -> u64 {
        self.requested_output
    }
}

impl TurnBudget {
    pub fn from_config(config: &TurnConfig) -> Result<Self, AiseError> {
        config.validate()?;
        Ok(Self {
            limits: TurnBudgetLimits {
                max_repair_rounds: config.max_repair_rounds,
                max_llm_calls: config.max_llm_calls,
                max_input_tokens: config.max_input_tokens,
                max_output_tokens: config.max_output_tokens,
                max_total_tokens: config.max_total_tokens,
                max_retrieved_items: config.max_retrieved_items,
                max_context_tokens: config.max_context_tokens,
                max_character_thoughts: config.max_character_thoughts,
                max_validation_issues: config.max_validation_issues,
                max_trace_spans: config.max_trace_spans,
            },
            usage: TurnBudgetUsage::default(),
        })
    }

    pub fn new(limits: TurnBudgetLimits) -> Self {
        Self {
            limits,
            usage: TurnBudgetUsage::default(),
        }
    }

    pub fn max_repair_rounds(&self) -> u32 {
        self.limits.max_repair_rounds
    }

    pub fn max_retrieved_items(&self) -> usize {
        self.limits.max_retrieved_items
    }

    pub fn max_context_tokens(&self) -> u64 {
        self.limits.max_context_tokens
    }

    pub fn max_character_thoughts(&self) -> usize {
        self.limits.max_character_thoughts
    }

    pub fn max_validation_issues(&self) -> usize {
        self.limits.max_validation_issues
    }

    pub fn max_trace_spans(&self) -> usize {
        self.limits.max_trace_spans
    }

    pub fn remaining_output_tokens(&self) -> u64 {
        self.limits.max_output_tokens.saturating_sub(self.usage.output_tokens)
    }

    pub fn llm_calls(&self) -> u32 {
        self.usage.llm_calls
    }

    pub fn repair_rounds(&self) -> u32 {
        self.usage.repair_rounds
    }

    pub fn consume_repair_round(&mut self) -> Result<(), AiseError> {
        if self.usage.repair_rounds >= self.limits.max_repair_rounds {
            return Err(AiseError::ValidationBudgetExhausted(self.usage.repair_rounds));
        }
        self.usage.repair_rounds += 1;
        Ok(())
    }

    pub fn input_tokens(&self) -> u64 {
        self.usage.input_tokens
    }

    pub fn output_tokens(&self) -> u64 {
        self.usage.output_tokens
    }

    pub fn total_tokens(&self) -> u64 {
        self.usage.total_tokens
    }

    pub fn reserve_llm_call(
        &mut self,
        estimated_input: u64,
        requested_output: u64,
    ) -> Result<LlmReservation, AiseError> {
        if self.usage.llm_calls >= self.limits.max_llm_calls {
            return Err(AiseError::TokenBudgetExceeded(format!(
                "llm call limit {} reached",
                self.limits.max_llm_calls
            )));
        }
        let projected_input = self.usage.input_tokens.saturating_add(estimated_input);
        let projected_output = self.usage.output_tokens.saturating_add(requested_output);
        let projected_total = self
            .usage
            .total_tokens
            .saturating_add(estimated_input)
            .saturating_add(requested_output);
        if projected_input > self.limits.max_input_tokens {
            return Err(AiseError::TokenBudgetExceeded("input token limit".into()));
        }
        if projected_output > self.limits.max_output_tokens {
            return Err(AiseError::TokenBudgetExceeded("output token limit".into()));
        }
        if projected_total > self.limits.max_total_tokens {
            return Err(AiseError::TokenBudgetExceeded("total token limit".into()));
        }
        Ok(LlmReservation { requested_output })
    }

    pub fn settle_llm_call(&mut self, actual_input: u64, actual_output: u64) -> Result<(), AiseError> {
        self.usage.llm_calls += 1;
        self.usage.input_tokens = self.usage.input_tokens.saturating_add(actual_input);
        self.usage.output_tokens = self.usage.output_tokens.saturating_add(actual_output);
        self.usage.total_tokens = self
            .usage
            .total_tokens
            .saturating_add(actual_input)
            .saturating_add(actual_output);
        if self.usage.input_tokens > self.limits.max_input_tokens {
            return Err(AiseError::TokenBudgetExceeded("input token limit exceeded".into()));
        }
        if self.usage.output_tokens > self.limits.max_output_tokens {
            return Err(AiseError::TokenBudgetExceeded("output token limit exceeded".into()));
        }
        if self.usage.total_tokens > self.limits.max_total_tokens {
            return Err(AiseError::TokenBudgetExceeded("total token limit exceeded".into()));
        }
        Ok(())
    }
}
