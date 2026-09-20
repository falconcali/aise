use crate::harness::{ExecuteSpec, constant_rule, entry, execute, fact, fragment, ids, limits, rule, run};
use aise::domain::ids::TurnNumber;
use aise::domain::knowledge::activation::{
    ActivationGroupKey, ActivationPattern, ActivationRejectionReason, ActivationRuleVersion, ActivationStopReason,
    ActivationTimedState, ScanFragmentKind, SecondaryLogic,
};

#[test]
fn scan_depth_expands_to_minimum_and_respects_entry_cap() {
    let mut capped = rule("deep");
    capped.match_rule.scan_depth = Some(2);
    let mut runtime_limits = limits();
    runtime_limits.minimum_activations = 2;
    runtime_limits.initial_scan_depth = 1;
    runtime_limits.max_scan_depth = 4;
    let result = execute(
        ExecuteSpec::new(
            vec![
                entry(fact(1), rule("near"), "near body"),
                entry(fact(2), rule("middle"), "middle body"),
                entry(fact(3), capped, "deep body"),
            ],
            vec![
                fragment(ScanFragmentKind::PlayerContribution, 1, 0, "near"),
                fragment(ScanFragmentKind::RecentStory, 2, 1, "middle"),
                fragment(ScanFragmentKind::RecentStory, 3, 2, "deep"),
            ],
            4,
        )
        .with_limits(runtime_limits),
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002"]);
    assert_eq!(result.stop_reason, ActivationStopReason::MinimumSatisfied);
    assert_eq!(result.continuation.scan_depth, 2);
}

#[test]
fn recursion_chain_cycle_and_controls_are_bounded() {
    let chain = execute(ExecuteSpec::new(
        vec![
            entry(fact(1), rule("alpha"), "beta"),
            entry(fact(2), rule("beta"), "gamma"),
            entry(fact(3), rule("gamma"), "alpha"),
        ],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&chain.activated), vec!["fact_0001", "fact_0002", "fact_0003"]);
    assert!(chain.continuation.consumed.recursion_steps <= 3);

    let mut prevent = rule("alpha");
    prevent.recursion.prevent_recursion = true;
    let prevented = execute(ExecuteSpec::new(
        vec![entry(fact(1), prevent, "beta"), entry(fact(2), rule("beta"), "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&prevented.activated), vec!["fact_0001"]);

    let mut excluded = rule("beta");
    excluded.recursion.exclude_recursion = true;
    let excluded_result = execute(ExecuteSpec::new(
        vec![entry(fact(1), rule("alpha"), "beta"), entry(fact(2), excluded, "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&excluded_result.activated), vec!["fact_0001"]);
    assert_eq!(
        excluded_result
            .rejection_summary
            .get(&ActivationRejectionReason::RecursionExcluded),
        Some(&1)
    );
}

#[test]
fn recursion_delay_waits_for_requested_level() {
    let mut delayed = rule("beta");
    delayed.recursion.delay_until_recursion = Some(2);
    let result = execute(ExecuteSpec::new(
        vec![
            entry(fact(1), rule("alpha"), "beta gamma"),
            entry(fact(2), delayed, "done"),
            entry(fact(3), rule("gamma"), "beta"),
        ],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002", "fact_0003"]);
    assert_eq!(
        result.rejection_summary.get(&ActivationRejectionReason::RecursionLevelLocked),
        Some(&1)
    );
}

#[test]
fn groups_use_sticky_score_override_order_and_stable_ties() {
    let group = ActivationGroupKey::try_new("exclusive").unwrap();
    let mut lower = rule("alpha");
    lower.selection.groups = vec![group.clone()];
    lower.selection.use_group_scoring = true;
    lower.selection.group_weight = 0;
    let mut higher = rule("alpha beta");
    higher.match_rule.keys = vec![
        ActivationPattern::Literal("alpha".to_owned()),
        ActivationPattern::Literal("beta".to_owned()),
    ];
    higher.selection.groups = vec![group.clone()];
    higher.selection.use_group_scoring = true;
    higher.selection.group_weight = 0;
    let scored = execute(ExecuteSpec::new(
        vec![entry(fact(1), lower.clone(), "one"), entry(fact(2), higher, "two")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha beta")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&scored.activated), vec!["fact_0002"]);

    let sticky_version = ActivationRuleVersion::from_rule(&lower);
    let mut override_rule = lower.clone();
    override_rule.selection.group_override = true;
    override_rule.selection.order = 100;
    let sticky = run(
        vec![entry(fact(1), lower, "one"), entry(fact(2), override_rule, "two")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![ActivationTimedState {
            source_id: fact(1),
            rule_version: sticky_version,
            sticky_through_turn: Some(TurnNumber::try_new(4).unwrap()),
            cooldown_through_turn: None,
        }],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&sticky.activated), vec!["fact_0001"]);
}

#[test]
fn negative_secondary_matches_do_not_increase_group_score() {
    let group = ActivationGroupKey::try_new("exclusive").unwrap();
    let mut preferred = rule("alpha");
    preferred.selection.groups = vec![group.clone()];
    preferred.selection.use_group_scoring = true;
    preferred.selection.group_weight = 0;
    preferred.selection.order = 10;
    let mut negative = rule("alpha");
    negative.match_rule.secondary_logic = SecondaryLogic::NotAll;
    negative.match_rule.secondary_keys = vec![
        ActivationPattern::Literal("x".to_owned()),
        ActivationPattern::Literal("y".to_owned()),
    ];
    negative.selection.groups = vec![group];
    negative.selection.use_group_scoring = true;
    negative.selection.group_weight = 0;
    let result = execute(ExecuteSpec::new(
        vec![entry(fact(1), preferred, "one"), entry(fact(2), negative, "two")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha x")],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001"]);
}

#[test]
fn depth_expansion_counts_only_new_fragment_matches() {
    let mut runtime_limits = limits();
    runtime_limits.minimum_activations = 2;
    runtime_limits.initial_scan_depth = 1;
    runtime_limits.max_scan_depth = 2;
    runtime_limits.max_pattern_matches = 2;
    let result = execute(
        ExecuteSpec::new(
            vec![
                entry(fact(1), rule("alpha"), "one"),
                entry(fact(2), rule("beta"), "two"),
            ],
            vec![
                fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha"),
                fragment(ScanFragmentKind::RecentStory, 2, 1, "beta"),
            ],
            4,
        )
        .with_limits(runtime_limits),
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002"]);
    assert_eq!(result.continuation.consumed.pattern_matches, 2);
}

#[test]
fn delay_suppresses_exact_configured_turn_range() {
    let mut delayed = constant_rule();
    delayed.timing.delay_turns = 3;
    let blocked = execute(ExecuteSpec::new(vec![entry(fact(1), delayed.clone(), "body")], vec![], 3)).unwrap();
    assert!(blocked.activated.is_empty());
    let admitted = execute(ExecuteSpec::new(vec![entry(fact(1), delayed, "body")], vec![], 4)).unwrap();
    assert_eq!(ids(&admitted.activated), vec!["fact_0001"]);
}

#[test]
fn recursion_fragment_budget_is_enforced() {
    let mut runtime_limits = limits();
    runtime_limits.max_recursion_fragments = 1;
    let result = execute(
        ExecuteSpec::new(
            vec![
                entry(fact(1), rule("alpha"), "beta"),
                entry(fact(2), rule("beta"), "gamma"),
                entry(fact(3), rule("gamma"), "delta"),
            ],
            vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
            4,
        )
        .with_limits(runtime_limits),
    );
    assert!(result.is_err());
}
