use crate::config::{ActivationRuleLimitsConfig, FragmentMatchCacheLimits};
use crate::context::activation::fragment_cache::{
    FragmentMatchCache, FragmentMatchCacheValue, LruFragmentMatchCache, fragment_cache_key,
};
use crate::context::activation::index::{
    ActivationOverlayIndex, FrozenPackIndexCache, build_overlay_index, compose_index_snapshot,
};
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::knowledge::activation::{
    ActivationContinuation, ActivationEntryBody, ActivationError, ActivationFragmentMatches, ActivationIndexLimits,
    ActivationIndexSnapshot, ActivationMacroValues, ActivationRecursionInput, ActivationRequest, ActivationResult,
    ActivationRunMode, ActivationRuntimeLimits, ActivationScanBuffer, ActivationStoreFailure, ExternalActivationSeed,
    FrozenPackIndex, FrozenPackIndexKey, GenerationTrigger, KnowledgeActivationSession, LoadedActivationEntry,
    MATCHER_VERSION, build_frozen_pack_index, macro_digest,
};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::knowledge_read_port::{KnowledgeFilter, SourceKnowledgeQuery};
use crate::persistence::{
    ActivationIndexPort, ActivationTimedStateQuery, ActivationTimedStateReadPort, KnowledgeReadPort, StoreError,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{Instrument, info_span};

pub struct ActivationRunOutcome {
    pub result: ActivationResult,
    pub index_snapshot: Arc<ActivationIndexSnapshot>,
    pub loaded_entries: BTreeMap<KnowledgeSourceId, LoadedActivationEntry>,
}

pub fn activation_provider_span(
    story_id: &StoryId,
    turn_number: TurnNumber,
    provider_id: &'static str,
    requested_limit: usize,
) -> tracing::Span {
    info_span!(
        "knowledge.activation.provider",
        story_id = %story_id,
        turn_number = turn_number.get(),
        provider_id,
        requested_limit,
        candidate_count = tracing::field::Empty,
        status = tracing::field::Empty,
        error_code = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    )
}

pub struct ActivationRunSpec<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub scan_buffer: &'a ActivationScanBuffer,
    pub macros: ActivationMacroValues,
    pub story_id: &'a StoryId,
    pub turn_number: TurnNumber,
    pub generation_trigger: GenerationTrigger,
    pub mode: ActivationRunMode,
    pub external_seeds: &'a [ExternalActivationSeed],
    pub continuation: Option<ActivationContinuation>,
}

pub struct KnowledgeActivationCoordinator {
    knowledge: Arc<dyn KnowledgeReadPort>,
    index: Arc<dyn ActivationIndexPort>,
    timed_state: Arc<dyn ActivationTimedStateReadPort>,
    fragment_cache: Arc<dyn FragmentMatchCache>,
    pack_cache: Arc<FrozenPackIndexCache>,
    index_limits: ActivationIndexLimits,
    rule_limits: ActivationRuleLimitsConfig,
    runtime_limits: ActivationRuntimeLimits,
}

impl KnowledgeActivationCoordinator {
    pub fn new(
        knowledge: Arc<dyn KnowledgeReadPort>,
        index: Arc<dyn ActivationIndexPort>,
        timed_state: Arc<dyn ActivationTimedStateReadPort>,
        index_limits: ActivationIndexLimits,
        rule_limits: ActivationRuleLimitsConfig,
        runtime_limits: ActivationRuntimeLimits,
        cache_limits: FragmentMatchCacheLimits,
    ) -> Self {
        Self {
            knowledge,
            index,
            timed_state,
            fragment_cache: Arc::new(LruFragmentMatchCache::new(cache_limits)),
            pack_cache: Arc::new(FrozenPackIndexCache::new(
                cache_limits.max_cached_stories,
                cache_limits.max_total_estimated_bytes,
            )),
            index_limits,
            rule_limits,
            runtime_limits,
        }
    }

    pub fn with_fragment_cache(mut self, cache: Arc<dyn FragmentMatchCache>) -> Self {
        self.fragment_cache = cache;
        self
    }

    pub fn runtime_limits(&self) -> ActivationRuntimeLimits {
        self.runtime_limits
    }

    pub fn knowledge(&self) -> &Arc<dyn KnowledgeReadPort> {
        &self.knowledge
    }

    pub fn fragment_cache(&self) -> &Arc<dyn FragmentMatchCache> {
        &self.fragment_cache
    }

    pub fn authorize_seed(
        &self,
        index: &ActivationIndexSnapshot,
        source_id: &KnowledgeSourceId,
        delivery: &KnowledgeDelivery,
    ) -> bool {
        let Some(metadata) = index.metadata.get(source_id) else {
            return false;
        };
        match delivery {
            KnowledgeDelivery::Writer => matches!(metadata.kind, KnowledgeKind::Fact | KnowledgeKind::Rumor),
            KnowledgeDelivery::Character { .. } => metadata.kind == KnowledgeKind::Rumor,
        }
    }

    pub async fn prepare_index(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: &ActivationMacroValues,
    ) -> Result<Arc<ActivationIndexSnapshot>, ActivationError> {
        self.prepare_index_observed(
            snapshot,
            macros,
            TurnNumber::try_new(1).map_err(|_| ActivationError::InvalidRule {
                code: "turn_number_invalid",
            })?,
            GenerationTrigger::DryRunPreview,
            ActivationRunMode::Preview,
        )
        .await
    }

    pub(crate) async fn prepare_index_for_run(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: &ActivationMacroValues,
        turn_number: TurnNumber,
        generation_trigger: GenerationTrigger,
        mode: ActivationRunMode,
    ) -> Result<Arc<ActivationIndexSnapshot>, ActivationError> {
        self.prepare_index_observed(snapshot, macros, turn_number, generation_trigger, mode)
            .await
    }

    async fn prepare_index_observed(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: &ActivationMacroValues,
        turn_number: TurnNumber,
        generation_trigger: GenerationTrigger,
        mode: ActivationRunMode,
    ) -> Result<Arc<ActivationIndexSnapshot>, ActivationError> {
        if macros.player_name.len() > self.rule_limits.max_macro_value_bytes
            || macros.player_role_label.len() > self.rule_limits.max_macro_value_bytes
        {
            return Err(ActivationError::InvalidRule {
                code: "macro_value_too_long",
            });
        }
        let started = std::time::Instant::now();
        let span = info_span!(
            "knowledge.activation.prepare",
            story_id = %snapshot.story_id,
            turn_number = turn_number.get(),
            base_revision = snapshot.base_revision.get(),
            pack_digest = %snapshot.pack_digest,
            overlay_version = tracing::field::Empty,
            matcher_version = MATCHER_VERSION,
            generation_trigger = ?generation_trigger,
            mode = ?mode,
            index_entries = tracing::field::Empty,
            overlay_entries = tracing::field::Empty,
            tombstones = tracing::field::Empty,
            literal_patterns = tracing::field::Empty,
            regex_patterns = tracing::field::Empty,
            compiled_bytes = tracing::field::Empty,
            frozen_cache_hit = tracing::field::Empty,
            timed_state_entries = tracing::field::Empty,
            status = tracing::field::Empty,
            error_code = tracing::field::Empty,
            latency_ms = tracing::field::Empty,
        );
        let outcome: Result<Arc<ActivationIndexSnapshot>, ActivationError> = async move {
            let metadata = self
                .index
                .load_snapshot(snapshot, self.index_limits)
                .await
                .map_err(map_store_error)?;
            let digest = macro_digest(macros);
            let key = FrozenPackIndexKey {
                pack_digest: snapshot.pack_digest.clone(),
                macro_digest: digest,
                matcher_version: MATCHER_VERSION,
            };
            let (pack_index, frozen_cache_hit) = match self.pack_cache.get(&key) {
                Some(cached) => (cached, true),
                None => {
                    let built: Arc<FrozenPackIndex> = Arc::new(build_frozen_pack_index(
                        key,
                        metadata.pack_entries.values(),
                        macros,
                        self.index_limits,
                        self.rule_limits.limits(),
                    )?);
                    self.pack_cache.insert(built.clone())?;
                    (built, false)
                }
            };
            let overlay: ActivationOverlayIndex = build_overlay_index(
                metadata.reference.overlay_version,
                &metadata.entries,
                &pack_index,
                macros,
                self.index_limits,
                self.rule_limits.limits(),
            )?;
            let current = tracing::Span::current();
            current.record("overlay_version", metadata.reference.overlay_version);
            current.record("index_entries", metadata.entries.len());
            current.record("overlay_entries", overlay.upserts.len());
            current.record("tombstones", overlay.tombstones.len());
            current.record(
                "literal_patterns",
                pack_index.literal_index.len().saturating_add(overlay.literal_index.len()),
            );
            current.record(
                "regex_patterns",
                pack_index.regex_set.len().saturating_add(overlay.regex_set.len()),
            );
            current.record(
                "compiled_bytes",
                pack_index
                    .estimated_bytes()
                    .saturating_add(overlay.literal_index.compiled_bytes())
                    .saturating_add(overlay.regex_set.compiled_bytes()),
            );
            current.record("frozen_cache_hit", frozen_cache_hit);
            Ok(Arc::new(compose_index_snapshot(
                metadata.reference.clone(),
                &pack_index,
                &overlay,
                self.index_limits,
            )?))
        }
        .instrument(span.clone())
        .await;
        span.record("latency_ms", started.elapsed().as_millis() as u64);
        match &outcome {
            Ok(_) => {
                span.record("status", "ok");
            }
            Err(error) => {
                span.record("status", "error");
                span.record("error_code", error.code());
            }
        }
        outcome
    }

    pub fn match_fragments(
        &self,
        story_id: &StoryId,
        index: &ActivationIndexSnapshot,
        macro_digest: &Sha256Digest,
        scan_buffer: &ActivationScanBuffer,
    ) -> ActivationFragmentMatches {
        let span = info_span!(
            "knowledge.activation.scan",
            story_id = %story_id,
            fragments = scan_buffer.fragments().len(),
        );
        let _guard = span.enter();
        let mut matches = ActivationFragmentMatches::new();
        let mut cache_hits = 0usize;
        for fragment in scan_buffer.fragments() {
            let key = fragment_cache_key(story_id, &index.reference, macro_digest, fragment);
            if let Some(cached) = self.fragment_cache.get(&key) {
                cache_hits = cache_hits.saturating_add(1);
                matches.insert(fragment.id.clone(), Arc::new(cached.matches.clone()));
                continue;
            }
            let computed = index.match_fragment(fragment);
            if let Err(error) = self.fragment_cache.insert(
                key,
                FragmentMatchCacheValue {
                    matches: computed.clone(),
                },
            ) {
                tracing::warn!(error_code = error.code(), "activation fragment cache insert failed");
            }
            matches.insert(fragment.id.clone(), Arc::new(computed));
        }
        tracing::debug!(cache_hits, total = matches.len(), "activation fragment matching complete");
        matches
    }

    pub async fn run(&self, spec: ActivationRunSpec<'_>) -> Result<ActivationRunOutcome, ActivationError> {
        let index = self
            .prepare_index_for_run(
                spec.snapshot,
                &spec.macros,
                spec.turn_number,
                spec.generation_trigger,
                spec.mode,
            )
            .await?;
        self.run_with_index(spec, index).await
    }

    pub async fn run_with_index(
        &self,
        spec: ActivationRunSpec<'_>,
        index: Arc<ActivationIndexSnapshot>,
    ) -> Result<ActivationRunOutcome, ActivationError> {
        let ActivationRunSpec {
            snapshot,
            scan_buffer,
            macros,
            story_id,
            turn_number,
            generation_trigger,
            mode,
            external_seeds,
            continuation,
        } = spec;
        if !index.matches_snapshot(snapshot, MATCHER_VERSION) {
            return Err(ActivationError::SnapshotMismatch);
        }
        if continuation.is_some() {
            info_span!(
                "knowledge.activation.resume",
                story_id = %story_id,
                turn_number = turn_number.get(),
                base_revision = snapshot.base_revision.get(),
                pack_digest = %snapshot.pack_digest,
                overlay_version = index.reference.overlay_version,
                external_seeds = external_seeds.len(),
                authorized_seeds = external_seeds.len(),
                newly_activated = tracing::field::Empty,
                consumed_items = tracing::field::Empty,
                consumed_tokens = tracing::field::Empty,
                status = tracing::field::Empty,
                error_code = tracing::field::Empty,
                latency_ms = tracing::field::Empty,
            )
            .in_scope(|| ());
        }
        let digest = macro_digest(&macros);
        let fragment_matches = self.match_fragments(story_id, &index, &digest, scan_buffer);
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot,
                limit: self.index_limits.max_entries,
            })
            .await
            .map_err(map_store_error)?;
        if external_seeds.len() > self.runtime_limits.max_external_candidates {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "external_candidates",
            });
        }
        let request = ActivationRequest {
            story_id,
            turn_number,
            generation_trigger,
            mode,
            knowledge_snapshot: snapshot,
            index_snapshot: &index,
            scan_buffer,
            fragment_matches: &fragment_matches,
            timed_state: &timed_state,
            external_seeds,
            continuation,
            limits: self.runtime_limits,
        };
        let (result, mut loaded_entries) = self.drive_with_loaded(request, snapshot).await?;
        loaded_entries.retain(|source_id, _| result.activated.iter().any(|item| &item.source_id == source_id));
        Ok(ActivationRunOutcome {
            result,
            index_snapshot: index,
            loaded_entries,
        })
    }

    pub async fn drive(
        &self,
        request: ActivationRequest<'_>,
        snapshot: &KnowledgeSnapshotRef,
    ) -> Result<ActivationResult, ActivationError> {
        self.drive_with_loaded(request, snapshot).await.map(|outcome| outcome.0)
    }

    async fn drive_with_loaded(
        &self,
        request: ActivationRequest<'_>,
        snapshot: &KnowledgeSnapshotRef,
    ) -> Result<(ActivationResult, BTreeMap<KnowledgeSourceId, LoadedActivationEntry>), ActivationError> {
        let span = info_span!(
            "knowledge.activation.rounds",
            story_id = %request.story_id,
            turn_number = request.turn_number.get(),
        );
        let max_item_bytes = self.runtime_limits.max_single_entry_bytes;
        let story_id = request.story_id.clone();
        let turn_number = request.turn_number;
        async move {
            let mut session = KnowledgeActivationSession::start(request)?;
            let mut rounds = 0u32;
            let mut loaded_entries = BTreeMap::new();
            while let Some(outcome) = session.next_round()? {
                rounds = rounds.saturating_add(1);
                let round_span = info_span!(
                    "knowledge.activation.round",
                    story_id = %story_id,
                    turn_number = turn_number.get(),
                    state = ?outcome.state,
                    round = rounds,
                    recursion_level = tracing::field::Empty,
                    scan_depth = tracing::field::Empty,
                    scan_fragments = tracing::field::Empty,
                    scan_bytes = tracing::field::Empty,
                    scan_tokens = tracing::field::Empty,
                    literal_matches = tracing::field::Empty,
                    regex_matches = tracing::field::Empty,
                    pattern_matches = tracing::field::Empty,
                    cache_hits = tracing::field::Empty,
                    cache_misses = tracing::field::Empty,
                    candidates = outcome.admitted.len(),
                    activated = tracing::field::Empty,
                    rejected = tracing::field::Empty,
                    rejected_disabled = tracing::field::Empty,
                    rejected_scope_mismatch = tracing::field::Empty,
                    rejected_delayed = tracing::field::Empty,
                    rejected_cooldown = tracing::field::Empty,
                    rejected_recursion_excluded = tracing::field::Empty,
                    rejected_recursion_level_locked = tracing::field::Empty,
                    rejected_secondary_condition = tracing::field::Empty,
                    rejected_group_loser = tracing::field::Empty,
                    rejected_probability = tracing::field::Empty,
                    rejected_budget = tracing::field::Empty,
                    rejected_duplicate = tracing::field::Empty,
                    rejected_work_limit = tracing::field::Empty,
                    knowledge_tokens = tracing::field::Empty,
                    stop_reason = tracing::field::Empty,
                    status = tracing::field::Empty,
                    error_code = tracing::field::Empty,
                );
                let round_result = async {
                    let (bodies, loaded) = if outcome.admitted.is_empty() {
                        (Vec::new(), Vec::new())
                    } else {
                        self.load_bodies(snapshot, &outcome.admitted, max_item_bytes, &mut session)
                            .await?
                    };
                    session.supply_bodies(ActivationRecursionInput { bodies })?;
                    Ok::<_, ActivationError>(loaded)
                }
                .instrument(round_span.clone())
                .await;
                match &round_result {
                    Ok(_) => {
                        round_span.record("status", "ok");
                    }
                    Err(error) => {
                        round_span.record("status", "error");
                        round_span.record("error_code", error.code());
                    }
                }
                for entry in round_result? {
                    loaded_entries.insert(entry.body.source_id.clone(), entry);
                }
            }
            tracing::debug!(rounds, "activation rounds complete");
            let span = info_span!("knowledge.activation.budget");
            let _guard = span.enter();
            session.finish().map(|result| (result, loaded_entries))
        }
        .instrument(span)
        .await
    }

    async fn load_bodies(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        admitted: &[KnowledgeSourceId],
        max_item_bytes: usize,
        session: &mut KnowledgeActivationSession<'_>,
    ) -> Result<(Vec<ActivationEntryBody>, Vec<LoadedActivationEntry>), ActivationError> {
        let span = info_span!("knowledge.activation.bodies", requested = admitted.len());
        let filter = KnowledgeFilter {
            delivery: KnowledgeDelivery::Writer,
            knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
            max_item_bytes,
        };
        let records = async {
            self.knowledge
                .find_by_source_ids(SourceKnowledgeQuery {
                    snapshot,
                    filter: &filter,
                    source_ids: admitted,
                    limit: admitted.len(),
                })
                .await
        }
        .instrument(span)
        .await;
        let records = match records {
            Ok(records) => records,
            Err(error) => {
                if admitted.iter().try_fold(false, |mandatory, source_id| {
                    session.admitted_is_mandatory(source_id).map(|value| mandatory || value)
                })? {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                for source_id in admitted {
                    session.drop_admitted(source_id)?;
                }
                tracing::warn!(error = %error, "non-mandatory activation body load failed");
                return Ok((Vec::new(), Vec::new()));
            }
        };
        let mut bodies = Vec::with_capacity(records.len());
        let mut loaded = Vec::with_capacity(records.len());
        for record in records {
            if !admitted.contains(&record.source_id) || record.kind != record.source_id.kind() {
                return Err(ActivationError::SnapshotMismatch);
            }
            let body = ActivationEntryBody {
                source_id: record.source_id.clone(),
                kind: record.kind,
                token_cost: estimate_text_tokens(record.content.as_str()).max(1),
                body: record.content.clone(),
            };
            loaded.push(LoadedActivationEntry {
                body: body.clone(),
                salience: record.salience,
                source: record.source,
                activation: record.activation,
                activation_rule_version: record.activation_rule_version,
            });
            bodies.push(body);
        }
        for source_id in admitted {
            if !bodies.iter().any(|body| &body.source_id == source_id) {
                if session.admitted_is_mandatory(source_id)? {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                session.drop_admitted(source_id)?;
            }
        }
        Ok((bodies, loaded))
    }
}

fn map_store_error(error: StoreError) -> ActivationError {
    ActivationError::Store(match error {
        StoreError::RevisionConflict => ActivationStoreFailure::RevisionConflict,
        StoreError::NotFound => ActivationStoreFailure::NotFound,
        StoreError::LimitExceeded { limit } => return ActivationError::WorkLimitExceeded { limit },
        StoreError::Serialization { .. } => ActivationStoreFailure::Serialization,
        _ => ActivationStoreFailure::Unavailable,
    })
}

#[cfg(test)]
#[path = "tests/coordinator_tests.rs"]
mod tests;
