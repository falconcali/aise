use crate::config::{RetrievalConfig, TurnConfig, TurnContentLimitsConfig};
use crate::turn::turn_contract::{LlmBudgetReservation, LlmCallId, LlmCallUsage};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

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
    pub max_candidates_per_retriever: usize,
    pub max_candidates_total: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_retrieved_tokens: u64,
    pub max_item_bytes: usize,
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
    pub fn from(turn: &TurnConfig, content: &TurnContentLimitsConfig, retrieval: &RetrievalConfig) -> Self {
        Self {
            max_repair_rounds: turn.max_repair_rounds,
            max_llm_calls: turn.max_llm_calls,
            max_input_tokens: turn.max_input_tokens,
            max_output_tokens: turn.max_output_tokens,
            max_total_tokens: turn.max_total_tokens,
            max_candidates_per_retriever: retrieval.max_candidates_per_retriever,
            max_candidates_total: retrieval.max_candidates_total,
            max_items_per_audience: retrieval.max_items_per_audience,
            max_tokens_per_audience: retrieval.max_tokens_per_audience,
            max_total_items: retrieval.max_total_items,
            max_retrieved_tokens: retrieval.max_total_tokens,
            max_item_bytes: retrieval.max_item_bytes,
            max_context_tokens: turn.max_context_tokens,
            max_character_thoughts: turn.max_character_thoughts,
            max_character_thought_bytes: content.max_character_thought_bytes,
            max_plan_bytes: content.max_plan_bytes,
            max_proposal_bytes: content.max_proposal_bytes,
            max_validation_issues: turn.max_validation_issues,
            max_validation_issue_bytes: content.max_validation_issue_bytes,
            max_trace_spans: turn.max_trace_spans,
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
    pub fn from_config(
        turn: &TurnConfig,
        content: &TurnContentLimitsConfig,
        retrieval: &RetrievalConfig,
    ) -> Result<Self, TurnExecutionError> {
        turn.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        content.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        retrieval.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        Ok(Self {
            limits: TurnBudgetLimits::from(turn, content, retrieval),
            usage: TurnBudgetUsage::default(),
        })
    }

    pub fn max_repair_rounds(&self) -> u32 {
        self.limits.max_repair_rounds
    }

    pub fn max_total_items(&self) -> usize {
        self.limits.max_total_items
    }

    pub fn max_items_per_audience(&self) -> usize {
        self.limits.max_items_per_audience
    }

    pub fn max_tokens_per_audience(&self) -> u64 {
        self.limits.max_tokens_per_audience
    }

    pub fn max_candidates_per_retriever(&self) -> usize {
        self.limits.max_candidates_per_retriever
    }

    pub fn max_candidates_total(&self) -> usize {
        self.limits.max_candidates_total
    }

    pub fn max_item_bytes(&self) -> usize {
        self.limits.max_item_bytes
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
