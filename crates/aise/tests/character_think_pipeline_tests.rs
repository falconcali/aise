use aise::character::CharacterThinkPipeline;
use aise::config::{LlmConfig, TurnConfig, TurnContentLimitsConfig};
use aise::core::turn_budget::TurnBudget;
use aise::core::turn_contract::{IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest};
use aise::core::turn_data::{BaselineContext, StoryGoal, WriterPlan};
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::domain::character::CharacterState;
use aise::domain::ids::{CharacterId, StoryId, StoryRevision, TurnId};
use aise::domain::story_state::{AuthoritativeStoryState, PlayerStoryState, StoryReadSnapshot};
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::{LlmProtocolErrorKind, LlmProviderError};
use aise::llm::gateway::LlmGateway;
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct ThoughtStub {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmProvider for ThoughtStub {
    fn provider_name(&self) -> &'static str {
        "thought-stub"
    }

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletion {
            text: r#"{"perception":"sees someone","emotion":"cautious","goal":"learn the truth","possible_action":"greet"}"#
                .into(),
            finish_reason: Some(FinishReason::Stop),
            reasoning_content: None,
            usage: Some(LlmTokenUsage {
                input_tokens: 5,
                cached_input_tokens: None,
                output_tokens: 5,
                reasoning_tokens: None,
                total_tokens: 10,
                accuracy: UsageAccuracy::Exact,
            }),
            charge: None,
        })
    }

    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
        _on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

fn budget() -> TurnBudget {
    let config = TurnConfig {
        max_repair_rounds: 3,
        max_llm_calls: 8,
        max_input_tokens: 8_192,
        max_output_tokens: 2_048,
        max_total_tokens: 10_240,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    TurnBudget::from_config(&config, &TurnContentLimitsConfig::default()).unwrap()
}

fn prepared_ctx(character_ids: &[&str], requested: &[&str]) -> aise::core::turn_context::TurnExecutionContext {
    let mut context = aise::core::turn_context::TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::try_new("story-1").unwrap(),
            TurnId::try_new("turn-1").unwrap(),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            1000,
        ),
        TurnRequest::try_new("hi".to_string()).unwrap(),
        budget(),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    context.complete_initialization().unwrap();
    let snapshot = StoryReadSnapshot::new(
        StoryId::try_new("story-1").unwrap(),
        StoryRevision::new(0),
        AuthoritativeStoryState::default(),
        PlayerStoryState::default(),
        None,
        Vec::new(),
        Vec::new(),
    );
    let baseline = BaselineContext {
        relevant_characters: character_ids
            .iter()
            .map(|id| CharacterState {
                id: CharacterId::from(*id),
                name: id.to_string(),
                bio: String::new(),
                internal_state: Default::default(),
            })
            .collect(),
        ..BaselineContext::default()
    };
    context.set_prepared_context(snapshot, baseline).unwrap();
    let plan = WriterPlan {
        retrieval_requests: Vec::new(),
        character_requests: requested.iter().map(|id| CharacterId::from(*id)).collect(),
        story_goal: StoryGoal::default(),
    };
    context.set_writer_plan(plan).unwrap();
    context
}

#[tokio::test]
async fn unknown_character_request_is_skipped() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(ThoughtStub { calls: calls.clone() });
    let config = LlmConfig {
        base_url: "http://localhost:9999/v1".into(),
        model: "test".into(),
        max_concurrent: 1,
        ..LlmConfig::default()
    };
    let gateway = Arc::new(LlmGateway::new(provider, config).expect("gateway"));
    let pipeline = CharacterThinkPipeline::new(gateway);
    let mut context = prepared_ctx(&["char-1"], &["char-1", "stranger"]);
    pipeline
        .execute(&mut context)
        .await
        .expect("unknown character request must not fail the turn");
    let thoughts = context.thoughts();
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0].character_id.as_str(), "char-1");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "only the known character triggers an LLM call");
}

#[tokio::test]
async fn only_unknown_character_requests_are_skipped() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(ThoughtStub { calls: calls.clone() });
    let config = LlmConfig {
        base_url: "http://localhost:9999/v1".into(),
        model: "test".into(),
        max_concurrent: 1,
        ..LlmConfig::default()
    };
    let gateway = Arc::new(LlmGateway::new(provider, config).expect("gateway"));
    let pipeline = CharacterThinkPipeline::new(gateway);
    let mut context = prepared_ctx(&["char-1"], &["stranger"]);
    pipeline
        .execute(&mut context)
        .await
        .expect("unknown character request must not fail the turn");
    assert!(context.thoughts().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no LLM call when every request is unknown");
}
