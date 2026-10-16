use super::*;

#[test]
fn generation_usage_converts_inclusive_provider_counts_to_exclusive_buckets() {
    let usage = LlmTokenUsage {
        input_tokens: 17903,
        cached_input_tokens: Some(17817),
        output_tokens: 188,
        reasoning_tokens: Some(120),
        total_tokens: 18091,
    };

    assert_eq!(
        generation_usage(&usage),
        GenerationUsage {
            input: 86,
            input_cached_tokens: 17817,
            output: 68,
            output_reasoning_tokens: 120,
            total: 18091,
        }
    );
}

#[test]
fn generation_usage_bounds_invalid_detail_counts() {
    let usage = LlmTokenUsage {
        input_tokens: 10,
        cached_input_tokens: Some(15),
        output_tokens: 5,
        reasoning_tokens: Some(8),
        total_tokens: 15,
    };

    assert_eq!(
        generation_usage(&usage),
        GenerationUsage {
            input: 0,
            input_cached_tokens: 10,
            output: 0,
            output_reasoning_tokens: 5,
            total: 15,
        }
    );
}
