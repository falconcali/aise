use aise::config::{RetrievalConfig, TurnConfig, TurnContentLimitsConfig};
use aise::core::turn_budget::TurnBudget;

#[test]
fn turn_budget_from_config_matches_retrieval_limits() {
    let retrieval = RetrievalConfig {
        max_total_items: 20,
        max_total_tokens: 2_000,
        max_items_per_audience: 5,
        max_tokens_per_audience: 500,
        ..RetrievalConfig::default()
    };
    let budget =
        TurnBudget::from_config(&TurnConfig::default(), &TurnContentLimitsConfig::default(), &retrieval).unwrap();
    assert_eq!(budget.max_total_items(), 20);
    assert_eq!(budget.max_retrieved_tokens(), 2_000);
    assert_eq!(budget.max_items_per_audience(), 5);
}
