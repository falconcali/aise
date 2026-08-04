#[derive(Debug, Clone)]
pub struct TurnBudget {
    max_repair_rounds: u32,
    max_tokens: u32,
    max_retrieved_items: usize,
}

impl TurnBudget {
    pub fn new(max_repair_rounds: u32, max_tokens: u32, max_retrieved_items: usize) -> Self {
        Self {
            max_repair_rounds,
            max_tokens,
            max_retrieved_items,
        }
    }

    pub fn max_repair_rounds(&self) -> u32 {
        self.max_repair_rounds
    }

    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    pub fn max_retrieved_items(&self) -> usize {
        self.max_retrieved_items
    }
}
