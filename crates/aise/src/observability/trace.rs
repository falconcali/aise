use super::content::ContentCapture;
use super::model::{
    Attribute, ObservationKind, ObservationSpec, SessionSpec, TRACE_ENVIRONMENT, TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
    TRACE_METADATA_STORY_ID, TRACE_METADATA_TURN_NUMBER, TRACE_NAME, TRACE_RELEASE, TRACE_TAGS, TraceOutcome,
    TraceSpec,
};
use super::observation::Observation;
use opentelemetry::Context;
use std::future::Future;
use tracing::Span;

pub struct Trace {
    pub(crate) root: Option<Observation>,
    pub(crate) context: Context,
    pub(crate) content: ContentCapture,
    pub(crate) propagated: Vec<Attribute>,
    finished: bool,
}

impl Trace {
    pub(crate) fn begin(session: SessionSpec, spec: TraceSpec, content: ContentCapture) -> Self {
        let mut metadata = session.metadata.clone();
        metadata.extend(spec.metadata.clone());
        metadata.push(Attribute::string(TRACE_NAME, spec.name));
        metadata.push(Attribute::string(super::model::SCHEMA_VERSION, "2"));
        if let Some(id) = &session.id {
            metadata.push(Attribute::string(super::model::SESSION_ID, id.clone()));
        }
        let root = Observation::new(
            ObservationSpec {
                name: spec.name,
                kind: ObservationKind::Chain,
                input: spec.input,
                metadata,
            },
            None,
            content.clone(),
        );
        let context = root.context.clone();
        let mut trace = Self {
            root: Some(root),
            context,
            content,
            propagated: Vec::new(),
            finished: false,
        };
        trace.bind(Attribute::string(TRACE_TAGS, spec.tags.join(",")));
        trace
    }

    pub fn root(&self) -> &Observation {
        self.root.as_ref().expect("trace root exists")
    }

    pub fn begin_observation(&self, mut spec: ObservationSpec) -> Observation {
        let mut metadata = self.propagated.clone();
        metadata.append(&mut spec.metadata);
        spec.metadata = metadata;
        self.root().begin(spec)
    }

    pub fn bind(&mut self, attribute: Attribute) {
        if matches!(
            attribute.key,
            TRACE_NAME
                | TRACE_TAGS
                | TRACE_ENVIRONMENT
                | TRACE_RELEASE
                | super::model::SCHEMA_VERSION
                | super::model::SESSION_ID
                | TRACE_METADATA_STORY_ID
                | TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST
                | TRACE_METADATA_TURN_NUMBER
        ) {
            self.root
                .as_mut()
                .expect("trace root exists")
                .record_attribute(attribute.clone());
            if let Some(existing) = self.propagated.iter_mut().find(|item| item.key == attribute.key) {
                *existing = attribute;
            } else {
                self.propagated.push(attribute);
            }
            self.context = self.root().context.clone();
        }
    }

    pub fn content_capture(&self) -> &ContentCapture {
        &self.content
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn span(&self) -> Span {
        self.root().span.clone()
    }

    pub fn bind_session(&mut self, session_id: &str, story_id: &str) {
        self.bind(Attribute::string(super::model::SESSION_ID, session_id));
        self.bind(Attribute::string(TRACE_METADATA_STORY_ID, story_id));
    }

    pub fn bind_request(&mut self, digest: &str) {
        self.bind(Attribute::string(TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST, digest));
    }

    pub fn bind_turn(&mut self, turn_number: u64) {
        self.bind(Attribute::u64(TRACE_METADATA_TURN_NUMBER, turn_number));
    }

    pub async fn trace<F: Future>(&self, future: F) -> F::Output {
        self.root().trace(future).await
    }

    pub fn finish(mut self, outcome: TraceOutcome) {
        if let Some(root) = self.root.take() {
            root.finish(outcome);
        }
        self.finished = true;
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(root) = self.root.as_mut() {
                root.finish_incomplete();
            }
        }
    }
}
