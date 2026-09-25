use super::fields::{
    BoundedContent, GenerationCost, GenerationUsage, METADATA_CONTENT_ENCODE_FAILED, METADATA_ERROR_CODE,
    METADATA_FAILURE_KIND, METADATA_INPUT_CAPTURED_BYTES, METADATA_INPUT_ORIGINAL_BYTES, METADATA_INPUT_SHA256,
    METADATA_INPUT_TRUNCATED, METADATA_OUTPUT_CAPTURED_BYTES, METADATA_OUTPUT_ORIGINAL_BYTES, METADATA_OUTPUT_SHA256,
    METADATA_OUTPUT_TRUNCATED, METADATA_STAGE, OBSERVATION_COST_DETAILS, OBSERVATION_INPUT, OBSERVATION_LEVEL,
    OBSERVATION_OUTPUT, OBSERVATION_STATUS_MESSAGE, OBSERVATION_TYPE, OBSERVATION_USAGE_DETAILS, ObservationAttribute,
    ObservationFields, ObservationFinish, ObservationStatus, ObservationValue, SCHEMA_VERSION,
};
use super::step::ObservationStep;
use opentelemetry::{Array, Context, Value};
use std::future::Future;
use tracing::{Instrument, Span, field::Empty};
use tracing_opentelemetry::OpenTelemetrySpanExt;

macro_rules! observation_span {
    ($name:literal) => {
        tracing::info_span!(
            target: "aise::observation",
            $name,
            "otel.status_code" = Empty,
            "otel.status_message" = Empty
        )
    };
}

pub struct ObservationSpan {
    span: Span,
    finished: bool,
}

impl ObservationSpan {
    pub fn begin(step: ObservationStep, fields: ObservationFields) -> Self {
        Self::begin_inner(step, fields, None)
    }

    pub fn begin_with_parent(step: ObservationStep, fields: ObservationFields, parent: &Context) -> Self {
        Self::begin_inner(step, fields, Some(parent))
    }

    fn begin_inner(step: ObservationStep, fields: ObservationFields, parent: Option<&Context>) -> Self {
        let span = create_span(step);
        if let Some(parent) = parent {
            let _ = span.set_parent(parent.clone());
        }
        let mut observation = Self { span, finished: false };
        if observation.is_recording() {
            observation.record_static(OBSERVATION_TYPE, step.kind().as_str());
            observation.record_static(SCHEMA_VERSION, ObservationStep::SCHEMA_VERSION);
            observation.record_fields(fields);
        }
        observation
    }

    pub async fn in_scope<F: Future>(&self, future: F) -> F::Output {
        future.instrument(self.span.clone()).await
    }

    pub fn finish(mut self, finish: ObservationFinish) {
        self.finish_inner(finish);
    }

    pub fn is_recording(&self) -> bool {
        !self.span.is_disabled()
    }

    pub(crate) fn tracing_span(&self) -> Span {
        self.span.clone()
    }

    pub(crate) fn record_attribute(&mut self, attribute: ObservationAttribute) {
        if !self.is_recording() {
            return;
        }
        match attribute.value {
            ObservationValue::String(value) => self.span.set_attribute(attribute.key, value),
            ObservationValue::Bool(value) => self.span.set_attribute(attribute.key, value),
            ObservationValue::I64(value) => self.span.set_attribute(attribute.key, value),
            ObservationValue::U64(value) => {
                if let Ok(value) = i64::try_from(value) {
                    self.span.set_attribute(attribute.key, value);
                } else {
                    self.span.set_attribute(attribute.key, value.to_string());
                }
            }
            ObservationValue::F64(value) => self.span.set_attribute(attribute.key, value),
            ObservationValue::StringList(value) => self.span.set_attribute(
                attribute.key,
                Value::Array(Array::String(value.into_iter().map(Into::into).collect())),
            ),
        }
    }

    pub(crate) fn record_static(&mut self, key: &'static str, value: &'static str) {
        if self.is_recording() {
            self.span.set_attribute(key, value);
        }
    }

    pub(crate) fn record_fields(&mut self, fields: ObservationFields) {
        if !self.is_recording() {
            return;
        }
        for attribute in fields.metadata {
            self.record_attribute(attribute);
        }
        if let Some(input) = fields.input {
            self.record_content(input, ContentDirection::Input);
        }
    }

    pub(crate) fn finish_inner(&mut self, finish: ObservationFinish) {
        if self.finished {
            return;
        }
        if self.is_recording() {
            for attribute in finish.metadata {
                self.record_attribute(attribute);
            }
            if let Some(output) = finish.output {
                self.record_content(output, ContentDirection::Output);
            }
            if let Some(usage) = finish.usage {
                self.record_usage(usage);
            }
            if let Some(cost) = finish.cost {
                self.record_cost(cost);
            }
            self.record_finish_status(finish.status, finish.error);
        }
        self.finished = true;
    }

    fn record_content(&mut self, content: BoundedContent, direction: ContentDirection) {
        let (content_key, original_key, captured_key, truncated_key, hash_key) = match direction {
            ContentDirection::Input => (
                OBSERVATION_INPUT,
                METADATA_INPUT_ORIGINAL_BYTES,
                METADATA_INPUT_CAPTURED_BYTES,
                METADATA_INPUT_TRUNCATED,
                METADATA_INPUT_SHA256,
            ),
            ContentDirection::Output => (
                OBSERVATION_OUTPUT,
                METADATA_OUTPUT_ORIGINAL_BYTES,
                METADATA_OUTPUT_CAPTURED_BYTES,
                METADATA_OUTPUT_TRUNCATED,
                METADATA_OUTPUT_SHA256,
            ),
        };
        self.span.set_attribute(content_key, content.json);
        self.record_attribute(ObservationAttribute::u64(original_key, content.original_bytes as u64));
        self.record_attribute(ObservationAttribute::u64(captured_key, content.captured_bytes as u64));
        self.record_attribute(ObservationAttribute::bool(truncated_key, content.truncated));
        self.span.set_attribute(hash_key, content.sha256);
    }

    fn record_usage(&mut self, usage: GenerationUsage) {
        if !usage.is_valid() {
            return;
        }
        match serde_json::to_string(&usage) {
            Ok(value) => self.span.set_attribute(OBSERVATION_USAGE_DETAILS, value),
            Err(_) => self.span.set_attribute(METADATA_CONTENT_ENCODE_FAILED, true),
        }
    }

    fn record_cost(&mut self, cost: GenerationCost) {
        if !cost.is_exportable() {
            return;
        }
        match serde_json::to_string(&cost) {
            Ok(value) => self.span.set_attribute(OBSERVATION_COST_DETAILS, value),
            Err(_) => self.span.set_attribute(METADATA_CONTENT_ENCODE_FAILED, true),
        }
    }

    fn record_finish_status(&mut self, status: ObservationStatus, error: Option<super::fields::ObservationError>) {
        match status {
            ObservationStatus::Ok => {
                self.span.record("otel.status_code", "OK");
            }
            ObservationStatus::Cancelled | ObservationStatus::Conflict | ObservationStatus::Incomplete => {
                self.span.record("otel.status_message", status.as_str());
                self.span.set_attribute(OBSERVATION_LEVEL, "WARNING");
                self.span.set_attribute(OBSERVATION_STATUS_MESSAGE, status.as_str());
            }
            ObservationStatus::Error | ObservationStatus::DeadlineExceeded => {
                self.span.record("otel.status_code", "ERROR");
                self.span.record("otel.status_message", status.as_str());
                self.span.set_attribute(OBSERVATION_LEVEL, "ERROR");
                self.span.set_attribute(OBSERVATION_STATUS_MESSAGE, status.as_str());
            }
        }
        if let Some(error) = error {
            let message = bounded_message(error.message.as_str());
            self.span.record("otel.status_code", "ERROR");
            self.span.record("otel.status_message", message.as_str());
            self.span.set_attribute(
                OBSERVATION_LEVEL,
                if status == ObservationStatus::Conflict {
                    "WARNING"
                } else {
                    "ERROR"
                },
            );
            self.span.set_attribute(OBSERVATION_STATUS_MESSAGE, message);
            self.span.set_attribute(METADATA_ERROR_CODE, error.code);
            self.span.set_attribute(METADATA_FAILURE_KIND, error.failure_kind);
            if let Some(stage) = error.stage {
                self.span.set_attribute(METADATA_STAGE, stage);
            }
        }
    }
}

fn bounded_message(message: &str) -> String {
    message.chars().take(1024).collect()
}

impl Drop for ObservationSpan {
    fn drop(&mut self) {
        if !self.finished {
            self.finish_inner(ObservationFinish {
                status: ObservationStatus::Incomplete,
                ..ObservationFinish::default()
            });
        }
    }
}

pub async fn observe_result<T, E, F, M>(
    step: ObservationStep,
    fields: ObservationFields,
    future: F,
    map_error: M,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
    M: FnOnce(&E) -> super::fields::ObservationError,
{
    let span = ObservationSpan::begin(step, fields);
    let result = span.in_scope(future).await;
    let finish = match &result {
        Ok(_) => ObservationFinish {
            status: ObservationStatus::Ok,
            ..ObservationFinish::default()
        },
        Err(error) => ObservationFinish {
            status: ObservationStatus::Error,
            error: Some(map_error(error)),
            ..ObservationFinish::default()
        },
    };
    span.finish(finish);
    result
}

enum ContentDirection {
    Input,
    Output,
}

fn create_span(step: ObservationStep) -> Span {
    match step {
        ObservationStep::ExecuteStoryTurn => {
            observation_span!("execute-story-turn (执行故事回合)")
        }
        ObservationStep::ResolveInteractionSession => {
            observation_span!("resolve-interaction-session (解析交互会话)")
        }
        ObservationStep::ValidateRequest => observation_span!("validate-request (校验请求)"),
        ObservationStep::AdmitTurnTask => observation_span!("admit-turn-task (准入回合任务)"),
        ObservationStep::CoordinateStoryTurn => {
            observation_span!("coordinate-story-turn (协调故事回合)")
        }
        ObservationStep::LoadStory => observation_span!("load-story (加载故事)"),
        ObservationStep::CheckIdempotency => {
            observation_span!("check-idempotency (检查幂等性)")
        }
        ObservationStep::RunTurnPipelines => {
            observation_span!("run-turn-pipelines (执行回合流水线)")
        }
        ObservationStep::InitializeTurn => observation_span!("initialize-turn (初始化回合)"),
        ObservationStep::PrepareContext => observation_span!("prepare-context (准备上下文)"),
        ObservationStep::LoadStorySnapshot => {
            observation_span!("load-story-snapshot (加载故事快照)")
        }
        ObservationStep::ActivateWorldInfo => {
            observation_span!("activate-world-info (激活世界信息)")
        }
        ObservationStep::PlanTurn => observation_span!("plan-turn (规划回合)"),
        ObservationStep::ProjectNarrative => {
            observation_span!("project-narrative (投影叙事图)")
        }
        ObservationStep::GenerateWriterPlan => {
            observation_span!("generate-writer-plan (生成写作计划)")
        }
        ObservationStep::RetrieveContext => {
            observation_span!("retrieve-context (检索上下文)")
        }
        ObservationStep::ThinkCharacters => {
            observation_span!("think-characters (角色思考)")
        }
        ObservationStep::ThinkCharacter => observation_span!("think-character (角色思考)"),
        ObservationStep::GenerateStory => observation_span!("generate-story (生成故事)"),
        ObservationStep::DraftStoryText => {
            observation_span!("draft-story-text (起草故事正文)")
        }
        ObservationStep::ExtractStoryState => {
            observation_span!("extract-story-state (提取故事状态)")
        }
        ObservationStep::InferStoryState => {
            observation_span!("infer-story-state (推断故事状态)")
        }
        ObservationStep::ValidateStory => observation_span!("validate-story (校验故事)"),
        ObservationStep::RepairStory => observation_span!("repair-story (修复故事)"),
        ObservationStep::ReviseStoryText => {
            observation_span!("revise-story-text (修订故事正文)")
        }
        ObservationStep::CommitTurn => observation_span!("commit-turn (提交回合)"),
        ObservationStep::PersistTurn => observation_span!("persist-turn (持久化回合)"),
    }
}

#[cfg(test)]
#[path = "tests/span_tests.rs"]
mod tests;
