use crate::harness::{build_index, entry, fact, index_limits, knowledge_snapshot, rule, story_id, writer};
use aise::context::activation::seed_provider::{ActivationSeedProvider, ActivationSeedRequest, ProviderError};
use aise::domain::ids::TurnNumber;
use aise::domain::knowledge::activation::{
    ActivationSeedKind, ExternalActivationSeed, GenerationTrigger, ScanFragment,
};
use async_trait::async_trait;

struct StaticProvider {
    seeds: Vec<ExternalActivationSeed>,
}

#[async_trait]
impl ActivationSeedProvider for StaticProvider {
    fn name(&self) -> &'static str {
        "static"
    }

    async fn seeds(&self, _request: ActivationSeedRequest<'_>) -> Result<Vec<ExternalActivationSeed>, ProviderError> {
        Ok(self.seeds.clone())
    }
}

struct FailingProvider;

#[async_trait]
impl ActivationSeedProvider for FailingProvider {
    fn name(&self) -> &'static str {
        "failing"
    }

    async fn seeds(&self, _request: ActivationSeedRequest<'_>) -> Result<Vec<ExternalActivationSeed>, ProviderError> {
        Err(ProviderError::Unavailable)
    }
}

#[tokio::test]
async fn providers_return_bounded_authorized_seeds() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let index = build_index(&entries, index_limits()).unwrap();
    let snapshot = knowledge_snapshot();
    let story = story_id();
    let fragments: Vec<ScanFragment> = Vec::new();
    let provider = StaticProvider {
        seeds: vec![ExternalActivationSeed {
            source_id: fact(1),
            delivery: writer(),
            kind: ActivationSeedKind::Provider,
            provider_rank: Some(1),
            mandatory: false,
        }],
    };
    let produced = provider
        .seeds(ActivationSeedRequest {
            story_id: &story,
            turn_number: TurnNumber::try_new(4).unwrap(),
            generation_trigger: GenerationTrigger::Normal,
            knowledge_snapshot: &snapshot,
            index_snapshot: &index,
            scan_fragments: &fragments,
            max_seeds: 8,
        })
        .await
        .unwrap();
    assert_eq!(produced.len(), 1);
    assert_eq!(produced[0].kind, ActivationSeedKind::Provider);
    assert!(produced.len() <= 8);
}

#[tokio::test]
async fn provider_failures_surface_as_provider_errors() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let index = build_index(&entries, index_limits()).unwrap();
    let snapshot = knowledge_snapshot();
    let story = story_id();
    let fragments: Vec<ScanFragment> = Vec::new();
    let error = FailingProvider
        .seeds(ActivationSeedRequest {
            story_id: &story,
            turn_number: TurnNumber::try_new(4).unwrap(),
            generation_trigger: GenerationTrigger::Normal,
            knowledge_snapshot: &snapshot,
            index_snapshot: &index,
            scan_fragments: &fragments,
            max_seeds: 8,
        })
        .await
        .unwrap_err();
    assert_eq!(error, ProviderError::Unavailable);
    assert_eq!(FailingProvider.name(), "failing");
}
