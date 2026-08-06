use crate::config::{TurnConfig, TurnContentLimitsConfig};
use crate::core::turn_contract::{LlmBudgetReservation, LlmCallId, LlmCallUsage};
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};

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
    pub max_retrieval_candidates: usize,
    pub max_retrieved_item_bytes: usize,
    pub max_retrieved_tokens: u64,
    pub max_context_tokens: u64,
    pub max_character_thoughts: usize,
    pub max_character_thought_bytes: usize,
    pub max_plan_bytes: usize,
    pub max_proposal_bytes: usize,
    pub max_validation_issues: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_spans: usize,
}

impl TurnBudgetLimits {
    pub fn from(config: &TurnConfig, content: &TurnContentLimitsConfig) -> Self {
        Self {
            max_repair_rounds: config.max_repair_rounds,
            max_llm_calls: config.max_llm_calls,
            max_input_tokens: config.max_input_tokens,
            max_output_tokens: config.max_output_tokens,
            max_total_tokens: config.max_total_tokens,
            max_retrieved_items: config.max_retrieved_items,
            max_retrieval_candidates: config.max_retrieval_candidates,
            max_retrieved_item_bytes: content.max_retrieved_item_bytes,
            max_retrieved_tokens: content.max_retrieved_tokens,
            max_context_tokens: config.max_context_tokens,
            max_character_thoughts: config.max_character_thoughts,
            max_character_thought_bytes: content.max_character_thought_bytes,
            max_plan_bytes: content.max_plan_bytes,
            max_proposal_bytes: content.max_proposal_bytes,
            max_validation_issues: config.max_validation_issues,
            max_validation_issue_bytes: content.max_validation_issue_bytes,
            max_trace_spans: config.max_trace_spans,
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

impl TurnBudget {
    pub fn from_config(config: &TurnConfig, content: &TurnContentLimitsConfig) -> Result<Self, TurnExecutionError> {
        config.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        content.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        Ok(Self {
            limits: TurnBudgetLimits::from(config, content),
            usage: TurnBudgetUsage::default(),
        })
    }

    pub fn max_repair_rounds(&self) -> u32 {
        self.limits.max_repair_rounds
    }

    pub fn max_retrieved_items(&self) -> usize {
        self.limits.max_retrieved_items
    }

    pub fn max_retrieval_candidates(&self) -> usize {
        self.limits.max_retrieval_candidates
    }

    pub fn max_retrieved_item_bytes(&self) -> usize {
        self.limits.max_retrieved_item_bytes
    }

    pub fn max_retrieved_tokens(&self) -> u64 {
        self.limits.max_retrieved_tokens
    }

    pub fn max_context_tokens(&self) -> u64 {
        self.limits.max_context_tokens
    }

    pub fn max_character_thoughts(&self) -> usize {
        self.limits.max_character_thoughts
    }

    pub fn max_character_thought_bytes(&self) -> usize {
        self.limits.max_character_thought_bytes
    }

    pub fn max_plan_bytes(&self) -> usize {
        self.limits.max_plan_bytes
    }

    pub fn max_proposal_bytes(&self) -> usize {
        self.limits.max_proposal_bytes
    }

    pub fn max_validation_issues(&self) -> usize {
        self.limits.max_validation_issues
    }

    pub fn max_validation_issue_bytes(&self) -> usize {
        self.limits.max_validation_issue_bytes
    }

    pub fn max_trace_spans(&self) -> usize {
        self.limits.max_trace_spans
    }

    pub fn max_llm_calls(&self) -> u32 {
        self.limits.max_llm_calls
    }

    pub fn max_input_tokens(&self) -> u64 {
        self.limits.max_input_tokens
    }

    pub fn max_output_tokens(&self) -> u64 {
        self.limits.max_output_tokens
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

    pub fn consume_repair_round(&mut self) -> Result<(), TurnExecutionError> {
        if self.usage.repair_rounds >= self.limits.max_repair_rounds {
            return Err(TurnExecutionError::new(
                TurnFailureKind::ValidationBudgetExhausted,
                "validation_budget_exhausted",
                None,
                format!("validation failed after {} repair rounds", self.usage.repair_rounds),
            ));
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

    pub fn reserve_llm(
        &mut self,
        input_tokens: u64,
        maximum_output_tokens: u64,
    ) -> Result<LlmBudgetReservation, TurnExecutionError> {
        if self.usage.llm_calls >= self.limits.max_llm_calls {
            return Err(self.budget_error(format!("llm call limit {} reached", self.limits.max_llm_calls)));
        }
        let projected_input = self.usage.input_tokens.saturating_add(input_tokens);
        let projected_output = self.usage.output_tokens.saturating_add(maximum_output_tokens);
        let projected_total = self
            .usage
            .total_tokens
            .saturating_add(input_tokens)
            .saturating_add(maximum_output_tokens);
        if projected_input > self.limits.max_input_tokens {
            return Err(self.budget_error("input token limit".into()));
        }
        if projected_output > self.limits.max_output_tokens {
            return Err(self.budget_error("output token limit".into()));
        }
        if projected_total > self.limits.max_total_tokens {
            return Err(self.budget_error("total token limit".into()));
        }
        self.usage.input_tokens = projected_input;
        self.usage.output_tokens = projected_output;
        self.usage.total_tokens = projected_total;
        Ok(LlmBudgetReservation::new(LlmCallId::new(), input_tokens, maximum_output_tokens))
    }

    pub fn settle_llm(
        &mut self,
        reservation: LlmBudgetReservation,
        usage: LlmCallUsage,
    ) -> Result<(), TurnExecutionError> {
        let reserved_input = reservation.reserved_input_tokens();
        let reserved_output = reservation.reserved_output_tokens();
        let actual_input = usage.input_tokens;
        let actual_output = usage.output_tokens;
        self.usage.input_tokens = self.usage.input_tokens.saturating_sub(reserved_input);
        self.usage.output_tokens = self.usage.output_tokens.saturating_sub(reserved_output);
        self.usage.total_tokens = self
            .usage
            .total_tokens
            .saturating_sub(reserved_input)
            .saturating_sub(reserved_output);
        let settled_input = self.usage.input_tokens.saturating_add(actual_input);
        let settled_output = self.usage.output_tokens.saturating_add(actual_output);
        let settled_total = self
            .usage
            .total_tokens
            .saturating_add(actual_input)
            .saturating_add(actual_output);
        if settled_input > self.limits.max_input_tokens {
            return Err(self.budget_error("input token limit exceeded".into()));
        }
        if settled_output > self.limits.max_output_tokens {
            return Err(self.budget_error("output token limit exceeded".into()));
        }
        if settled_total > self.limits.max_total_tokens {
            return Err(self.budget_error("total token limit exceeded".into()));
        }
        self.usage.llm_calls += 1;
        self.usage.input_tokens = settled_input;
        self.usage.output_tokens = settled_output;
        self.usage.total_tokens = settled_total;
        Ok(())
    }

    pub fn release_llm(&mut self, reservation: LlmBudgetReservation) {
        let reserved_input = reservation.reserved_input_tokens();
        let reserved_output = reservation.reserved_output_tokens();
        self.usage.input_tokens = self.usage.input_tokens.saturating_sub(reserved_input);
        self.usage.output_tokens = self.usage.output_tokens.saturating_sub(reserved_output);
        self.usage.total_tokens = self
            .usage
            .total_tokens
            .saturating_sub(reserved_input)
            .saturating_sub(reserved_output);
    }

    fn budget_error(&self, message: String) -> TurnExecutionError {
        TurnExecutionError::new(TurnFailureKind::TokenBudgetExceeded, "token_budget_exceeded", None, message)
    }
}
