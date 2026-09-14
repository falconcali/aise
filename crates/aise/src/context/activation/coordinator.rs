use crate::config::{ActivationRuleLimitsConfig, FragmentMatchCacheLimits};
use crate::context::activation::index::{
    ActivationOverlayIndex, FragmentMatchCache, FragmentMatchCacheValue, FrozenPackIndexCache,
    InMemoryFragmentMatchCache, build_overlay_index, compose_index_snapshot, estimate_match_bytes, fragment_cache_key,
};
use crate::context::activation::seed_provider::{ActivationSeedProvider, ActivationSeedRequest};
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::knowledge::activation::{
    ActivationContinuation, ActivationEntryBody, ActivationError, ActivationFragmentMatches, ActivationIndexLimits,
    ActivationIndexSnapshot, ActivationMacroValues, ActivationRecursionInput, ActivationRequest, ActivationResult,
    ActivationRunMode, ActivationRuntimeLimits, ActivationScanBuffer, ActivationStoreFailure, ExternalActivationSeed,
    FrozenPackIndex, FrozenPackIndexKey, GenerationTrigger, KnowledgeActivationSession, MATCHER_VERSION,
    build_frozen_pack_index, macro_digest,
};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::knowledge_read_port::{KnowledgeFilter, SourceKnowledgeQuery};
use crate::persistence::{
    ActivationIndexPort, ActivationTimedStateQuery, ActivationTimedStateReadPort, KnowledgeReadPort, StoreError,
};
use std::sync::Arc;
use tracing::{Instrument, info_span};

pub struct ActivationRunOutcome {
    pub result: ActivationResult,
    pub index_snapshot: Arc<ActivationIndexSnapshot>,
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
    seed_providers: Vec<Arc<dyn ActivationSeedProvider>>,
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
            seed_providers: Vec::new(),
            fragment_cache: Arc::new(InMemoryFragmentMatchCache::new(cache_limits)),
            pack_cache: Arc::new(FrozenPackIndexCache::new(cache_limits.max_cached_stories)),
            index_limits,
            rule_limits,
            runtime_limits,
        }
    }

    pub fn with_seed_providers(mut self, providers: Vec<Arc<dyn ActivationSeedProvider>>) -> Self {
        self.seed_providers = providers;
        self
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

    pub async fn build_index_snapshot(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: &ActivationMacroValues,
    ) -> Result<Arc<ActivationIndexSnapshot>, ActivationError> {
        let span = info_span!(
            "knowledge.activation.index",
            story_id = %snapshot.story_id,
            base_revision = snapshot.base_revision.get(),
            matcher_version = MATCHER_VERSION,
        );
        async move {
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
            let pack_index = match self.pack_cache.get(&key) {
                Some(cached) => cached,
                None => {
                    let built: Arc<FrozenPackIndex> = Arc::new(build_frozen_pack_index(
                        key,
                        metadata.entries.values().filter(|entry| entry.from_pack),
                        macros,
                        self.index_limits,
                        self.rule_limits.limits(),
                    )?);
                    self.pack_cache.insert(built.clone());
                    built
                }
            };
            let overlay: ActivationOverlayIndex = build_overlay_index(
                metadata.reference.overlay_version,
                &metadata.entries,
                macros,
                self.index_limits,
                self.rule_limits.limits(),
            )?;
            tracing::debug!(
                literal_patterns = pack_index.literal_index.len(),
                regex_patterns = pack_index.regex_set.len(),
                overlay_literal_patterns = overlay.literal_index.len(),
                overlay_regex_patterns = overlay.regex_set.len(),
                "activation index composed"
            );
            Ok(Arc::new(compose_index_snapshot(
                metadata.reference.clone(),
                metadata.entries.clone(),
                &pack_index,
                &overlay,
            )))
        }
        .instrument(span)
        .await
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
                matches.insert(fragment.id.clone(), cached.matches);
                continue;
            }
            let computed = Arc::new(index.match_fragment(fragment));
            self.fragment_cache.insert(
                key,
                FragmentMatchCacheValue {
                    matches: computed.clone(),
                    estimated_bytes: estimate_match_bytes(&computed),
                },
            );
            matches.insert(fragment.id.clone(), computed);
        }
        tracing::debug!(cache_hits, total = matches.len(), "activation fragment matching complete");
        matches
    }

    pub async fn collect_seeds(
        &self,
        request: ActivationSeedRequest<'_>,
        base: &[ExternalActivationSeed],
    ) -> Result<Vec<ExternalActivationSeed>, ActivationError> {
        let mut seeds = base.to_vec();
        for provider in &self.seed_providers {
            let span = info_span!("knowledge.activation.provider", provider = provider.name());
            let produced = provider
                .seeds(ActivationSeedRequest {
                    story_id: request.story_id,
                    turn_number: request.turn_number,
                    generation_trigger: request.generation_trigger,
                    knowledge_snapshot: request.knowledge_snapshot,
                    index_snapshot: request.index_snapshot,
                    scan_fragments: request.scan_fragments,
                    max_seeds: request.max_seeds,
                })
                .instrument(span)
                .await
                .map_err(|_| ActivationError::ProviderFailure {
                    provider: provider.name(),
                })?;
            if produced.len() > request.max_seeds {
                return Err(ActivationError::ProviderFailure {
                    provider: provider.name(),
                });
            }
            for seed in &produced {
                if !self.authorize_seed(request.index_snapshot, &seed.source_id, &seed.delivery) {
                    return Err(ActivationError::ExternalTargetUnauthorized);
                }
            }
            seeds.extend(produced);
            if seeds.len() > self.runtime_limits.max_external_candidates {
                return Err(ActivationError::WorkLimitExceeded {
                    limit: "external_candidates",
                });
            }
        }
        Ok(seeds)
    }

    pub async fn run(&self, spec: ActivationRunSpec<'_>) -> Result<ActivationRunOutcome, ActivationError> {
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
        let index = self.build_index_snapshot(snapshot, &macros).await?;
        let digest = macro_digest(&macros);
        let fragment_matches = self.match_fragments(story_id, &index, &digest, scan_buffer);
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot,
                limit: self.runtime_limits.max_activated_entries,
            })
            .await
            .map_err(map_store_error)?;
        let seeds = self
            .collect_seeds(
                ActivationSeedRequest {
                    story_id,
                    turn_number,
                    generation_trigger,
                    knowledge_snapshot: snapshot,
                    index_snapshot: &index,
                    scan_fragments: scan_buffer.fragments(),
                    max_seeds: self.runtime_limits.max_external_candidates,
                },
                external_seeds,
            )
            .await?;
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
            external_seeds: &seeds,
            continuation,
            limits: self.runtime_limits,
        };
        let result = self.drive(request, snapshot).await?;
        Ok(ActivationRunOutcome {
            result,
            index_snapshot: index,
        })
    }

    pub async fn drive(
        &self,
        request: ActivationRequest<'_>,
        snapshot: &KnowledgeSnapshotRef,
    ) -> Result<ActivationResult, ActivationError> {
        let span = info_span!(
            "knowledge.activation.rounds",
            story_id = %request.story_id,
            turn_number = request.turn_number.get(),
        );
        let max_item_bytes = self.runtime_limits.max_single_entry_bytes;
        async move {
            let mut session = KnowledgeActivationSession::start(request)?;
            let mut rounds = 0u32;
            while let Some(outcome) = session.next_round()? {
                rounds = rounds.saturating_add(1);
                let bodies = if outcome.admitted.is_empty() {
                    Vec::new()
                } else {
                    self.load_bodies(snapshot, &outcome.admitted, max_item_bytes, &mut session)
                        .await?
                };
                session.supply_bodies(ActivationRecursionInput { bodies })?;
            }
            tracing::debug!(rounds, "activation rounds complete");
            let span = info_span!("knowledge.activation.budget");
            let _guard = span.enter();
            session.finish()
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
    ) -> Result<Vec<ActivationEntryBody>, ActivationError> {
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
                .map_err(map_store_error)
        }
        .instrument(span)
        .await?;
        let mut bodies = Vec::with_capacity(records.len());
        for record in records {
            bodies.push(ActivationEntryBody {
                source_id: record.source_id.clone(),
                kind: record.source_id.kind(),
                token_cost: estimate_text_tokens(record.content.as_str()).max(1),
                body: record.content,
            });
        }
        for source_id in admitted {
            if !bodies.iter().any(|body| &body.source_id == source_id) {
                session.drop_admitted(source_id)?;
            }
        }
        Ok(bodies)
    }
}

fn map_store_error(error: StoreError) -> ActivationError {
    ActivationError::Store(match error {
        StoreError::RevisionConflict => ActivationStoreFailure::RevisionConflict,
        StoreError::NotFound => ActivationStoreFailure::NotFound,
        StoreError::LimitExceeded { .. } => ActivationStoreFailure::LimitExceeded,
        StoreError::Serialization { .. } => ActivationStoreFailure::Serialization,
        _ => ActivationStoreFailure::Unavailable,
    })
}
