use crate::harness::{ExecuteSpec, constant_rule, entry, execute, fact, fragment, ids, limits, rule};
use aise::domain::knowledge::activation::{
    ActivationPattern, ActivationRejectionReason, ScanFragmentKind, SecondaryLogic,
};

#[test]
fn primary_literals_normalize_unicode_whitespace_words_punctuation_and_macros() {
    let mut whole_word = rule("cat");
    whole_word.match_rule.match_whole_words = true;
    let entries = vec![
        entry(fact(1), rule("  STRASSE\t灯  "), "one"),
        entry(fact(2), rule("龙门，开启"), "two"),
        entry(fact(3), whole_word, "three"),
        entry(fact(4), rule("{{player_name}} 是{{player_role_label}}"), "four"),
        entry(fact(5), rule("split key"), "five"),
    ];
    let result = execute(ExecuteSpec::new(
        entries,
        vec![
            fragment(
                ScanFragmentKind::PlayerContribution,
                1,
                0,
                "strasse  灯；龙门，开启；cat! concatenate",
            ),
            fragment(ScanFragmentKind::NarrativeDirection, 1, 1, "艾琳 是守门人；split"),
            fragment(ScanFragmentKind::NarrativeEvent, 1, 2, "key"),
        ],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002", "fact_0003", "fact_0004"]);
}

#[test]
fn regex_flags_are_applied_without_literal_fallback() {
    let entries = vec![
        entry(fact(1), rule("/^ALPHA$/im"), "one"),
        entry(fact(2), rule("/a.b/su"), "two"),
    ];
    let result = execute(ExecuteSpec::new(
        entries,
        vec![fragment(
            ScanFragmentKind::PlayerContribution,
            1,
            0,
            "x\nalpha\ny\na\nb",
        )],
        4,
    ))
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002"]);
    let invalid = execute(ExecuteSpec::new(
        vec![entry(fact(3), rule("/alpha/ii"), "bad")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ));
    assert!(invalid.is_err());
}

#[test]
fn all_secondary_logic_variants_merge_matches_across_fragments() {
    let cases = [
        (SecondaryLogic::AndAny, "one", true),
        (SecondaryLogic::AndAny, "absent", false),
        (SecondaryLogic::AndAll, "one two", true),
        (SecondaryLogic::AndAll, "one", false),
        (SecondaryLogic::NotAny, "absent", true),
        (SecondaryLogic::NotAny, "one", false),
        (SecondaryLogic::NotAll, "one", true),
        (SecondaryLogic::NotAll, "one two", false),
    ];
    for (logic, secondary_text, expected) in cases {
        let mut candidate_rule = rule("primary");
        candidate_rule.match_rule.secondary_keys = vec![
            ActivationPattern::Literal("one".to_owned()),
            ActivationPattern::Literal("two".to_owned()),
        ];
        candidate_rule.match_rule.secondary_logic = logic;
        let result = execute(ExecuteSpec::new(
            vec![entry(fact(1), candidate_rule, "body")],
            vec![
                fragment(ScanFragmentKind::PlayerContribution, 1, 0, "primary"),
                fragment(ScanFragmentKind::NarrativeEvent, 1, 1, secondary_text),
            ],
            4,
        ))
        .unwrap();
        assert_eq!(!result.activated.is_empty(), expected, "{logic:?} {secondary_text}");
    }
}

#[test]
fn exact_target_only_entries_never_match_text() {
    let mut exact = rule("alpha");
    exact.mode.exact_target_only = true;
    let result = execute(ExecuteSpec::new(
        vec![entry(fact(1), exact, "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        4,
    ))
    .unwrap();
    assert!(result.activated.is_empty());
}

#[test]
fn disabled_entries_are_rejected_once() {
    let mut disabled = constant_rule();
    disabled.mode.enabled = false;
    let result =
        execute(ExecuteSpec::new(vec![entry(fact(1), disabled, "body")], vec![], 4).with_limits(limits())).unwrap();
    assert!(result.activated.is_empty());
    assert_eq!(result.rejection_summary.get(&ActivationRejectionReason::Disabled), Some(&1));
}
