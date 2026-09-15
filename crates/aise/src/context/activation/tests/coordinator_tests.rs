use super::*;

#[test]
fn provider_span_contract_can_be_constructed_without_registration() {
    let story_id = StoryId::try_new("story-provider-span").unwrap();
    let turn_number = TurnNumber::try_new(1).unwrap();
    let _span = activation_provider_span(&story_id, turn_number, "provider", 1);
}
