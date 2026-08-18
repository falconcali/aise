use crate::domain::ids::{RoleId, allocate_dynamic_role_candidates};
use crate::domain::story_instance::state::CastPolicy;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{
    ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationLocation, ValidationRemedy,
};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct DomainInvariantValidator;

impl DeterministicValidator for DomainInvariantValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let (Some(extraction), Some(snapshot)) = (ctx.extraction(), ctx.snapshot()) else {
            return Ok(Vec::new());
        };
        let dto = extraction;
        let mut issues = Vec::new();

        if !dto.cast_policy_violations.is_empty() {
            issues.push(cast_policy_issue(
                "cast_policy_violations",
                "extractor reported a material cast policy violation that requires narrative repair",
            ));
        }
        let cast_policy = snapshot.instance_settings().cast_policy;
        if cast_policy != CastPolicy::Open && !dto.new_roles.is_empty() {
            issues.push(cast_policy_issue(
                "new_roles",
                "new roles are not permitted under the configured cast policy",
            ));
        }

        if !dto.new_roles.is_empty() {
            match allocate_dynamic_role_candidates(snapshot.role_id_high_water(), dto.new_roles.len()) {
                Ok(pool) => {
                    for (index, role) in dto.new_roles.iter().enumerate() {
                        let matches_candidate = RoleId::try_new(role.role_id.clone())
                            .ok()
                            .and_then(|role_id| pool.position_of(&role_id))
                            == Some(index);
                        if !matches_candidate {
                            issues.push(new_role_issue(
                                index,
                                "role_id must be the next unused dynamic candidate in rendered order",
                            ));
                        }
                    }
                }
                Err(_) => issues.push(new_role_issue(0, "dynamic role id allocation overflowed")),
            }
        }

        for (index, relationship) in dto.relationship_states.iter().enumerate() {
            if i16::try_from(relationship.trust).is_err() {
                issues.push(trust_issue(index));
            }
        }

        Ok(issues)
    }
}

fn cast_policy_issue(path: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::CastPolicyViolation,
        class: ValidationIssueClass::Story,
        remedy: ValidationRemedy::RepairStory,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: path.to_owned(),
            item_index: None,
        }),
    }
}

fn new_role_issue(index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::NewRoleInvalid,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: format!("new_roles[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}

fn trust_issue(index: usize) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::DomainInvariantViolated,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: "relationship trust is outside the domain's accepted range".to_owned(),
        location: Some(ValidationLocation {
            path: format!("relationship_states[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}

#[cfg(test)]
#[path = "tests/domain_invariant_tests.rs"]
mod tests;
