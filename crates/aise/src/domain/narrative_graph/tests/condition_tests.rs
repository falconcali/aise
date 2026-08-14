use crate::domain::asset::ids::{AttributeKey, FactKey, NarrativeConditionKey, NarrativeNodeKey, StoryRoleKey};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::CharacterId;
use crate::domain::narrative_graph::condition::{
    ConditionEvalContext, NarrativeCondition, NarrativeNodeState, NarrativeTruthValue, RoleControllerKind,
    SemanticNarrativeCondition, evaluate_condition,
};
use crate::domain::narrative_graph::definition::NarrativeLimits;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::narrative_graph::state_view::{NarrativeStateView, NarrativeStateViewError};
use std::collections::BTreeMap;

struct StubStateView {
    facts: BTreeMap<FactKey, ScalarValue>,
    attributes: BTreeMap<(StoryRoleKey, AttributeKey), ScalarValue>,
    trust: BTreeMap<(StoryRoleKey, StoryRoleKey), i16>,
    controllers: BTreeMap<StoryRoleKey, RoleControllerKind>,
}

impl StubStateView {
    fn empty() -> Self {
        Self {
            facts: BTreeMap::new(),
            attributes: BTreeMap::new(),
            trust: BTreeMap::new(),
            controllers: BTreeMap::new(),
        }
    }
}

impl NarrativeStateView for StubStateView {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(self.facts.get(fact_key))
    }

    fn character_attribute(
        &self,
        role_key: &StoryRoleKey,
        attribute: &AttributeKey,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(self.attributes.get(&(role_key.clone(), attribute.clone())))
    }

    fn relationship_trust(
        &self,
        source_role_key: &StoryRoleKey,
        target_role_key: &StoryRoleKey,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        Ok(self.trust.get(&(source_role_key.clone(), target_role_key.clone())).copied())
    }

    fn role_controller(&self, role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError> {
        self.controllers
            .get(role_key)
            .copied()
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_key: role_key.as_str().to_owned(),
            })
    }

    fn character_id_for_role(&self, role_key: &StoryRoleKey) -> Result<CharacterId, NarrativeStateViewError> {
        Err(NarrativeStateViewError::UnknownRole {
            role_key: role_key.as_str().to_owned(),
        })
    }
}

fn limits() -> NarrativeLimits {
    NarrativeLimits {
        max_graph_nodes: 64,
        max_graph_edges: 64,
        max_condition_depth: 8,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
        max_semantic_conditions: 16,
        max_semantic_criterion_bytes: 1024,
        max_frontier_nodes: 16,
        max_semantic_queries_per_turn: 16,
        max_semantic_query_bytes: 1024,
        max_evidence_bytes: 256,
        max_result_reason_bytes: 256,
        max_transitions_per_turn: 16,
        max_pending_effects: 16,
    }
}

fn eval(
    condition: &NarrativeCondition,
    semantic: &BTreeMap<NarrativeConditionKey, NarrativeTruthValue>,
) -> NarrativeTruthValue {
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView::empty();
    let ctx = ConditionEvalContext {
        state: &state,
        view: &view,
        semantic_results: semantic,
        current_turn: 0,
        limits: limits(),
    };
    evaluate_condition(condition, &ctx, 0).expect("condition should evaluate")
}

#[test]
fn story_started_is_always_satisfied() {
    assert_eq!(
        eval(&NarrativeCondition::StoryStarted, &BTreeMap::new()),
        NarrativeTruthValue::Satisfied
    );
}

#[test]
fn turn_reaches_compares_current_turn() {
    let condition = NarrativeCondition::TurnReaches { turn: 5 };
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView::empty();
    let semantic = BTreeMap::new();
    let ctx = ConditionEvalContext {
        state: &state,
        view: &view,
        semantic_results: &semantic,
        current_turn: 5,
        limits: limits(),
    };
    assert_eq!(evaluate_condition(&condition, &ctx, 0).unwrap(), NarrativeTruthValue::Satisfied);
    let ctx_before = ConditionEvalContext { current_turn: 4, ..ctx };
    assert_eq!(
        evaluate_condition(&condition, &ctx_before, 0).unwrap(),
        NarrativeTruthValue::Unsatisfied
    );
}

#[test]
fn semantic_condition_defaults_to_unknown_when_missing() {
    let key = NarrativeConditionKey::try_new("condition.example").unwrap();
    let condition = NarrativeCondition::Semantic {
        semantic: SemanticNarrativeCondition {
            condition_key: key,
            criterion: crate::domain::asset::validation::BoundedText::try_new("is it true?", "criterion", 256).unwrap(),
        },
    };
    assert_eq!(eval(&condition, &BTreeMap::new()), NarrativeTruthValue::Unknown);
}

#[test]
fn all_short_circuits_on_unsatisfied_before_unknown() {
    let key = NarrativeConditionKey::try_new("condition.unknown_leaf").unwrap();
    let condition = NarrativeCondition::All {
        conditions: vec![
            NarrativeCondition::TurnReaches { turn: 100 },
            NarrativeCondition::Semantic {
                semantic: SemanticNarrativeCondition {
                    condition_key: key,
                    criterion: crate::domain::asset::validation::BoundedText::try_new("?", "criterion", 256).unwrap(),
                },
            },
        ],
    };
    assert_eq!(eval(&condition, &BTreeMap::new()), NarrativeTruthValue::Unsatisfied);
}

#[test]
fn any_is_satisfied_if_one_branch_satisfied_even_with_unknown_sibling() {
    let key = NarrativeConditionKey::try_new("condition.unknown_leaf").unwrap();
    let condition = NarrativeCondition::Any {
        conditions: vec![
            NarrativeCondition::StoryStarted,
            NarrativeCondition::Semantic {
                semantic: SemanticNarrativeCondition {
                    condition_key: key,
                    criterion: crate::domain::asset::validation::BoundedText::try_new("?", "criterion", 256).unwrap(),
                },
            },
        ],
    };
    assert_eq!(eval(&condition, &BTreeMap::new()), NarrativeTruthValue::Satisfied);
}

#[test]
fn not_inverts_satisfied_and_unsatisfied_but_not_unknown() {
    assert_eq!(
        eval(
            &NarrativeCondition::Not {
                condition: Box::new(NarrativeCondition::StoryStarted)
            },
            &BTreeMap::new()
        ),
        NarrativeTruthValue::Unsatisfied
    );
    let key = NarrativeConditionKey::try_new("condition.unknown_leaf").unwrap();
    let unknown = NarrativeCondition::Semantic {
        semantic: SemanticNarrativeCondition {
            condition_key: key,
            criterion: crate::domain::asset::validation::BoundedText::try_new("?", "criterion", 256).unwrap(),
        },
    };
    assert_eq!(
        eval(
            &NarrativeCondition::Not {
                condition: Box::new(unknown)
            },
            &BTreeMap::new()
        ),
        NarrativeTruthValue::Unknown
    );
}

#[test]
fn node_state_condition_reads_runtime_state() {
    let node_key = NarrativeNodeKey::try_new("node.intro").unwrap();
    let mut state = NarrativeRuntimeState::initial();
    state.node_states.insert(node_key.clone(), NarrativeNodeState::Active);
    let view = StubStateView::empty();
    let semantic = BTreeMap::new();
    let ctx = ConditionEvalContext {
        state: &state,
        view: &view,
        semantic_results: &semantic,
        current_turn: 0,
        limits: limits(),
    };
    let condition = NarrativeCondition::NodeState {
        node_key,
        state: NarrativeNodeState::Active,
    };
    assert_eq!(evaluate_condition(&condition, &ctx, 0).unwrap(), NarrativeTruthValue::Satisfied);
}

#[test]
fn condition_depth_limit_is_enforced() {
    let mut condition = NarrativeCondition::StoryStarted;
    for _ in 0..20 {
        condition = NarrativeCondition::Not {
            condition: Box::new(condition),
        };
    }
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView::empty();
    let semantic = BTreeMap::new();
    let ctx = ConditionEvalContext {
        state: &state,
        view: &view,
        semantic_results: &semantic,
        current_turn: 0,
        limits: limits(),
    };
    assert!(evaluate_condition(&condition, &ctx, 0).is_err());
}
