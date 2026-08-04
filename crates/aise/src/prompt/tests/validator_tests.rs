use super::*;
use crate::prompt::model::PromptKind;
use crate::prompt::slot::VarSpec;
use serde_json::{Value, json};

fn make_slot(vars: Vec<VarSpec>) -> SlotSpec {
    SlotSpec {
        slot_id: "test.slot".into(),
        allowed_kinds: vec![PromptKind::Text],
        required: true,
        structured_output: false,
        output_contract_required: false,
        optimizable: false,
        allow_child_render: false,
        notes: None,
        vars,
        output_contract: None,
    }
}

fn make_contract() -> OutputContract {
    OutputContract {
        min_length: None,
        max_length: None,
        must_contain: Vec::new(),
        must_not_contain: Vec::new(),
    }
}

#[test]
fn valid_vars_pass() {
    let slot = make_slot(vec![
        VarSpec {
            name: "title".to_string(),
            var_type: VarType::String,
            required: true,
        },
        VarSpec {
            name: "count".to_string(),
            var_type: VarType::Number,
            required: true,
        },
        VarSpec {
            name: "active".to_string(),
            var_type: VarType::Bool,
            required: false,
        },
    ]);
    let vars = HashMap::from([
        ("title".to_string(), json!("Hello")),
        ("count".to_string(), json!(42)),
        ("active".to_string(), json!(true)),
    ]);

    assert!(validate_input_vars(&slot, &vars).is_ok());
}

#[test]
fn missing_required_var_fails() {
    let slot = make_slot(vec![VarSpec {
        name: "title".to_string(),
        var_type: VarType::String,
        required: true,
    }]);

    let err = validate_input_vars(&slot, &HashMap::new()).unwrap_err();

    assert!(err.to_string().contains("required variable `title` is missing"));
}

#[test]
fn wrong_type_fails() {
    let slot = make_slot(vec![VarSpec {
        name: "title".to_string(),
        var_type: VarType::String,
        required: true,
    }]);
    let vars = HashMap::from([("title".to_string(), json!(123))]);

    let err = validate_input_vars(&slot, &vars).unwrap_err();

    assert!(err.to_string().contains("expected type String"));
}

#[test]
fn extra_vars_are_ignored() {
    let slot = make_slot(vec![VarSpec {
        name: "title".to_string(),
        var_type: VarType::String,
        required: true,
    }]);
    let vars = HashMap::from([
        ("title".to_string(), json!("Hello")),
        ("extra".to_string(), json!("ignored")),
    ]);

    assert!(validate_input_vars(&slot, &vars).is_ok());
}

#[test]
fn null_required_var_fails() {
    let slot = make_slot(vec![VarSpec {
        name: "title".to_string(),
        var_type: VarType::String,
        required: true,
    }]);
    let vars = HashMap::from([("title".to_string(), Value::Null)]);

    let err = validate_input_vars(&slot, &vars).unwrap_err();

    assert!(err.to_string().contains("required variable `title` is null"));
}

#[test]
fn optional_var_can_be_absent_or_null() {
    let slot = make_slot(vec![VarSpec {
        name: "context".to_string(),
        var_type: VarType::String,
        required: false,
    }]);
    let absent = HashMap::new();
    let null_vars = HashMap::from([("context".to_string(), Value::Null)]);

    assert!(validate_input_vars(&slot, &absent).is_ok());
    assert!(validate_input_vars(&slot, &null_vars).is_ok());
}

#[test]
fn array_and_object_types_validate() {
    let slot = make_slot(vec![
        VarSpec {
            name: "items".to_string(),
            var_type: VarType::Array,
            required: true,
        },
        VarSpec {
            name: "data".to_string(),
            var_type: VarType::Object,
            required: true,
        },
    ]);
    let valid = HashMap::from([
        ("items".to_string(), json!(["a", "b"])),
        ("data".to_string(), json!({"key": "value"})),
    ]);
    let invalid = HashMap::from([
        ("items".to_string(), json!("not an array")),
        ("data".to_string(), json!("not an object")),
    ]);

    assert!(validate_input_vars(&slot, &valid).is_ok());
    assert!(validate_input_vars(&slot, &invalid).is_err());
}

#[test]
fn any_type_accepts_non_null_values() {
    let slot = make_slot(vec![VarSpec {
        name: "value".to_string(),
        var_type: VarType::Any,
        required: true,
    }]);

    for value in [json!("text"), json!(1), json!(true), json!([1]), json!({"a": 1})] {
        let vars = HashMap::from([("value".to_string(), value)]);
        assert!(validate_input_vars(&slot, &vars).is_ok());
    }
}

#[test]
fn output_contract_min_length_fails() {
    let mut contract = make_contract();
    contract.min_length = Some(10);

    let err = validate_output_contract("test.slot", &contract, "short").unwrap_err();

    assert!(err.to_string().contains("below minimum"));
}

#[test]
fn output_contract_max_length_fails() {
    let mut contract = make_contract();
    contract.max_length = Some(5);

    let err = validate_output_contract("test.slot", &contract, "too long").unwrap_err();

    assert!(err.to_string().contains("exceeds maximum"));
}

#[test]
fn output_contract_must_contain_fails() {
    let mut contract = make_contract();
    contract.must_contain = vec!["needle".to_string()];

    let err = validate_output_contract("test.slot", &contract, "haystack").unwrap_err();

    assert!(err.to_string().contains("must contain"));
}

#[test]
fn output_contract_must_not_contain_fails() {
    let mut contract = make_contract();
    contract.must_not_contain = vec!["forbidden".to_string()];

    let err = validate_output_contract("test.slot", &contract, "contains forbidden").unwrap_err();

    assert!(err.to_string().contains("must not contain"));
}

#[test]
fn permissive_output_contract_passes() {
    let contract = make_contract();

    assert!(validate_output_contract("test.slot", &contract, "").is_ok());
    assert!(validate_output_contract("test.slot", &contract, "anything").is_ok());
}
