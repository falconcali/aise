use super::*;

#[test]
fn output_rejects_unknown_fields() {
    let raw = serde_json::json!({"story_text": "hello", "extra": true});
    let result: Result<StoryGeneratorOutput, _> = serde_json::from_value(raw);
    assert!(result.is_err());
}

#[test]
fn output_round_trips_through_json() {
    let output = StoryGeneratorOutput {
        story_text: BoundedText::try_new("hello world", "story_text", 4096).unwrap(),
    };
    let raw = serde_json::to_value(&output).unwrap();
    let decoded: StoryGeneratorOutput = serde_json::from_value(raw).unwrap();
    assert_eq!(decoded, output);
}
