#[derive(Debug, Clone)]
pub struct TurnBudget {
    pub max_repair_rounds: u32,
    pub max_tokens: u32,
    pub max_retrieved_items: usize,
}

impl Default for TurnBudget {
    fn default() -> Self {
        Self {
            max_repair_rounds: 3,
            max_tokens: 2048,
            max_retrieved_items: 20,
        }
    }
}
