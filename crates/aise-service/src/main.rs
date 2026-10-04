#![forbid(unsafe_code)]

use aise_core::core::{
    EngineError, IdempotencyKey, StoryId, TurnCancellation, TurnControl, TurnEvent, TurnEventDeliveryError,
    TurnEventSink, TurnRequest, TurnResult,
};
use aise_core::engine::{AiseEngine, Engine};
use aise_core::llm::{LlmConfig, LlmGateway, OpenAiCompatProvider};
use aise_core::prompt::{Prompt, PromptConfig};
use aise_core::trace::{
    Attribute, ContentCapture, ObservabilityContentConfig, ObservationError, ObservationSession, ObservationStatus,
    SessionOutcome, SessionSpec, TRACE_ENVIRONMENT, TRACE_METADATA_STORY_ID, TRACE_METADATA_TURN_NUMBER, TRACE_RELEASE,
    Trace, TraceOutcome, TraceSpec,
};
use anyhow::Context;
use observability::{DETECTOR_OVERLAP_BYTES, ObservabilityConfig, ObservabilityRuntime, TelemetryDiagnostics};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;
use uuid::Uuid;

pub mod observability;

const DEFAULT_STORY_ID: &str = "default-story";
const DEFAULT_IDEMPOTENCY_KEY: &str = "default-turn";
const DEFAULT_PLAYER_INPUT: &str = "继续这个故事。";
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_LLM_TEMPERATURE: f32 = 0.7;
const DEFAULT_LLM_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../aise-core/assets/prompts");
const DEFAULT_PROMPT_MAX_PROMPTS: usize = 16;
const DEFAULT_PROMPT_MAX_TEMPLATE_BYTES: usize = 64 * 1024;
const DEFAULT_PROMPT_MAX_TOTAL_TEMPLATE_BYTES: usize = 1024 * 1024;

struct ConsoleTurnEventSink;

impl TurnEventSink for ConsoleTurnEventSink {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError> {
        println!("{event:?}");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let (observability_config, observability_runtime) = initialize_observability();
    let service_result = run_default_turn(&observability_config).await;
    let shutdown_report = tokio::task::spawn_blocking(move || observability_runtime.shutdown_with_timeout()).await?;
    TelemetryDiagnostics.shutdown(&shutdown_report);
    service_result
}

async fn run_default_turn(observability_config: &ObservabilityConfig) -> anyhow::Result<()> {
    let request = TurnRequest {
        story_id: StoryId::try_new(DEFAULT_STORY_ID).context("invalid default story id")?,
        idempotency_key: IdempotencyKey::try_new(DEFAULT_IDEMPOTENCY_KEY).context("invalid default idempotency key")?,
        player_input: DEFAULT_PLAYER_INPUT.to_owned(),
    };
    let control = TurnControl::new(Instant::now() + DEFAULT_TURN_TIMEOUT, TurnCancellation::new());
    let (session, mut trace) = begin_default_trace(observability_config);
    let gateway = build_llm_gateway()?;
    let prompt = build_prompt()?;
    let result = AiseEngine::new(gateway, prompt)
        .run_turn(request, control, &ConsoleTurnEventSink, &trace)
        .await;
    let trace_outcome = trace_outcome(&trace, &result);
    let status = trace_outcome.status;
    if let Ok(turn_result) = &result {
        trace.bind(vec![Attribute::u64(
            TRACE_METADATA_TURN_NUMBER,
            turn_result.result.turn_number,
        )]);
    }
    trace.finish(trace_outcome);
    session.finish(SessionOutcome {
        status,
        metadata: Vec::new(),
    });
    let result = result.context("default turn failed")?;

    println!("{}", result.result.story_text);
    Ok(())
}

fn build_llm_gateway() -> anyhow::Result<Arc<LlmGateway>> {
    let config = load_llm_config()?;
    let provider = Arc::new(OpenAiCompatProvider::new(&config));
    Ok(Arc::new(LlmGateway::new(provider, config)))
}

fn build_prompt() -> anyhow::Result<Arc<Prompt>> {
    let config = load_prompt_config()?;
    let prompt = Prompt::new(config)?;
    Ok(Arc::new(prompt))
}

fn load_llm_config() -> anyhow::Result<LlmConfig> {
    Ok(LlmConfig {
        base_url: required_env("AISE_LLM_BASE_URL")?,
        api_key: required_env("AISE_LLM_API_KEY")?,
        model: required_env("AISE_LLM_MODEL")?,
        temperature: parsed_env("AISE_LLM_TEMPERATURE", DEFAULT_LLM_TEMPERATURE)?,
        timeout_ms: parsed_env("AISE_LLM_TIMEOUT_MS", DEFAULT_LLM_TIMEOUT_MS)?,
    })
}

fn load_prompt_config() -> anyhow::Result<PromptConfig> {
    Ok(PromptConfig {
        directory: PathBuf::from(DEFAULT_PROMPT_DIRECTORY),
        max_prompts: DEFAULT_PROMPT_MAX_PROMPTS,
        max_template_bytes: DEFAULT_PROMPT_MAX_TEMPLATE_BYTES,
        max_total_template_bytes: DEFAULT_PROMPT_MAX_TOTAL_TEMPLATE_BYTES,
    })
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("missing required environment variable {name}"))
}

fn parsed_env<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .with_context(|| format!("invalid value for environment variable {name}")),
        Err(_) => Ok(default),
    }
}

fn initialize_observability() -> (ObservabilityConfig, ObservabilityRuntime) {
    let load = ObservabilityConfig::load_from_env();
    let config = load.config.clone();
    let issues = load.issues.clone();
    let components = ObservabilityRuntime::initialize(load, TelemetryDiagnostics);
    let enabled = components.runtime.is_enabled();
    let normal_logs = filter_fn(|metadata| metadata.target() != "aise::observation");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(components.layer)
        .with(tracing_subscriber::fmt::layer().with_filter(normal_logs))
        .with(filter)
        .init();

    for issue in issues {
        TelemetryDiagnostics.configuration(issue.field, issue.error_kind, config.endpoint_host().as_deref());
    }
    if config.enabled && !enabled {
        TelemetryDiagnostics.initialization("provider_unavailable", config.endpoint_host().as_deref());
    }
    tracing::info!(
        target: "aise::telemetry",
        enabled,
        environment = %config.environment,
        release = %config.release,
        sample_rate = config.sample_rate,
        content_policy = ?config.content_policy,
        queue_capacity = config.max_queue_size,
        endpoint_host = config.endpoint_host().as_deref().unwrap_or("unknown"),
        "OpenTelemetry configured"
    );

    (config, components.runtime)
}

fn begin_default_trace(config: &ObservabilityConfig) -> (ObservationSession, Trace) {
    let content = ContentCapture::new(ObservabilityContentConfig {
        policy: config.content_policy.clone(),
        max_field_bytes: config.max_field_bytes,
        max_observation_bytes: config.max_observation_bytes,
        detector_overlap_bytes: DETECTOR_OVERLAP_BYTES,
    });
    let session_id = Uuid::new_v4().to_string();
    let session = ObservationSession::begin(
        SessionSpec {
            id: Some(session_id.clone()),
            user_id: None,
            metadata: Vec::new(),
        },
        content.clone(),
    );
    let mut trace = session.begin_trace(TraceSpec {
        name: "default-turn",
        input: content.encode(&DEFAULT_PLAYER_INPUT, content.max_observation_bytes()).content,
        metadata: vec![
            Attribute::string(TRACE_ENVIRONMENT, config.environment.clone()),
            Attribute::string(TRACE_RELEASE, config.release.clone()),
        ],
        tags: vec!["story-turn".to_owned()],
    });
    trace.bind(vec![Attribute::string(TRACE_METADATA_STORY_ID, DEFAULT_STORY_ID)]);
    tracing::info!(session_id, story_id = DEFAULT_STORY_ID, "observation session started");
    (session, trace)
}

fn trace_outcome(trace: &Trace, result: &Result<TurnResult, EngineError>) -> TraceOutcome {
    match result {
        Ok(turn_result) => TraceOutcome {
            status: ObservationStatus::Ok,
            output: trace
                .content_capture()
                .encode(&turn_result.result.story_text, trace.content_capture().max_observation_bytes())
                .content,
            ..TraceOutcome::default()
        },
        Err(error) => TraceOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "engine_error".into(),
                failure_kind: "engine".into(),
                stage: None,
                message: error.to_string(),
            }),
            ..TraceOutcome::default()
        },
    }
}
