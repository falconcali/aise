use aise::domain::asset::ids::Sha256Digest;
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{FactId, RoleId, RumorId, StoryId, StoryRevision, TurnNumber};
use aise::domain::knowledge::activation::{
    ActivatedKnowledgeRef, ActivationBudgetClass, ActivationContinuation, ActivationEntryInput,
    ActivationEntryMetadata, ActivationExecutionInput, ActivationGroupKey, ActivationIndexSnapshot,
    ActivationIndexSnapshotRef, ActivationMacroValues, ActivationPattern, ActivationRejectionReason, ActivationRequest,
    ActivationRunMode, ActivationRuntimeLimits, ActivationScanBuffer, ActivationSeedKind, ActivationStopReason,
    ActivationTimedState, ExternalActivationSeed, GenerationTrigger, KnowledgeActivationEngine,
    KnowledgeActivationRule, ScanFragment, ScanFragmentKind, SecondaryLogic,
};
use aise::domain::knowledge::{KnowledgeIdHighWater, KnowledgeKind, KnowledgeSourceId};
use aise::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use aise::domain::turn::KnowledgeDelivery;
use std::collections::BTreeMap;

#[derive(Clone)]
struct EntrySpec {
    source_id: KnowledgeSourceId,
    kind: KnowledgeKind,
    rule: KnowledgeActivationRule,
    body: &'static str,
    token_cost: u64,
    deliveries: Vec<KnowledgeDelivery>,
    salience: u8,
}

fn fact(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(format!("fact_{sequence:04}")).unwrap())
}

fn rumor(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Rumor(RumorId::try_new(format!("rumor_{sequence:04}")).unwrap())
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "activation_test", 65_536).unwrap()
}

fn rule(key: &str) -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.match_rule.keys = vec![ActivationPattern::Literal(key.to_owned())];
    rule
}

fn constant_rule() -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.mode.constant = true;
    rule
}

fn entry(source_id: KnowledgeSourceId, rule: KnowledgeActivationRule, body: &'static str) -> EntrySpec {
    EntrySpec {
        source_id,
        kind: KnowledgeKind::Fact,
        rule,
        body,
        token_cost: 4,
        deliveries: vec![KnowledgeDelivery::Writer],
        salience: 50,
    }
}

fn fragment(kind: ScanFragmentKind, depth: u16, order: u32, value: &str) -> ScanFragment {
    ScanFragment::new(kind, depth, order, text(value))
}

fn limits() -> ActivationRuntimeLimits {
    ActivationRuntimeLimits {
        minimum_activations: 0,
        initial_scan_depth: 1,
        max_scan_depth: 8,
        include_summary_at_max_depth: true,
        max_scan_fragments: 64,
        max_scan_bytes: 65_536,
        max_scan_tokens: 65_536,
        max_literal_patterns: 256,
        max_regex_patterns: 256,
        max_pattern_matches: 4096,
        max_candidates_per_round: 256,
        max_recursion_steps: 8,
        max_recursion_fragments: 64,
        max_recursion_bytes: 65_536,
        max_recursion_tokens: 65_536,
        max_activated_entries: 128,
        max_depth_expansions: 8,
        max_external_candidates: 128,
        max_evidence_per_entry: 32,
        max_evidence_bytes: 65_536,
        max_items_per_audience: 64,
        max_tokens_per_audience: 10_000,
        max_total_items: 128,
        max_total_tokens: 20_000,
        max_single_entry_bytes: 4096,
        reserved_tokens: 2000,
        mandatory_tokens: 2000,
    }
}

fn execute(
    entries: Vec<EntrySpec>,
    fragments: Vec<ScanFragment>,
    timed_state: Vec<ActivationTimedState>,
    external_seeds: Vec<ExternalActivationSeed>,
    runtime_limits: ActivationRuntimeLimits,
    turn: u64,
    continuation: Option<ActivationContinuation>,
) -> Result<aise::domain::knowledge::activation::ActivationResult, aise::domain::knowledge::activation::ActivationError>
{
    let story_id = StoryId::try_new("activation-story").unwrap();
    let digest =
        Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000001").unwrap();
    let knowledge_snapshot = KnowledgeSnapshotRef {
        story_id: story_id.clone(),
        pack_digest: digest.clone(),
        base_revision: StoryRevision::new(7),
        knowledge_id_high_water: KnowledgeIdHighWater::zero(),
    };
    let mut metadata = BTreeMap::new();
    let mut runtime_entries = Vec::new();
    for spec in entries {
        let version = spec.rule.rule_version().unwrap();
        metadata.insert(
            spec.source_id.clone(),
            ActivationEntryMetadata {
                source_id: spec.source_id.clone(),
                kind: spec.kind,
                rule: spec.rule,
                rule_version: version,
                salience: spec.salience,
            },
        );
        runtime_entries.push(ActivationEntryInput {
            source_id: spec.source_id,
            deliveries: spec.deliveries,
            token_cost: spec.token_cost,
            body: text(spec.body),
        });
    }
    let execution_input = ActivationExecutionInput::try_new(
        runtime_entries,
        ActivationMacroValues {
            player_name: "艾琳".to_owned(),
            player_role_label: "守门人".to_owned(),
        },
        256,
        65_536,
        128,
        1024,
    )
    .unwrap();
    let index =
        ActivationIndexSnapshot::new(ActivationIndexSnapshotRef::from_knowledge(&knowledge_snapshot, 3, 1), metadata)
            .with_execution_input(execution_input);
    let scan_buffer = ActivationScanBuffer::try_new(fragments, 64, 65_536).unwrap();
    KnowledgeActivationEngine.run(ActivationRequest {
        story_id: &story_id,
        turn_number: TurnNumber::try_new(turn).unwrap(),
        generation_trigger: GenerationTrigger::Normal,
        mode: ActivationRunMode::CommitEligible,
        knowledge_snapshot: &knowledge_snapshot,
        index_snapshot: &index,
        scan_buffer: &scan_buffer,
        timed_state: &timed_state,
        external_seeds: &external_seeds,
        continuation,
        limits: runtime_limits,
    })
}

fn ids(entries: &[ActivatedKnowledgeRef]) -> Vec<String> {
    entries.iter().map(|entry| entry.source_id.as_str().to_owned()).collect()
}

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
    let result = execute(
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
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002", "fact_0003", "fact_0004"]);
}

#[test]
fn regex_flags_are_applied_without_literal_fallback() {
    let entries = vec![
        entry(fact(1), rule("/^ALPHA$/im"), "one"),
        entry(fact(2), rule("/a.b/su"), "two"),
    ];
    let result = execute(
        entries,
        vec![fragment(
            ScanFragmentKind::PlayerContribution,
            1,
            0,
            "x\nalpha\ny\na\nb",
        )],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002"]);
    let invalid = execute(
        vec![entry(fact(3), rule("/alpha/ii"), "bad")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    );
    assert!(invalid.is_err());
}

#[test]
fn all_secondary_logic_variants_merge_matches_across_fragments() {
    let cases = [
        (SecondaryLogic::AndAny, "one", true),
        (SecondaryLogic::AndAny, "none", false),
        (SecondaryLogic::AndAll, "one two", true),
        (SecondaryLogic::AndAll, "one", false),
        (SecondaryLogic::NotAny, "none", true),
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
        let result = execute(
            vec![entry(fact(1), candidate_rule, "body")],
            vec![
                fragment(ScanFragmentKind::PlayerContribution, 1, 0, "primary"),
                fragment(ScanFragmentKind::NarrativeEvent, 1, 1, secondary_text),
            ],
            vec![],
            vec![],
            limits(),
            4,
            None,
        )
        .unwrap();
        assert_eq!(!result.activated.is_empty(), expected, "{logic:?} {secondary_text}");
    }
}

#[test]
fn scan_depth_expands_to_minimum_and_respects_entry_cap() {
    let mut capped = rule("deep");
    capped.match_rule.scan_depth = Some(2);
    let mut runtime_limits = limits();
    runtime_limits.minimum_activations = 2;
    runtime_limits.initial_scan_depth = 1;
    runtime_limits.max_scan_depth = 4;
    let result = execute(
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
        vec![],
        vec![],
        runtime_limits,
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&result.activated), vec!["fact_0001", "fact_0002"]);
    assert_eq!(result.stop_reason, ActivationStopReason::MinimumSatisfied);
    assert_eq!(result.continuation.scan_depth, 2);
}

#[test]
fn recursion_chain_cycle_and_controls_are_bounded() {
    let chain = execute(
        vec![
            entry(fact(1), rule("alpha"), "beta"),
            entry(fact(2), rule("beta"), "gamma"),
            entry(fact(3), rule("gamma"), "alpha"),
        ],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&chain.activated), vec!["fact_0001", "fact_0002", "fact_0003"]);
    assert!(chain.continuation.consumed.recursion_steps <= 3);

    let mut prevent = rule("alpha");
    prevent.recursion.prevent_recursion = true;
    let prevented = execute(
        vec![entry(fact(1), prevent, "beta"), entry(fact(2), rule("beta"), "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&prevented.activated), vec!["fact_0001"]);

    let mut excluded = rule("beta");
    excluded.recursion.exclude_recursion = true;
    let excluded_result = execute(
        vec![entry(fact(1), rule("alpha"), "beta"), entry(fact(2), excluded, "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
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
    let result = execute(
        vec![
            entry(fact(1), rule("alpha"), "beta gamma"),
            entry(fact(2), delayed, "done"),
            entry(fact(3), rule("gamma"), "beta"),
        ],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
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
    let scored = execute(
        vec![entry(fact(1), lower.clone(), "one"), entry(fact(2), higher, "two")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha beta")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ids(&scored.activated), vec!["fact_0002"]);

    let sticky_version = lower.rule_version().unwrap();
    let mut override_rule = lower.clone();
    override_rule.selection.group_override = true;
    override_rule.selection.order = 100;
    let sticky = execute(
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
fn probability_is_stable_once_per_turn_and_honors_boundaries() {
    let mut never = constant_rule();
    never.selection.probability = 0;
    let always = constant_rule();
    let first = execute(
        vec![entry(fact(1), never, "never"), entry(fact(2), always, "always")],
        vec![],
        vec![],
        vec![],
        limits(),
        11,
        None,
    )
    .unwrap();
    assert_eq!(ids(&first.activated), vec!["fact_0002"]);
    assert_eq!(first.rejection_summary.get(&ActivationRejectionReason::Probability), Some(&1));
    let resumed = execute(
        vec![
            {
                let mut rule = constant_rule();
                rule.selection.probability = 0;
                entry(fact(1), rule, "never")
            },
            entry(fact(2), constant_rule(), "always"),
        ],
        vec![],
        vec![],
        vec![],
        limits(),
        11,
        Some(first.continuation),
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
    let version = timed_rule.rule_version().unwrap();
    let initial = execute(
        vec![entry(fact(1), timed_rule.clone(), "body")],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        5,
        None,
    )
    .unwrap();
    let state = initial.pending_timed_state.upserts[0].clone();
    assert_eq!(state.sticky_through_turn.unwrap().get(), 7);
    assert_eq!(state.cooldown_through_turn.unwrap().get(), 9);
    let sticky = execute(
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
    let cooling = execute(
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
    assert_ne!(changed_rule.rule_version().unwrap(), version);
    let invalidated = execute(
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
fn delay_suppresses_exact_configured_turn_range() {
    let mut delayed = constant_rule();
    delayed.timing.delay_turns = 3;
    let blocked = execute(
        vec![entry(fact(1), delayed.clone(), "body")],
        vec![],
        vec![],
        vec![],
        limits(),
        3,
        None,
    )
    .unwrap();
    assert!(blocked.activated.is_empty());
    let admitted = execute(vec![entry(fact(1), delayed, "body")], vec![], vec![], vec![], limits(), 4, None).unwrap();
    assert_eq!(ids(&admitted.activated), vec!["fact_0001"]);
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
        vec![normal_entry, reserved_entry, mandatory_entry],
        vec![],
        vec![],
        vec![],
        runtime_limits,
        4,
        None,
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
    assert!(execute(vec![oversized], vec![], vec![], vec![], hard_limits, 4, None).is_err());
}

#[test]
fn continuation_adds_one_new_delivery_without_reactivation() {
    let role_id = RoleId::try_new("companion").unwrap();
    let mut rumor_entry = entry(rumor(1), rule("alpha"), "rumor body");
    rumor_entry.kind = KnowledgeKind::Rumor;
    rumor_entry.token_cost = 7;
    let first = execute(
        vec![rumor_entry.clone()],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![],
        limits(),
        4,
        None,
    )
    .unwrap();
    let resumed = execute(
        vec![rumor_entry],
        vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")],
        vec![],
        vec![ExternalActivationSeed {
            source_id: rumor(1),
            delivery: KnowledgeDelivery::Character { role_id },
            kind: ActivationSeedKind::PlannerExactTarget,
            provider_rank: None,
            mandatory: true,
        }],
        limits(),
        4,
        Some(first.continuation),
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
    let first = execute(entries.clone(), fragments.clone(), vec![], vec![], limits(), 9, None).unwrap();
    let second = execute(
        entries.into_iter().rev().collect(),
        fragments.into_iter().rev().collect(),
        vec![],
        vec![],
        limits(),
        9,
        None,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&first.activated).unwrap(),
        serde_json::to_value(&second.activated).unwrap()
    );
    assert_eq!(first.rejection_summary, second.rejection_summary);
    assert_eq!(first.pending_timed_state.upserts, second.pending_timed_state.upserts);
    assert_eq!(first.pending_timed_state.deletes, second.pending_timed_state.deletes);
}
