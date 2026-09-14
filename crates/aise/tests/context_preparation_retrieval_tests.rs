use aise::config::ActivationConfig;
use aise::context::activation::{ActivationRunSpec, KnowledgeActivationCoordinator};
use aise::domain::asset::ids::{PackId, Sha256Digest};
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::{FactId, MemoryId, RoleId, StoryId, StoryRevision, TurnNumber};
use aise::domain::knowledge::activation::{
    ActivationEntryMetadata, ActivationIndexLimits, ActivationIndexMetadata, ActivationIndexSnapshotRef,
    ActivationMacroValues, ActivationPattern, ActivationRuleVersion, ActivationRunMode, ActivationScanBuffer,
    ActivationTimedState, GenerationTrigger, KnowledgeActivationRule, ScanFragment, ScanFragmentKind,
};
use aise::domain::knowledge::{KnowledgeIdHighWater, KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use aise::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use aise::persistence::activation_index_port::ActivationIndexPort;
use aise::persistence::activation_timed_state_port::{ActivationTimedStateQuery, ActivationTimedStateReadPort};
use aise::persistence::knowledge_read_port::{
    KnowledgeIndexQuery, KnowledgeIndexRecord, KnowledgeReadPort, KnowledgeRecord, OwnerMemoryQuery,
    SourceKnowledgeQuery,
};
use aise::persistence::store::StoreError;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000002").unwrap()
}

fn story_id() -> StoryId {
    StoryId::try_new("retrieval-story").unwrap()
}

fn snapshot() -> KnowledgeSnapshotRef {
    KnowledgeSnapshotRef {
        story_id: story_id(),
        pack_digest: digest(),
        base_revision: StoryRevision::new(3),
        knowledge_id_high_water: KnowledgeIdHighWater::zero(),
    }
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "retrieval_test", 65_536).unwrap()
}

fn fact(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Fact(FactId::try_new(format!("fact_{sequence:04}")).unwrap())
}

fn memory(sequence: u64) -> KnowledgeSourceId {
    KnowledgeSourceId::Memory(MemoryId::try_new(format!("memory_{sequence:04}")).unwrap())
}

fn seed_source() -> KnowledgeSource {
    KnowledgeSource::Seed {
        pack_id: PackId::try_new("pack").unwrap(),
        pack_digest: digest(),
    }
}

fn rule(key: &str) -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.match_rule.keys = vec![ActivationPattern::Literal(key.to_owned())];
    rule
}

struct FixtureKnowledge {
    records: BTreeMap<KnowledgeSourceId, KnowledgeRecord>,
    body_loads: AtomicUsize,
}

impl FixtureKnowledge {
    fn new(records: Vec<KnowledgeRecord>) -> Self {
        Self {
            records: records.into_iter().map(|record| (record.source_id.clone(), record)).collect(),
            body_loads: AtomicUsize::new(0),
        }
    }

    fn body_loads(&self) -> usize {
        self.body_loads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl KnowledgeReadPort for FixtureKnowledge {
    async fn find_by_source_ids(&self, query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        let mut found = Vec::new();
        for source_id in query.source_ids {
            let Some(record) = self.records.get(source_id) else {
                continue;
            };
            if !query.filter.knowledge_kinds.contains(&record.kind) {
                continue;
            }
            self.body_loads.fetch_add(1, Ordering::SeqCst);
            found.push(record.clone());
        }
        Ok(found)
    }

    async fn find_memories_by_owner(&self, query: OwnerMemoryQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(self
            .records
            .values()
            .filter(|record| record.kind == KnowledgeKind::Memory && record.memory_owner.as_ref() == Some(query.owner))
            .cloned()
            .collect())
    }

    async fn list_index(&self, _query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
        Ok(Vec::new())
    }
}

struct FixtureIndex {
    entries: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
    loads: AtomicUsize,
}

impl FixtureIndex {
    fn new(entries: Vec<ActivationEntryMetadata>) -> Self {
        Self {
            entries: entries.into_iter().map(|entry| (entry.source_id.clone(), entry)).collect(),
            loads: AtomicUsize::new(0),
        }
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ActivationIndexPort for FixtureIndex {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        _limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexMetadata>, StoreError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(ActivationIndexMetadata {
            reference: ActivationIndexSnapshotRef::from_knowledge(
                knowledge,
                1,
                aise::domain::knowledge::activation::MATCHER_VERSION,
            ),
            entries: self.entries.clone(),
        }))
    }
}

struct FixtureTimed {
    states: Vec<ActivationTimedState>,
}

#[async_trait]
impl ActivationTimedStateReadPort for FixtureTimed {
    async fn load_timed_state(
        &self,
        _query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError> {
        Ok(self.states.clone())
    }
}

struct Fixture {
    coordinator: KnowledgeActivationCoordinator,
    knowledge: Arc<FixtureKnowledge>,
    index: Arc<FixtureIndex>,
}

fn fixture() -> Fixture {
    let gate_rule = rule("gate");
    let bell_rule = rule("bell");
    let entries = vec![
        ActivationEntryMetadata {
            source_id: fact(1),
            kind: KnowledgeKind::Fact,
            rule: gate_rule.clone(),
            rule_version: ActivationRuleVersion::from_rule(&gate_rule),
            salience: 80,
            from_pack: true,
        },
        ActivationEntryMetadata {
            source_id: fact(2),
            kind: KnowledgeKind::Fact,
            rule: bell_rule.clone(),
            rule_version: ActivationRuleVersion::from_rule(&bell_rule),
            salience: 40,
            from_pack: true,
        },
    ];
    let records = vec![
        KnowledgeRecord {
            source_id: fact(1),
            kind: KnowledgeKind::Fact,
            content: text("The village gate is closed."),
            salience: 80,
            source: seed_source(),
            memory_owner: None,
            activation: Some(gate_rule),
            activation_rule_version: None,
        },
        KnowledgeRecord {
            source_id: fact(2),
            kind: KnowledgeKind::Fact,
            content: text("The bell rings at midnight."),
            salience: 40,
            source: seed_source(),
            memory_owner: None,
            activation: Some(bell_rule),
            activation_rule_version: None,
        },
        KnowledgeRecord {
            source_id: memory(1),
            kind: KnowledgeKind::Memory,
            content: text("A private memory about the gate."),
            salience: 60,
            source: seed_source(),
            memory_owner: Some(RoleId::try_new("companion").unwrap()),
            activation: None,
            activation_rule_version: None,
        },
    ];
    let knowledge = Arc::new(FixtureKnowledge::new(records));
    let index = Arc::new(FixtureIndex::new(entries));
    let timed = Arc::new(FixtureTimed { states: Vec::new() });
    let activation = ActivationConfig::default();
    let coordinator = KnowledgeActivationCoordinator::new(
        knowledge.clone(),
        index.clone(),
        timed,
        activation.domain_index_limits(),
        activation.rule,
        activation.domain_runtime_limits(),
        activation.cache,
    );
    Fixture {
        coordinator,
        knowledge,
        index,
    }
}

fn scan_buffer(value: &str) -> ActivationScanBuffer {
    ActivationScanBuffer::try_new(
        vec![ScanFragment::new(
            ScanFragmentKind::PlayerContribution,
            1,
            0,
            text(value),
        )],
        16,
        4096,
    )
    .unwrap()
}

fn macros() -> ActivationMacroValues {
    ActivationMacroValues {
        player_name: "Eirin".to_owned(),
        player_role_label: "Warden".to_owned(),
    }
}

fn spec<'a>(
    snapshot: &'a KnowledgeSnapshotRef,
    story: &'a StoryId,
    buffer: &'a ActivationScanBuffer,
    trigger: GenerationTrigger,
) -> ActivationRunSpec<'a> {
    ActivationRunSpec {
        snapshot,
        scan_buffer: buffer,
        macros: macros(),
        story_id: story,
        turn_number: TurnNumber::try_new(4).unwrap(),
        generation_trigger: trigger,
        mode: ActivationRunMode::CommitEligible,
        external_seeds: &[],
        continuation: None,
    }
}

#[tokio::test]
async fn baseline_activation_selects_matching_entries_and_loads_only_their_bodies() {
    let fixture = fixture();
    let snapshot = snapshot();
    let story = story_id();
    let buffer = scan_buffer("the gate is shut");
    let outcome = fixture
        .coordinator
        .run(spec(&snapshot, &story, &buffer, GenerationTrigger::Normal))
        .await
        .unwrap();
    assert_eq!(
        outcome
            .result
            .activated
            .iter()
            .map(|entry| entry.source_id.as_str().to_owned())
            .collect::<Vec<_>>(),
        vec!["fact_0001"]
    );
    assert_eq!(fixture.knowledge.body_loads(), 1);
}

#[tokio::test]
async fn resuming_with_a_continuation_does_not_reactivate_or_reload_bodies() {
    let fixture = fixture();
    let snapshot = snapshot();
    let story = story_id();
    let buffer = scan_buffer("the gate is shut");
    let first = fixture
        .coordinator
        .run(spec(&snapshot, &story, &buffer, GenerationTrigger::Normal))
        .await
        .unwrap();
    let loads_after_first = fixture.knowledge.body_loads();
    let mut resumed_spec = spec(&snapshot, &story, &buffer, GenerationTrigger::Normal);
    resumed_spec.continuation = Some(first.result.continuation.clone());
    let resumed = fixture.coordinator.run(resumed_spec).await.unwrap();
    assert_eq!(resumed.result.activated.len(), 1);
    assert_eq!(fixture.knowledge.body_loads(), loads_after_first);
    assert_eq!(
        resumed.result.continuation.consumed.knowledge_tokens,
        first.result.continuation.consumed.knowledge_tokens
    );
}

#[tokio::test]
async fn repeated_matches_deduplicate_into_one_activation() {
    let fixture = fixture();
    let snapshot = snapshot();
    let story = story_id();
    let buffer = ActivationScanBuffer::try_new(
        vec![
            ScanFragment::new(ScanFragmentKind::PlayerContribution, 1, 0, text("gate gate")),
            ScanFragment::new(ScanFragmentKind::NarrativeEvent, 1, 1, text("gate again")),
        ],
        16,
        4096,
    )
    .unwrap();
    let outcome = fixture
        .coordinator
        .run(spec(&snapshot, &story, &buffer, GenerationTrigger::Normal))
        .await
        .unwrap();
    assert_eq!(outcome.result.activated.len(), 1);
    assert_eq!(fixture.knowledge.body_loads(), 1);
}

#[tokio::test]
async fn repair_reuses_the_frozen_index_and_the_prior_continuation() {
    let fixture = fixture();
    let snapshot = snapshot();
    let story = story_id();
    let buffer = scan_buffer("the gate is shut");
    let first = fixture
        .coordinator
        .run(spec(&snapshot, &story, &buffer, GenerationTrigger::Normal))
        .await
        .unwrap();
    let index_loads = fixture.index.loads();
    let mut repair_spec = spec(&snapshot, &story, &buffer, GenerationTrigger::Normal);
    repair_spec.continuation = Some(first.result.continuation.clone());
    let repaired = fixture.coordinator.run(repair_spec).await.unwrap();
    assert_eq!(
        repaired
            .result
            .activated
            .iter()
            .map(|entry| entry.source_id.clone())
            .collect::<Vec<_>>(),
        first
            .result
            .activated
            .iter()
            .map(|entry| entry.source_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(fixture.index.loads() > index_loads);
}

#[tokio::test]
async fn memory_entries_are_never_activated_and_stay_owner_scoped() {
    let fixture = fixture();
    let snapshot = snapshot();
    let story = story_id();
    let buffer = scan_buffer("a private memory about the gate");
    let outcome = fixture
        .coordinator
        .run(spec(&snapshot, &story, &buffer, GenerationTrigger::Normal))
        .await
        .unwrap();
    assert!(
        outcome
            .result
            .activated
            .iter()
            .all(|entry| !matches!(entry.source_id, KnowledgeSourceId::Memory(_)))
    );
    let owner = RoleId::try_new("companion").unwrap();
    let owned = fixture
        .knowledge
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &owner,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .unwrap();
    assert_eq!(owned.len(), 1);
    let stranger = RoleId::try_new("stranger").unwrap();
    let none = fixture
        .knowledge
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &stranger,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .unwrap();
    assert!(none.is_empty());
}
