use crate::harness::{ExecuteSpec, constant_rule, entry, execute, fact, fragment, ids, limits, rule, rumor, run};
use aise::domain::ids::RoleId;
use aise::domain::knowledge::KnowledgeKind;
use aise::domain::knowledge::activation::{
    ActivationBudgetClass, ActivationGroupKey, ActivationRejectionReason, ActivationRuleVersion, ActivationSeedKind,
    ExternalActivationSeed, ScanFragmentKind,
};
use aise::domain::turn::KnowledgeDelivery;

#[test]
fn probability_is_stable_once_per_turn_and_honors_boundaries() {
    let mut never = constant_rule();
    never.selection.probability = 0;
    let always = constant_rule();
    let first = execute(ExecuteSpec::new(
        vec![entry(fact(1), never, "never"), entry(fact(2), always, "always")],
        vec![],
        11,
    ))
    .unwrap();
    assert_eq!(ids(&first.activated), vec!["fact_0002"]);
    assert_eq!(first.rejection_summary.get(&ActivationRejectionReason::Probability), Some(&1));
    let resumed = execute(
        ExecuteSpec::new(
            vec![
                {
                    let mut rule = constant_rule();
                    rule.selection.probability = 0;
                    entry(fact(1), rule, "never")
                },
                entry(fact(2), constant_rule(), "always"),
            ],
            vec![],
            11,
        )
        .with_continuation(first.continuation),
    )
    .unwrap();
    assert_eq!(ids(&resumed.activated), vec!["fact_0002"]);
    assert_eq!(resumed.continuation.failed_probability.len(), 1);
}

#[test]
fn timing_ranges_and_rule_version_invalidation_are_exact() {
    let mut timed_rule = rule("alpha");
    timed_rule.timing.sticky_turns = 2;
    timed_rule.timing.cooldown_turns = 2;
    let version = ActivationRuleVersion::from_rule(&timed_rule);
    let initial = execute(ExecuteSpec::new(
        vec![entry(fact(1), timed_rule.clone(), "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        5,
    ))
    .unwrap();
    let state = initial.pending_timed_state.upserts[0].clone();
    assert_eq!(state.sticky_through_turn.unwrap().get(), 7);
    assert_eq!(state.cooldown_through_turn.unwrap().get(), 9);
    let sticky = run(
        vec![entry(fact(1), timed_rule.clone(), "body")],
        vec![],
        vec![state.clone()],
        vec![],
        limits(),
        7,
        None,
    )
    .unwrap();
    assert_eq!(ids(&sticky.activated), vec!["fact_0001"]);
    assert!(sticky.pending_timed_state.upserts.is_empty());
    let cooling = run(
        vec![entry(fact(1), timed_rule.clone(), "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![state.clone()],
        vec![],
        limits(),
        8,
        None,
    )
    .unwrap();
    assert!(cooling.activated.is_empty());
    assert_eq!(cooling.rejection_summary.get(&ActivationRejectionReason::Cooldown), Some(&1));
    let mut changed_rule = timed_rule;
    changed_rule.selection.order = 1;
    assert_ne!(ActivationRuleVersion::from_rule(&changed_rule), version);
    let invalidated = run(
        vec![entry(fact(1), changed_rule, "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![state],
        vec![],
        limits(),
        8,
        None,
    )
    .unwrap();
    assert_eq!(ids(&invalidated.activated), vec!["fact_0001"]);
    assert!(invalidated.pending_timed_state.deletes.is_empty());
    assert_eq!(invalidated.pending_timed_state.upserts.len(), 1);
}

#[test]
fn normal_reserved_mandatory_and_audience_budgets_are_enforced() {
    let mut normal = constant_rule();
    normal.selection.order = 3;
    let mut reserved = constant_rule();
    reserved.selection.order = 2;
    reserved.budget_class = ActivationBudgetClass::Reserved;
    let mut mandatory = constant_rule();
    mandatory.selection.order = 1;
    mandatory.budget_class = ActivationBudgetClass::Mandatory;
    let mut runtime_limits = limits();
    runtime_limits.max_total_tokens = 30;
    runtime_limits.max_tokens_per_audience = 30;
    runtime_limits.reserved_tokens = 10;
    runtime_limits.mandatory_tokens = 10;
    let mut normal_entry = entry(fact(1), normal, "normal");
    normal_entry.token_cost = 20;
    let mut reserved_entry = entry(fact(2), reserved, "reserved");
    reserved_entry.token_cost = 10;
    let mut mandatory_entry = entry(fact(3), mandatory, "mandatory");
    mandatory_entry.token_cost = 10;
    let result = execute(
        ExecuteSpec::new(vec![normal_entry, reserved_entry, mandatory_entry], vec![], 4).with_limits(runtime_limits),
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0003", "fact_0002"]);
    assert_eq!(result.continuation.consumed.knowledge_tokens, 20);

    let mut mandatory = constant_rule();
    mandatory.budget_class = ActivationBudgetClass::Mandatory;
    let mut oversized = entry(fact(4), mandatory, "oversized");
    oversized.token_cost = 31;
    let mut hard_limits = limits();
    hard_limits.max_total_tokens = 30;
    hard_limits.max_tokens_per_audience = 30;
    hard_limits.reserved_tokens = 5;
    hard_limits.mandatory_tokens = 5;
    assert!(execute(ExecuteSpec::new(vec![oversized], vec![], 4).with_limits(hard_limits)).is_err());
}

#[test]
fn continuation_adds_one_new_delivery_without_reactivation() {
    let role_id = RoleId::try_new("companion").unwrap();
    let mut rumor_entry = entry(rumor(1), rule("alpha"), "rumor body");
    rumor_entry.kind = KnowledgeKind::Rumor;
    rumor_entry.token_cost = 7;
    let first = execute(ExecuteSpec::new(
        vec![rumor_entry.clone()],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    let resumed = execute(
        ExecuteSpec::new(
            vec![rumor_entry],
            vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
            4,
        )
        .with_seeds(vec![ExternalActivationSeed {
            source_id: rumor(1),
            delivery: KnowledgeDelivery::Character { role_id },
            kind: ActivationSeedKind::PlannerExactTarget,
            provider_rank: None,
            mandatory: true,
        }])
        .with_continuation(first.continuation),
    )
    .unwrap();
    assert_eq!(resumed.activated.len(), 1);
    assert_eq!(resumed.activated[0].deliveries.len(), 2);
    assert_eq!(resumed.continuation.consumed.knowledge_tokens, 14);
}

#[test]
fn input_permutation_produces_identical_order_evidence_rejections_and_delta() {
    let mut grouped = rule("alpha");
    grouped.selection.groups = vec![ActivationGroupKey::try_new("g").unwrap()];
    grouped.selection.group_weight = 0;
    let mut rejected = constant_rule();
    rejected.selection.probability = 0;
    let entries = vec![
        entry(fact(2), grouped.clone(), "two"),
        entry(fact(1), grouped, "one"),
        entry(fact(3), rejected, "three"),
    ];
    let fragments = vec![
        fragment(ScanFragmentKind::NarrativeEvent, 1, 2, "alpha"),
        fragment(ScanFragmentKind::PlayerContribution, 1, 1, "alpha"),
    ];
    let first = execute(ExecuteSpec::new(entries.clone(), fragments.clone(), 9)).unwrap();
    let second = execute(ExecuteSpec::new(
        entries.into_iter().rev().collect(),
        fragments.into_iter().rev().collect(),
        9,
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&first.activated).unwrap(),
        serde_json::to_value(&second.activated).unwrap()
    );
    assert_eq!(first.rejection_summary, second.rejection_summary);
    assert_eq!(first.pending_timed_state.upserts, second.pending_timed_state.upserts);
    assert_eq!(first.pending_timed_state.deletes, second.pending_timed_state.deletes);
}

#[test]
fn single_entry_byte_ceiling_rejects_without_dropping_the_turn() {
    let mut runtime_limits = limits();
    runtime_limits.max_single_entry_bytes = 2;
    let result = execute(
        ExecuteSpec::new(vec![entry(fact(1), constant_rule(), "oversized body")], vec![], 4)
            .with_limits(runtime_limits),
    )
    .unwrap();
    assert!(result.activated.is_empty());
    assert_eq!(result.rejection_summary.get(&ActivationRejectionReason::Budget), Some(&1));
}
