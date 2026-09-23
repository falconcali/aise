use crate::config::LangfuseConfig;
use aise::config::TraceContentPolicy;
use aise::turn::turn_trace::{LlmCallData, SpanPayload, TraceId, TraceSpan, TraceSpanSink, TurnData, TurnTrace};
use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum LangfuseTraceError {
    #[error("Langfuse exporter configuration is incomplete")]
    MissingCredentials,
    #[error("invalid Langfuse OTLP endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("failed to build Langfuse HTTP client: {0}")]
    Client(String),
    #[error("Langfuse exporter shutdown timed out")]
    ShutdownTimeout,
}

pub struct LangfuseTraceSink {
    tx: mpsc::Sender<TurnTrace>,
    shutdown_token: CancellationToken,
    done: Arc<Notify>,
    shutdown_timeout: Duration,
}

struct LangfuseExporter {
    client: reqwest::Client,
    endpoint: String,
    authorization: String,
    environment: String,
    capture_content: bool,
    max_export_batch_size: usize,
    scheduled_delay: Duration,
    max_request_bytes: usize,
}

impl LangfuseTraceSink {
    pub fn from_config(
        config: &LangfuseConfig,
        content_policy: TraceContentPolicy,
    ) -> Result<Option<Arc<Self>>, LangfuseTraceError> {
        if !config.enabled {
            return Ok(None);
        }
        let public_key = config
            .public_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(LangfuseTraceError::MissingCredentials)?;
        let secret_key = config
            .secret_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(LangfuseTraceError::MissingCredentials)?;
        let authorization = base64::engine::general_purpose::STANDARD.encode(format!("{public_key}:{secret_key}"));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.export_timeout_ms))
            .build()
            .map_err(|error| LangfuseTraceError::Client(error.to_string()))?;
        let endpoint = format!("{}/api/public/otel/v1/traces", config.base_url.trim_end_matches('/'));
        reqwest::Url::parse(&endpoint).map_err(|error| LangfuseTraceError::InvalidEndpoint(error.to_string()))?;
        let exporter = LangfuseExporter {
            client,
            endpoint,
            authorization: format!("Basic {authorization}"),
            environment: config.environment.clone(),
            capture_content: content_policy == TraceContentPolicy::FullContent,
            max_export_batch_size: config.max_export_batch_size,
            scheduled_delay: Duration::from_millis(config.scheduled_delay_ms),
            max_request_bytes: config.max_request_bytes,
        };
        let (tx, rx) = mpsc::channel(config.max_queue_size);
        let shutdown_token = CancellationToken::new();
        let done = Arc::new(Notify::new());
        tokio::spawn(run_exporter(exporter, rx, shutdown_token.clone(), done.clone()));
        Ok(Some(Arc::new(Self {
            tx,
            shutdown_token,
            done,
            shutdown_timeout: Duration::from_millis(config.shutdown_timeout_ms),
        })))
    }

    pub async fn shutdown_with_grace(&self) -> Result<(), LangfuseTraceError> {
        self.shutdown_token.cancel();
        tokio::time::timeout(self.shutdown_timeout, self.done.notified())
            .await
            .map_err(|_| LangfuseTraceError::ShutdownTimeout)
    }
}

impl TraceSpanSink for LangfuseTraceSink {
    fn write_span(&self, _trace_id: &TraceId, _span: &TraceSpan) {}

    fn write_trace(&self, trace: &TurnTrace) {
        match self.tx.try_send(trace.clone()) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(trace)) => {
                tracing::warn!(
                    trace_id = %trace.trace_id,
                    error_kind = "langfuse_queue_full",
                    "Langfuse trace export queue is full"
                );
            }
            Err(mpsc::error::TrySendError::Closed(trace)) => {
                tracing::warn!(
                    trace_id = %trace.trace_id,
                    error_kind = "langfuse_exporter_closed",
                    "Langfuse trace exporter is closed"
                );
            }
        }
    }
}

async fn run_exporter(
    exporter: LangfuseExporter,
    mut rx: mpsc::Receiver<TurnTrace>,
    shutdown_token: CancellationToken,
    done: Arc<Notify>,
) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => {
                rx.close();
                drain_traces(&exporter, &mut rx).await;
                break;
            }
            trace = rx.recv() => {
                match trace {
                    Some(trace) => export_batch(&exporter, trace, &mut rx).await,
                    None => break,
                }
            }
        }
    }
    done.notify_one();
}

async fn drain_traces(exporter: &LangfuseExporter, rx: &mut mpsc::Receiver<TurnTrace>) {
    while let Some(first) = rx.recv().await {
        let traces = collect_batch(exporter.max_export_batch_size, first, rx);
        send_traces(exporter, &traces).await;
    }
}

async fn export_batch(exporter: &LangfuseExporter, first: TurnTrace, rx: &mut mpsc::Receiver<TurnTrace>) {
    tokio::time::sleep(exporter.scheduled_delay).await;
    let traces = collect_batch(exporter.max_export_batch_size, first, rx);
    send_traces(exporter, &traces).await;
}

fn collect_batch(max_export_batch_size: usize, first: TurnTrace, rx: &mut mpsc::Receiver<TurnTrace>) -> Vec<TurnTrace> {
    let mut traces = Vec::with_capacity(max_export_batch_size);
    traces.push(first);
    while traces.len() < max_export_batch_size {
        match rx.try_recv() {
            Ok(trace) => traces.push(trace),
            Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
    traces
}

async fn send_traces(exporter: &LangfuseExporter, traces: &[TurnTrace]) {
    let payload = otlp_payload(traces, exporter.capture_content, &exporter.environment);
    let body = match serde_json::to_vec(&payload) {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(
                trace_count = traces.len(),
                error = %error,
                "failed to serialize Langfuse OTLP payload"
            );
            return;
        }
    };
    if body.len() > exporter.max_request_bytes {
        if traces.len() > 1 {
            for trace in traces {
                send_trace(exporter, trace).await;
            }
        } else {
            tracing::warn!(
                trace_id = %traces[0].trace_id,
                payload_bytes = body.len(),
                limit_bytes = exporter.max_request_bytes,
                "Langfuse OTLP payload exceeds configured request limit"
            );
        }
        return;
    }
    send_body(exporter, body, traces.len()).await;
}

async fn send_trace(exporter: &LangfuseExporter, trace: &TurnTrace) {
    let payload = otlp_payload(std::slice::from_ref(trace), exporter.capture_content, &exporter.environment);
    let body = match serde_json::to_vec(&payload) {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(
                trace_id = %trace.trace_id,
                error = %error,
                "failed to serialize Langfuse OTLP payload"
            );
            return;
        }
    };
    if body.len() > exporter.max_request_bytes {
        tracing::warn!(
            trace_id = %trace.trace_id,
            payload_bytes = body.len(),
            limit_bytes = exporter.max_request_bytes,
            "Langfuse OTLP payload exceeds configured request limit"
        );
        return;
    }
    send_body(exporter, body, 1).await;
}

async fn send_body(exporter: &LangfuseExporter, body: Vec<u8>, trace_count: usize) {
    let response = exporter
        .client
        .post(&exporter.endpoint)
        .header(AUTHORIZATION, exporter.authorization.as_str())
        .header("x-langfuse-ingestion-version", "4")
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            tracing::warn!(
                trace_count,
                status = %response.status(),
                "Langfuse OTLP export was rejected"
            );
        }
        Err(error) => {
            tracing::warn!(
                trace_count,
                error = %error,
                "Langfuse OTLP export failed"
            );
        }
    }
}

fn otlp_payload(traces: &[TurnTrace], capture_content: bool, environment: &str) -> Value {
    let spans = traces
        .iter()
        .flat_map(|trace| otlp_spans(trace, capture_content, environment))
        .collect::<Vec<_>>();
    json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    string_attribute("service.name", "aise-server"),
                    string_attribute("deployment.environment.name", environment),
                ]
            },
            "scopeSpans": [{
                "scope": { "name": "aise-langfuse" },
                "spans": spans,
            }]
        }]
    })
}

fn otlp_spans(trace: &TurnTrace, capture_content: bool, environment: &str) -> Vec<Value> {
    let trace_id = otlp_trace_id(&trace.trace_id);
    let root_source = trace.spans.iter().find(|span| span.kind == "aise.turn");
    let root_payload = root_source.and_then(parse_payload);
    let root_span_id = root_source
        .map(|span| otlp_span_id(&span.span_id))
        .unwrap_or_else(|| otlp_span_id(trace.trace_id.as_str()));
    let known_ids = trace.spans.iter().map(|span| span.span_id.as_str()).collect::<HashSet<_>>();
    let custom_root_id = root_source.map(|span| span.span_id.as_str());
    let mut spans = Vec::with_capacity(trace.spans.len().saturating_add(1));
    let mut root_attributes = root_attributes(trace, root_payload.as_ref(), capture_content, environment);
    if let Some(output) = capture_content.then(|| trace_output(trace)).flatten() {
        root_attributes.push(string_attribute("langfuse.observation.output", &json_string(&output)));
    }
    spans.push(otlp_span(
        &trace_id,
        &root_span_id,
        None,
        "execute-story-turn",
        (trace.started_at_ms, trace.ended_at_ms),
        root_attributes,
        payload_error(root_payload.as_ref()),
    ));
    for source in &trace.spans {
        if custom_root_id == Some(source.span_id.as_str()) {
            continue;
        }
        let parent_span_id = match source.parent_span_id.as_deref() {
            Some(parent) if custom_root_id != Some(parent) && known_ids.contains(parent) => otlp_span_id(parent),
            _ => root_span_id.clone(),
        };
        let payload = parse_payload(source);
        spans.push(otlp_span(
            &trace_id,
            &otlp_span_id(&source.span_id),
            Some(&parent_span_id),
            &observation_name(source, payload.as_ref()),
            (source.started_at_ms, source.ended_at_ms),
            span_attributes(source, payload.as_ref(), capture_content),
            payload_error(payload.as_ref()),
        ));
    }
    spans
}

fn otlp_span(
    trace_id: &str,
    span_id: &str,
    parent_span_id: Option<&str>,
    name: &str,
    timestamps: (u64, u64),
    attributes: Vec<Value>,
    error: Option<String>,
) -> Value {
    let mut span = json!({
        "traceId": trace_id,
        "spanId": span_id,
        "name": name,
        "kind": 1,
        "startTimeUnixNano": nanos(timestamps.0),
        "endTimeUnixNano": nanos(timestamps.1),
        "attributes": attributes,
    });
    if let Some(parent_span_id) = parent_span_id {
        span["parentSpanId"] = Value::String(parent_span_id.to_owned());
    }
    if let Some(error) = error {
        span["status"] = json!({ "code": 2, "message": error });
    }
    span
}

fn parse_payload(span: &TraceSpan) -> Option<SpanPayload> {
    serde_json::from_value(span.payload.clone()).ok()
}

fn root_attributes(
    trace: &TurnTrace,
    payload: Option<&SpanPayload>,
    capture_content: bool,
    environment: &str,
) -> Vec<Value> {
    let mut attributes = vec![
        string_attribute("langfuse.observation.type", "chain"),
        string_attribute("langfuse.trace.name", "execute-story-turn"),
        string_attribute("langfuse.session.id", &trace.story_id),
        string_attribute("langfuse.environment", environment),
        string_attribute("langfuse.release", env!("CARGO_PKG_VERSION")),
        string_array_attribute("langfuse.trace.tags", &["story-turn"]),
        string_attribute("langfuse.trace.metadata.story_id", &trace.story_id),
        string_attribute("langfuse.trace.metadata.local_trace_id", trace.trace_id.as_str()),
        int_attribute(
            "langfuse.trace.metadata.dropped_span_count",
            i64::from(trace.dropped_span_count),
        ),
    ];
    if let Some(turn_number) = trace.turn_number {
        attributes.push(int_attribute("langfuse.trace.metadata.turn_number", millis(turn_number.get())));
    }
    if let Some(SpanPayload::Turn(data)) = payload {
        if capture_content {
            attributes.push(string_attribute(
                "langfuse.observation.input",
                &json_string(&data.player_contribution),
            ));
        }
        attributes.push(string_attribute("langfuse.observation.metadata.status", &data.status));
        append_error(&mut attributes, data.error.as_deref());
    }
    attributes
}

fn span_attributes(source: &TraceSpan, payload: Option<&SpanPayload>, capture_content: bool) -> Vec<Value> {
    let mut attributes = vec![
        string_attribute("langfuse.observation.type", observation_type(source, payload)),
        string_attribute("langfuse.observation.metadata.aise_kind", &source.kind),
        string_attribute("langfuse.observation.metadata.aise_name", &source.name),
    ];
    match payload {
        Some(SpanPayload::Pipeline(data)) => {
            attributes.push(string_attribute("langfuse.observation.metadata.stage", &data.stage));
            attributes.push(string_attribute("langfuse.observation.metadata.status", &data.status));
            append_error(&mut attributes, data.error.as_deref());
        }
        Some(SpanPayload::LlmCall(data)) => append_llm_attributes(&mut attributes, data, capture_content),
        Some(SpanPayload::ToolCall(data)) => {
            if capture_content {
                attributes.push(string_attribute("langfuse.observation.input", &json_string(&data.args)));
                attributes.push(string_attribute("langfuse.observation.output", &json_string(&data.result)));
            }
            attributes.push(int_attribute(
                "langfuse.observation.metadata.latency_ms",
                millis(data.latency_ms),
            ));
            if !data.ok {
                append_error(&mut attributes, Some("tool call failed"));
            }
        }
        Some(SpanPayload::Validation(data)) => {
            if capture_content {
                attributes.push(string_attribute("langfuse.observation.output", &json_string(data)));
            }
            if !data.pass {
                append_error(&mut attributes, Some("validation failed"));
            }
        }
        Some(SpanPayload::Persist(data)) => {
            attributes.push(string_attribute("langfuse.observation.metadata.status", &data.status));
            attributes.push(int_attribute(
                "langfuse.observation.metadata.latency_ms",
                millis(data.latency_ms),
            ));
            append_error(&mut attributes, data.error.as_deref());
        }
        Some(SpanPayload::Turn(_)) | None => {
            if let Some(status) = source.payload.get("status").and_then(Value::as_str) {
                attributes.push(string_attribute("langfuse.observation.metadata.status", status));
                if status == "error" {
                    let message = source
                        .payload
                        .get("error_code")
                        .and_then(Value::as_str)
                        .unwrap_or("operation failed");
                    append_error(&mut attributes, Some(message));
                }
            }
        }
    }
    attributes
}

fn append_llm_attributes(attributes: &mut Vec<Value>, data: &LlmCallData, capture_content: bool) {
    attributes.push(string_attribute("langfuse.observation.model.name", &data.model));
    attributes.push(string_attribute("langfuse.observation.usage_details", &usage_json(data)));
    attributes.push(string_attribute("langfuse.observation.metadata.provider", &data.provider));
    attributes.push(string_attribute("langfuse.observation.metadata.purpose", &data.purpose));
    attributes.push(bool_attribute("langfuse.observation.metadata.stream", data.stream));
    attributes.push(int_attribute(
        "langfuse.observation.metadata.queue_wait_ms",
        millis(data.queue_wait_ms),
    ));
    attributes.push(int_attribute(
        "langfuse.observation.metadata.provider_latency_ms",
        millis(data.provider_latency_ms),
    ));
    attributes.push(string_attribute(
        "langfuse.observation.metadata.usage_accuracy",
        &data.usage_accuracy,
    ));
    if let Some(finish_reason) = &data.finish_reason {
        attributes.push(string_attribute("langfuse.observation.metadata.finish_reason", finish_reason));
    }
    if let Some(charge) = &data.charge {
        attributes.push(string_attribute("langfuse.observation.metadata.charge", &json_string(charge)));
    }
    if let Some(structured) = &data.structured_output {
        attributes.push(string_attribute(
            "langfuse.observation.metadata.output_contract",
            &structured.output_contract,
        ));
        attributes.push(string_attribute(
            "langfuse.observation.metadata.schema_hash",
            &structured.schema_hash,
        ));
        attributes.push(string_attribute(
            "langfuse.observation.metadata.structured_output_mode",
            &structured.structured_output_mode,
        ));
    }
    if let Some(content) = data.content.as_ref().filter(|_| capture_content) {
        attributes.push(string_attribute("langfuse.observation.input", &json_string(&content.messages)));
        attributes.push(string_attribute("langfuse.observation.output", &json_string(&content.response)));
    }
    if data.status != "succeeded" {
        append_error(attributes, data.error_kind.as_deref().or(Some(data.status.as_str())));
    }
}

fn append_error(attributes: &mut Vec<Value>, error: Option<&str>) {
    if let Some(error) = error {
        attributes.push(string_attribute("langfuse.observation.level", "ERROR"));
        attributes.push(string_attribute("langfuse.observation.status_message", error));
    }
}

fn observation_name(source: &TraceSpan, payload: Option<&SpanPayload>) -> String {
    match payload {
        Some(SpanPayload::Turn(_)) => "execute-story-turn",
        Some(SpanPayload::Pipeline(data)) => stage_name(&data.stage),
        Some(SpanPayload::LlmCall(data)) => llm_name(&data.purpose),
        Some(SpanPayload::ToolCall(data)) => tool_name(&data.tool),
        Some(SpanPayload::Validation(_)) => "validate-story",
        Some(SpanPayload::Persist(_)) => "commit-turn",
        None => raw_name(source),
    }
    .to_owned()
}

fn observation_type(source: &TraceSpan, payload: Option<&SpanPayload>) -> &'static str {
    match payload {
        Some(SpanPayload::Turn(_)) => "chain",
        Some(SpanPayload::Pipeline(data)) if data.stage == "context_retrieval" => "retriever",
        Some(SpanPayload::Pipeline(data)) if data.stage == "validation" => "evaluator",
        Some(SpanPayload::Pipeline(_)) => "chain",
        Some(SpanPayload::LlmCall(data)) if data.purpose == "embedding" => "embedding",
        Some(SpanPayload::LlmCall(_)) => "generation",
        Some(SpanPayload::ToolCall(_)) => "tool",
        Some(SpanPayload::Validation(_)) => "evaluator",
        Some(SpanPayload::Persist(_)) => "span",
        None if source.kind == "context.retrieve" => "retriever",
        None if source.kind == "aise.validation" => "evaluator",
        None if source.kind == "aise.tool_call" => "tool",
        None => "span",
    }
}

fn payload_error(payload: Option<&SpanPayload>) -> Option<String> {
    match payload {
        Some(SpanPayload::Turn(TurnData { error, .. })) => error.clone(),
        Some(SpanPayload::Pipeline(data)) => data.error.clone(),
        Some(SpanPayload::LlmCall(data)) if data.status != "succeeded" => {
            Some(data.error_kind.clone().unwrap_or_else(|| data.status.clone()))
        }
        Some(SpanPayload::ToolCall(data)) if !data.ok => Some("tool call failed".into()),
        Some(SpanPayload::Validation(data)) if !data.pass => Some("validation failed".into()),
        Some(SpanPayload::Persist(data)) => data.error.clone(),
        _ => None,
    }
}

fn trace_output(trace: &TurnTrace) -> Option<String> {
    trace
        .spans
        .iter()
        .filter_map(parse_payload)
        .filter_map(|payload| match payload {
            SpanPayload::LlmCall(data)
                if matches!(data.purpose.as_str(), "story_generation" | "story_repair")
                    && data.status == "succeeded" =>
            {
                data.content.map(|content| content.response)
            }
            _ => None,
        })
        .next_back()
}

fn usage_json(data: &LlmCallData) -> String {
    let cached = data.cached_input_tokens.unwrap_or(0);
    let reasoning = data.reasoning_tokens.unwrap_or(0);
    let input = data.input_tokens.saturating_sub(cached);
    let output = data.output_tokens.saturating_sub(reasoning);
    json_string(&json!({
        "input": input,
        "input_cached_tokens": cached,
        "output": output,
        "output_reasoning_tokens": reasoning,
        "total": data.input_tokens.saturating_add(data.output_tokens),
    }))
}

fn stage_name(stage: &str) -> &'static str {
    match stage {
        "turn_initializer" => "initialize-turn",
        "baseline_ctx_builder" | "context" => "prepare-context",
        "writer_planner" => "plan-turn",
        "context_retrieval" => "retrieve-context",
        "character_think" => "think-characters",
        "story_generator" => "generate-story",
        "story_state_extractor" => "extract-story-state",
        "validation" => "validate-story",
        "story_repairer" => "repair-story",
        "turn_committer" => "commit-turn",
        _ => "execute-pipeline-stage",
    }
}

fn llm_name(purpose: &str) -> &'static str {
    match purpose {
        "writer_plan" => "plan-turn",
        "context_retrieval" => "retrieve-context",
        "character_think" => "think-character",
        "story_generation" => "generate-story",
        "story_state_extraction" => "extract-story-state",
        "story_repair" => "repair-story",
        "embedding" => "embed-context",
        _ => "generate-completion",
    }
}

fn tool_name(tool: &str) -> &'static str {
    match tool {
        "store.load_story_snapshot" => "load-story-snapshot",
        _ => "execute-tool",
    }
}

fn raw_name(source: &TraceSpan) -> &'static str {
    match source.name.as_str() {
        "context.retrieve" => "retrieve-context",
        "context.prepare" => "prepare-context",
        "narrative.reuse" => "reuse-narrative-projection",
        "story.commit" => "commit-turn",
        "validation.execute" => "validate-story",
        _ => "execute-step",
    }
}

fn string_attribute(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
}

fn int_attribute(key: &str, value: i64) -> Value {
    json!({ "key": key, "value": { "intValue": value.to_string() } })
}

fn bool_attribute(key: &str, value: bool) -> Value {
    json!({ "key": key, "value": { "boolValue": value } })
}

fn string_array_attribute(key: &str, values: &[&str]) -> Value {
    let values = values.iter().map(|value| json!({ "stringValue": value })).collect::<Vec<_>>();
    json!({ "key": key, "value": { "arrayValue": { "values": values } } })
}

fn otlp_trace_id(trace_id: &TraceId) -> String {
    fixed_hex_id(trace_id.as_str(), 32)
}

fn otlp_span_id(value: &str) -> String {
    fixed_hex_id(value, 16)
}

fn fixed_hex_id(value: &str, width: usize) -> String {
    let hex = value
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect::<String>();
    if hex.len() >= width {
        return hex[hex.len() - width..].to_ascii_lowercase();
    }
    let first = fnv1a(value.as_bytes(), 0xcbf29ce484222325);
    if width == 16 {
        return format!("{first:016x}");
    }
    let second = fnv1a(value.as_bytes(), 0x84222325cbf29ce4);
    format!("{first:016x}{second:016x}")
}

fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    bytes
        .iter()
        .fold(seed, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

fn json_string(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn millis(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn nanos(epoch_millis: u64) -> String {
    epoch_millis.saturating_mul(1_000_000).to_string()
}

#[cfg(test)]
#[path = "tests/langfuse_tests.rs"]
mod tests;
