use super::*;
use serde_json::json;

#[test]
fn missing_rule_fields_use_enabled_defaults() {
    let rule: ActivationRule = serde_json::from_value(json!({})).expect("empty rule should deserialize");

    assert!(rule.mode.enabled);
    assert!(!rule.mode.constant);
    assert!(rule.match_rule.keys.is_empty());
    assert_eq!(rule.match_rule.secondary_logic, SecondaryLogic::AndAny);
    assert_eq!(rule.selection.probability, ActivationProbability::ALWAYS);
    assert_eq!(rule.selection.group_weight, 100);
}

#[test]
fn grouped_rule_deserializes() {
    let rule: ActivationRule = serde_json::from_value(json!({
        "match": { "keys": [" 白蛇 "], "secondary_keys": ["白素贞"], "secondary_logic": "not_any" },
        "recursion": { "delay_until_recursion": 2 },
        "selection": { "order": 10, "probability": 40, "groups": ["bai.identity"] }
    }))
    .expect("rule should deserialize");

    assert_eq!(rule.match_rule.keys[0].as_str(), "白蛇");
    assert_eq!(rule.match_rule.secondary_logic, SecondaryLogic::NotAny);
    assert_eq!(rule.recursion.delay_until_recursion.map(|level| level.get()), Some(2));
    assert_eq!(rule.selection.probability.percent(), 40);
    assert_eq!(rule.selection.groups[0].as_str(), "bai.identity");
}

#[test]
fn invalid_rule_values_are_rejected() {
    for value in [
        json!({ "match": { "keys": ["   "] } }),
        json!({ "match": { "keys": ["x".repeat(ActivationKey::MAX_BYTES + 1)] } }),
        json!({ "selection": { "probability": 101 } }),
        json!({ "selection": { "groups": ["Bai Identity"] } }),
        json!({ "selection": { "groups": ["bai."] } }),
        json!({ "recursion": { "delay_until_recursion": 0 } }),
        json!({ "timing": { "sticky_turns": 1 } }),
    ] {
        assert!(
            serde_json::from_value::<ActivationRule>(value.clone()).is_err(),
            "{value} should be rejected"
        );
    }
}

#[test]
fn secondary_logic_follows_world_info_semantics() {
    assert!(SecondaryLogic::AndAny.holds(1, 3));
    assert!(!SecondaryLogic::AndAny.holds(0, 3));
    assert!(SecondaryLogic::AndAll.holds(3, 3));
    assert!(!SecondaryLogic::AndAll.holds(2, 3));
    assert!(SecondaryLogic::NotAny.holds(0, 3));
    assert!(!SecondaryLogic::NotAny.holds(1, 3));
    assert!(SecondaryLogic::NotAll.holds(2, 3));
    assert!(!SecondaryLogic::NotAll.holds(3, 3));
    assert!(SecondaryLogic::NotAny.holds(0, 0));
}
