use super::*;
use serde_json::{Value, json};

type TestEntry = (&'static str, ActivationRule, BoundedText);

fn config() -> ActivationConfig {
    ActivationConfig {
        scan_depth: 1,
        max_scan_depth: 4,
        max_recursion_steps: 3,
        max_activated_entries: 16,
        max_knowledge_bytes: 4096,
    }
}

fn entry(key: &'static str, rule: Value, content: &str) -> TestEntry {
    (
        key,
        serde_json::from_value(rule).expect("test rule should deserialize"),
        BoundedText::try_new(content, "test", usize::MAX).expect("test content should fit"),
    )
}

fn player(text: &str) -> ScanFragment {
    ScanFragment::new(ScanFragmentKind::PlayerContribution, 0, text)
}

fn run(
    entries: &[TestEntry],
    fragments: Vec<ScanFragment>,
    config: &ActivationConfig,
) -> ActivationOutcome<&'static str> {
    let story_id = StoryId::try_new("story").expect("test story id should be valid");
    let turn_number = TurnNumber::new(1);
    let candidates = entries
        .iter()
        .map(|(key, rule, content)| ActivationCandidate {
            key: *key,
            rule,
            content,
        })
        .collect();
    activate(
        candidates,
        &ScanBuffer::new(fragments),
        ActivationTurn {
            story_id: &story_id,
            turn_number: &turn_number,
        },
        config,
    )
}

fn keys(outcome: &ActivationOutcome<&'static str>) -> Vec<&'static str> {
    outcome.activated.iter().map(|entry| entry.key).collect()
}

fn rejected(outcome: &ActivationOutcome<&'static str>, reason: ActivationRejection) -> u32 {
    outcome.rejections.get(&reason).copied().unwrap_or_default()
}

#[test]
fn primary_key_in_player_contribution_activates_entry_with_evidence() {
    let entries = [
        entry("snake", json!({ "match": { "keys": ["蛇妖", "白蛇"] } }), "白素贞是白蛇。"),
        entry("temple", json!({ "match": { "keys": ["金山寺"] } }), "金山寺很有声望。"),
    ];

    let outcome = run(&entries, vec![player("我听说西湖有白蛇出没")], &config());

    assert_eq!(keys(&outcome), ["snake"]);
    let activated = &outcome.activated[0];
    assert_eq!(activated.class, ActivationClass::TextMatch);
    assert_eq!(
        activated.evidence,
        [ActivationEvidence {
            pattern_kind: ActivationPatternKind::Primary,
            pattern_ordinal: 1,
            fragment_kind: ScanFragmentKind::PlayerContribution,
            depth: 0,
            match_count: 1,
        }]
    );
    assert_eq!(outcome.stop_reason, ActivationStopReason::Complete);
}

#[test]
fn disabled_and_keyless_entries_never_activate_from_text() {
    let entries = [
        entry(
            "disabled",
            json!({ "match": { "keys": ["白蛇"] }, "mode": { "enabled": false } }),
            "x",
        ),
        entry("keyless", json!({}), "白蛇"),
    ];

    let outcome = run(&entries, vec![player("白蛇")], &config());

    assert!(outcome.activated.is_empty());
}

#[test]
fn secondary_logic_gates_primary_matches() {
    let entries = [
        entry(
            "and_any",
            json!({ "match": { "keys": ["法海"], "secondary_keys": ["白蛇", "妖"] } }),
            "a",
        ),
        entry(
            "not_any",
            json!({ "match": { "keys": ["法海"], "secondary_keys": ["白蛇"], "secondary_logic": "not_any" } }),
            "b",
        ),
        entry(
            "and_all",
            json!({ "match": { "keys": ["法海"], "secondary_keys": ["白蛇", "雷峰塔"], "secondary_logic": "and_all" } }),
            "c",
        ),
    ];

    let outcome = run(&entries, vec![player("法海盯上了白蛇")], &config());

    assert_eq!(keys(&outcome), ["and_any"]);
    assert_eq!(rejected(&outcome, ActivationRejection::SecondaryCondition), 2);
}

#[test]
fn scan_depth_limits_visible_recent_story() {
    let fragments = || {
        vec![
            player("继续"),
            ScanFragment::new(ScanFragmentKind::RecentStory, 1, "雨停了"),
            ScanFragment::new(ScanFragmentKind::RecentStory, 2, "许仙借伞给白娘子"),
        ]
    };
    let shallow = [entry("umbrella", json!({ "match": { "keys": ["借伞"] } }), "x")];
    let deep = [entry(
        "umbrella",
        json!({ "match": { "keys": ["借伞"], "scan_depth": 2 } }),
        "x",
    )];
    let capped = [entry(
        "umbrella",
        json!({ "match": { "keys": ["借伞"], "scan_depth": 9 } }),
        "x",
    )];
    let mut tight = config();
    tight.max_scan_depth = 1;

    assert!(run(&shallow, fragments(), &config()).activated.is_empty());
    assert_eq!(keys(&run(&deep, fragments(), &config())), ["umbrella"]);
    assert!(run(&capped, fragments(), &tight).activated.is_empty());
}

#[test]
fn constant_entries_activate_first_without_matches() {
    let entries = [
        entry(
            "matched",
            json!({ "match": { "keys": ["西湖"] }, "selection": { "order": 100 } }),
            "a",
        ),
        entry("constant", json!({ "mode": { "constant": true } }), "b"),
    ];

    let outcome = run(&entries, vec![player("西湖")], &config());

    assert_eq!(keys(&outcome), ["constant", "matched"]);
    assert_eq!(outcome.activated[0].class, ActivationClass::Constant);
}

#[test]
fn activated_content_triggers_recursive_activation() {
    let entries = [
        entry("bai", json!({ "match": { "keys": ["白娘子"] } }), "白娘子的真身是白蛇。"),
        entry("snake", json!({ "match": { "keys": ["白蛇"] } }), "白蛇修行千年。"),
    ];

    let outcome = run(&entries, vec![player("我去找白娘子")], &config());

    assert_eq!(keys(&outcome), ["bai", "snake"]);
    assert_eq!(outcome.activated[1].class, ActivationClass::Recursion);
    assert_eq!(outcome.activated[1].recursion_level, 1);
    assert_eq!(
        outcome.activated[1].evidence[0].fragment_kind,
        ScanFragmentKind::RecursionContent
    );
}

#[test]
fn recursion_controls_are_enforced() {
    let prevent = [
        entry(
            "bai",
            json!({ "match": { "keys": ["白娘子"] }, "recursion": { "prevent_recursion": true } }),
            "白娘子的真身是白蛇。",
        ),
        entry("snake", json!({ "match": { "keys": ["白蛇"] } }), "x"),
    ];
    let exclude = [
        entry("bai", json!({ "match": { "keys": ["白娘子"] } }), "白娘子的真身是白蛇。"),
        entry(
            "snake",
            json!({ "match": { "keys": ["白蛇"] }, "recursion": { "exclude_recursion": true } }),
            "x",
        ),
    ];

    assert_eq!(keys(&run(&prevent, vec![player("白娘子")], &config())), ["bai"]);
    let excluded = run(&exclude, vec![player("白娘子")], &config());
    assert_eq!(keys(&excluded), ["bai"]);
    assert_eq!(rejected(&excluded, ActivationRejection::RecursionExcluded), 1);
}

#[test]
fn delayed_entries_wait_for_recursion_level() {
    let delayed = json!({ "match": { "keys": ["西湖"] }, "recursion": { "delay_until_recursion": 1 } });
    let alone = [entry("lake", delayed.clone(), "x")];
    let with_trigger = [
        entry("lake", delayed, "x"),
        entry("bai", json!({ "match": { "keys": ["白娘子"] } }), "白娘子住在附近。"),
    ];

    let locked = run(&alone, vec![player("西湖")], &config());
    let unlocked = run(&with_trigger, vec![player("西湖边的白娘子")], &config());

    assert!(locked.activated.is_empty());
    assert_eq!(rejected(&locked, ActivationRejection::RecursionLocked), 1);
    assert_eq!(keys(&unlocked), ["bai", "lake"]);
    assert_eq!(unlocked.activated[1].recursion_level, 1);
    assert!(unlocked.rejections.is_empty());
}

#[test]
fn recursion_stops_at_configured_step_limit() {
    let entries = [
        entry("a", json!({ "match": { "keys": ["甲"] } }), "乙"),
        entry("b", json!({ "match": { "keys": ["乙"] } }), "丙"),
        entry("c", json!({ "match": { "keys": ["丙"] } }), "丁"),
    ];
    let mut limited = config();
    limited.max_recursion_steps = 1;

    let outcome = run(&entries, vec![player("甲")], &limited);

    assert_eq!(keys(&outcome), ["a", "b"]);
    assert_eq!(outcome.recursion_level, 1);
    assert_eq!(outcome.stop_reason, ActivationStopReason::RecursionExhausted);
}

#[test]
fn inclusion_group_keeps_a_single_winner() {
    let entries = [
        entry(
            "plain",
            json!({ "match": { "keys": ["白蛇"] }, "selection": { "groups": ["bai.identity"], "order": 50 } }),
            "a",
        ),
        entry(
            "override",
            json!({ "match": { "keys": ["白蛇"] }, "selection": { "groups": ["bai.identity"], "group_override": true } }),
            "b",
        ),
        entry("free", json!({ "match": { "keys": ["白蛇"] } }), "c"),
    ];

    let outcome = run(&entries, vec![player("白蛇")], &config());

    assert_eq!(keys(&outcome), ["free", "override"]);
    assert_eq!(rejected(&outcome, ActivationRejection::GroupLoser), 1);
}

#[test]
fn group_won_in_earlier_round_blocks_later_members() {
    let entries = [
        entry(
            "first",
            json!({ "match": { "keys": ["白娘子"] }, "selection": { "groups": ["bai.identity"] } }),
            "她是白蛇。",
        ),
        entry(
            "second",
            json!({ "match": { "keys": ["白蛇"] }, "selection": { "groups": ["bai.identity"] } }),
            "x",
        ),
    ];

    let outcome = run(&entries, vec![player("白娘子")], &config());

    assert_eq!(keys(&outcome), ["first"]);
    assert_eq!(rejected(&outcome, ActivationRejection::GroupLoser), 1);
}

#[test]
fn probability_is_deterministic_and_zero_never_activates() {
    const KEYS: [&str; 20] = [
        "e00", "e01", "e02", "e03", "e04", "e05", "e06", "e07", "e08", "e09", "e10", "e11", "e12", "e13", "e14", "e15",
        "e16", "e17", "e18", "e19",
    ];
    let entries = KEYS
        .into_iter()
        .map(|key| {
            entry(
                key,
                json!({ "match": { "keys": ["钱塘"] }, "selection": { "probability": 50 } }),
                "x",
            )
        })
        .chain([entry(
            "never",
            json!({ "match": { "keys": ["钱塘"] }, "selection": { "probability": 0 } }),
            "x",
        )])
        .collect::<Vec<_>>();

    let first = run(&entries, vec![player("钱塘")], &config());
    let second = run(&entries, vec![player("钱塘")], &config());

    assert_eq!(keys(&first), keys(&second));
    assert!(!keys(&first).contains(&"never"));
    assert!(!first.activated.is_empty() && first.activated.len() < 20);
    assert_eq!(
        rejected(&first, ActivationRejection::Probability) as usize,
        21 - first.activated.len()
    );
}

#[test]
fn budget_trims_lower_ranked_entries() {
    let entries = [
        entry("low", json!({ "match": { "keys": ["西湖"] } }), "low"),
        entry(
            "high",
            json!({ "match": { "keys": ["西湖"] }, "selection": { "order": 10 } }),
            "high",
        ),
        entry("scored", json!({ "match": { "keys": ["西湖", "断桥"] } }), "scored"),
    ];
    let mut limited = config();
    limited.max_activated_entries = 2;

    let outcome = run(&entries, vec![player("西湖断桥")], &limited);

    assert_eq!(keys(&outcome), ["high", "scored"]);
    assert_eq!(rejected(&outcome, ActivationRejection::Budget), 1);
    assert_eq!(outcome.stop_reason, ActivationStopReason::BudgetTrimmed);
}

#[test]
fn knowledge_byte_budget_is_enforced() {
    let entries = [
        entry(
            "long",
            json!({ "match": { "keys": ["西湖"] }, "selection": { "order": 1 } }),
            "0123456789",
        ),
        entry("short", json!({ "match": { "keys": ["西湖"] } }), "01234"),
    ];
    let mut limited = config();
    limited.max_knowledge_bytes = 12;

    let outcome = run(&entries, vec![player("西湖")], &limited);

    assert_eq!(keys(&outcome), ["long"]);
    assert_eq!(rejected(&outcome, ActivationRejection::Budget), 1);
}
