use crate::prompt::{
    error::PromptError,
    slot::{OutputContract, SlotSpec, VarType},
};
use serde_json::Value;
use std::collections::HashMap;

pub fn validate_input_vars(slot_spec: &SlotSpec, vars: &HashMap<String, Value>) -> Result<(), PromptError> {
    if slot_spec.vars.is_empty() {
        return Ok(());
    }

    for var_spec in &slot_spec.vars {
        let value = vars.get(&var_spec.name);

        if var_spec.required {
            match value {
                None => {
                    return Err(PromptError::SchemaValidationFailed(format!(
                        "required variable `{}` is missing",
                        var_spec.name
                    )));
                }
                Some(Value::Null) => {
                    return Err(PromptError::SchemaValidationFailed(format!(
                        "required variable `{}` is null",
                        var_spec.name
                    )));
                }
                _ => {}
            }
        }

        if let Some(value) = value {
            if !value.is_null() {
                check_var_type(&var_spec.name, &var_spec.var_type, value)?;
            }
        }
    }

    Ok(())
}

fn check_var_type(name: &str, expected: &VarType, value: &Value) -> Result<(), PromptError> {
    let ok = match expected {
        VarType::Any => true,
        VarType::String => value.is_string(),
        VarType::Number => value.is_number(),
        VarType::Bool => value.is_boolean(),
        VarType::Array => value.is_array(),
        VarType::Object => value.is_object(),
    };

    if !ok {
        return Err(PromptError::SchemaValidationFailed(format!(
            "variable `{}` expected type {:?}, got {}",
            name,
            expected,
            json_type_name(value),
        )));
    }

    Ok(())
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn validate_output_contract(slot_id: &str, contract: &OutputContract, rendered: &str) -> Result<(), PromptError> {
    if let Some(min) = contract.min_length {
        if rendered.len() < min {
            return Err(PromptError::OutputContractViolation {
                slot: slot_id.to_string(),
                reason: format!("rendered length {} is below minimum {}", rendered.len(), min),
            });
        }
    }

    if let Some(max) = contract.max_length {
        if rendered.len() > max {
            return Err(PromptError::OutputContractViolation {
                slot: slot_id.to_string(),
                reason: format!("rendered length {} exceeds maximum {}", rendered.len(), max),
            });
        }
    }

    for needle in &contract.must_contain {
        if !rendered.contains(needle.as_str()) {
            return Err(PromptError::OutputContractViolation {
                slot: slot_id.to_string(),
                reason: format!("rendered output must contain \"{}\"", needle),
            });
        }
    }

    for needle in &contract.must_not_contain {
        if rendered.contains(needle.as_str()) {
            return Err(PromptError::OutputContractViolation {
                slot: slot_id.to_string(),
                reason: format!("rendered output must not contain \"{}\"", needle),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/validator_tests.rs"]
mod tests;
