use aise::context::activation::index::{
    FragmentMatchCache, FragmentMatchCacheValue, InMemoryFragmentMatchCache, estimate_match_bytes, fragment_cache_key,
};
use aise::domain::asset::ids::Sha256Digest;
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{FactId, RumorId, StoryId, StoryRevision, TurnNumber};
use aise::domain::knowledge::activation::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEntryBody, ActivationEntryMetadata, ActivationError,
    ActivationFragmentMatches, ActivationIndexLimits, ActivationIndexSnapshot, ActivationIndexSnapshotRef,
    ActivationMacroValues, ActivationPattern, ActivationRecursionInput, ActivationRequest, ActivationResult,
    ActivationRuleVersion, ActivationRunMode, ActivationRuntimeLimits, ActivationScanBuffer, ActivationTimedState,
    ExternalActivationSeed, FrozenLiteralIndex, FrozenPackIndex, FrozenPackIndexKey, FrozenRegexSet, GenerationTrigger,
    KnowledgeActivationRule, KnowledgeActivationSession, MATCHER_VERSION, ScanFragment, ScanFragmentKind,
    build_frozen_pack_index, macro_digest,
};
use aise::domain::knowledge::{KnowledgeIdHighWater, KnowledgeKind, KnowledgeSourceId};
use aise::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use aise::domain::turn::KnowledgeDelivery;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct EntrySpec {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub rule: KnowledgeActivationRule,
    pub body: &'static str,
    pub token_cost: u64,
    pub salience: u8,
}

pub fn fact(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(format!("fact_{sequence:04}")).unwrap())
}

pub fn rumor(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Rumor(RumorId::try_new(format!("rumor_{sequence:04}")).unwrap())
}

pub fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "activation_test", 65_536).unwrap()
}

pub fn rule(key: &str) -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.match_rule.keys = vec![ActivationPattern::Literal(key.to_owned())];
    rule
}

pub fn constant_rule() -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.mode.constant = true;
    rule
}

pub fn entry(source_id: KnowledgeSourceId, rule: KnowledgeActivationRule, body: &'static str) -> EntrySpec {
    EntrySpec {
        source_id,
        kind: KnowledgeKind::Fact,
        rule,
        body,
        token_cost: 4,
        salience: 50,
    }
}

pub fn fragment(kind: ScanFragmentKind, depth: u16, order: u32, value: &str) -> ScanFragment {
    ScanFragment::new(kind, depth, order, text(value))
}

pub fn limits() -> ActivationRuntimeLimits {
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

pub fn index_limits() -> ActivationIndexLimits {
    ActivationIndexLimits {
        max_entries: 512,
        max_overlay_entries: 512,
        max_tombstones: 512,
        max_literal_patterns: 4096,
        max_regex_patterns: 1024,
        max_compiled_bytes: 4_194_304,
        max_regex_program_bytes: 1_048_576,
        max_macro_expansion_bytes: 1024,
    }
}

pub fn rule_limits() -> aise::domain::knowledge::activation::ActivationRuleLimits {
    aise::config::ActivationConfig::default().rule.limits()
}

pub fn story_id() -> StoryId {
    StoryId::try_new("activation-story").unwrap()
}

pub fn knowledge_snapshot() -> KnowledgeSnapshotRef {
    KnowledgeSnapshotRef {
        story_id: story_id(),
        pack_digest: Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
        base_revision: StoryRevision::new(7),
        knowledge_id_high_water: KnowledgeIdHighWater::zero(),
    }
}

pub fn macros() -> ActivationMacroValues {
    ActivationMacroValues {
        player_name: "艾琳".to_owned(),
        player_role_label: "守门人".to_owned(),
    }
}

pub fn metadata_of(entries: &[EntrySpec]) -> BTreeMap<KnowledgeSourceId, ActivationEntryMetadata> {
    let mut metadata = BTreeMap::new();
    for spec in entries {
        metadata.insert(
            spec.source_id.clone(),
            ActivationEntryMetadata {
                source_id: spec.source_id.clone(),
                kind: spec.kind,
                rule: spec.rule.clone(),
                rule_version: ActivationRuleVersion::from_rule(&spec.rule),
                salience: spec.salience,
                from_pack: true,
            },
        );
    }
    metadata
}

pub fn build_index(
    entries: &[EntrySpec],
    index_limits: ActivationIndexLimits,
) -> Result<ActivationIndexSnapshot, ActivationError> {
    let snapshot = knowledge_snapshot();
    let metadata = metadata_of(entries);
    let pack: FrozenPackIndex = build_frozen_pack_index(
        FrozenPackIndexKey {
            pack_digest: snapshot.pack_digest.clone(),
            macro_digest: macro_digest(&macros()),
            matcher_version: MATCHER_VERSION,
        },
        metadata.values(),
        &macros(),
        index_limits,
        rule_limits(),
    )?;
    Ok(ActivationIndexSnapshot::new(
        ActivationIndexSnapshotRef::from_knowledge(&snapshot, 3, MATCHER_VERSION),
        metadata,
        pack.literal_index,
        pack.regex_set,
        Arc::new(FrozenLiteralIndex::default()),
        Arc::new(FrozenRegexSet::default()),
    ))
}

pub fn match_all(index: &ActivationIndexSnapshot, buffer: &ActivationScanBuffer) -> ActivationFragmentMatches {
    let mut matches = ActivationFragmentMatches::new();
    for fragment in buffer.fragments() {
        matches.insert(fragment.id.clone(), Arc::new(index.match_fragment(fragment)));
    }
    matches
}

pub fn match_all_cached(
    index: &ActivationIndexSnapshot,
    buffer: &ActivationScanBuffer,
    cache: &dyn FragmentMatchCache,
) -> ActivationFragmentMatches {
    let digest = macro_digest(&macros());
    let story = story_id();
    let mut matches = ActivationFragmentMatches::new();
    for fragment in buffer.fragments() {
        let key = fragment_cache_key(&story, &index.reference, &digest, fragment);
        if let Some(cached) = cache.get(&key) {
            matches.insert(fragment.id.clone(), cached.matches);
            continue;
        }
        let computed = Arc::new(index.match_fragment(fragment));
        cache.insert(
            key,
            FragmentMatchCacheValue {
                matches: computed.clone(),
                estimated_bytes: estimate_match_bytes(&computed),
            },
        );
        matches.insert(fragment.id.clone(), computed);
    }
    matches
}

pub fn new_cache() -> InMemoryFragmentMatchCache {
    InMemoryFragmentMatchCache::new(aise::config::ActivationConfig::default().cache)
}

pub struct ExecuteSpec {
    pub entries: Vec<EntrySpec>,
    pub fragments: Vec<ScanFragment>,
    pub timed_state: Vec<ActivationTimedState>,
    pub external_seeds: Vec<ExternalActivationSeed>,
    pub runtime_limits: ActivationRuntimeLimits,
    pub turn: u64,
    pub continuation: Option<ActivationContinuation>,
    pub cache: Option<Arc<InMemoryFragmentMatchCache>>,
}

impl ExecuteSpec {
    pub fn new(entries: Vec<EntrySpec>, fragments: Vec<ScanFragment>, turn: u64) -> Self {
        Self {
            entries,
            fragments,
            timed_state: Vec::new(),
            external_seeds: Vec::new(),
            runtime_limits: limits(),
            turn,
            continuation: None,
            cache: None,
        }
    }

    pub fn with_limits(mut self, limits: ActivationRuntimeLimits) -> Self {
        self.runtime_limits = limits;
        self
    }

    pub fn with_timed_state(mut self, state: Vec<ActivationTimedState>) -> Self {
        self.timed_state = state;
        self
    }

    pub fn with_seeds(mut self, seeds: Vec<ExternalActivationSeed>) -> Self {
        self.external_seeds = seeds;
        self
    }

    pub fn with_continuation(mut self, continuation: ActivationContinuation) -> Self {
        self.continuation = Some(continuation);
        self
    }

    pub fn with_cache(mut self, cache: Arc<InMemoryFragmentMatchCache>) -> Self {
        self.cache = Some(cache);
        self
    }
}

pub fn execute(spec: ExecuteSpec) -> Result<ActivationResult, ActivationError> {
    let story = story_id();
    let snapshot = knowledge_snapshot();
    let index = build_index(&spec.entries, index_limits())?;
    let scan_buffer = ActivationScanBuffer::try_new(spec.fragments, 64, 65_536).unwrap();
    let fragment_matches = match spec.cache.as_ref() {
        Some(cache) => match_all_cached(&index, &scan_buffer, cache.as_ref()),
        None => match_all(&index, &scan_buffer),
    };
    let bodies = spec
        .entries
        .iter()
        .map(|item| {
            (
                item.source_id.clone(),
                ActivationEntryBody {
                    source_id: item.source_id.clone(),
                    kind: item.kind,
                    token_cost: item.token_cost,
                    body: text(item.body),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let request = ActivationRequest {
        story_id: &story,
        turn_number: TurnNumber::try_new(spec.turn).unwrap(),
        generation_trigger: GenerationTrigger::Normal,
        mode: ActivationRunMode::CommitEligible,
        knowledge_snapshot: &snapshot,
        index_snapshot: &index,
        scan_buffer: &scan_buffer,
        fragment_matches: &fragment_matches,
        timed_state: &spec.timed_state,
        external_seeds: &spec.external_seeds,
        continuation: spec.continuation,
        limits: spec.runtime_limits,
    };
    let mut session = KnowledgeActivationSession::start(request)?;
    while let Some(outcome) = session.next_round()? {
        let supplied = outcome
            .admitted
            .iter()
            .filter_map(|source_id| bodies.get(source_id).cloned())
            .collect::<Vec<_>>();
        for source_id in &outcome.admitted {
            if !bodies.contains_key(source_id) {
                session.drop_admitted(source_id)?;
            }
        }
        session.supply_bodies(ActivationRecursionInput { bodies: supplied })?;
    }
    session.finish()
}

pub fn run(
    entries: Vec<EntrySpec>,
    fragments: Vec<ScanFragment>,
    timed_state: Vec<ActivationTimedState>,
    external_seeds: Vec<ExternalActivationSeed>,
    runtime_limits: ActivationRuntimeLimits,
    turn: u64,
    continuation: Option<ActivationContinuation>,
) -> Result<ActivationResult, ActivationError> {
    let mut spec = ExecuteSpec::new(entries, fragments, turn)
        .with_limits(runtime_limits)
        .with_timed_state(timed_state)
        .with_seeds(external_seeds);
    spec.continuation = continuation;
    execute(spec)
}

pub fn ids(entries: &[ActivatedKnowledgeRef]) -> Vec<String> {
    entries.iter().map(|entry| entry.source_id.as_str().to_owned()).collect()
}

pub fn writer() -> KnowledgeDelivery {
    KnowledgeDelivery::Writer
}
