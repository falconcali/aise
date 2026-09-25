use super::content::ContentCapture;
use super::model::{
    Attribute, ObservationKind, ObservationSpec, SessionSpec, TRACE_ENVIRONMENT, TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
    TRACE_METADATA_STORY_ID, TRACE_METADATA_TURN_NUMBER, TRACE_NAME, TRACE_RELEASE, TRACE_TAGS, TraceOutcome,
    TraceSpec,
};
use super::observation::Observation;
use opentelemetry::Context;

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
        let propagated = metadata
            .iter()
            .filter(|attribute| is_propagated(attribute.key))
            .cloned()
            .collect();
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
            propagated,
            finished: false,
        };
        trace.bind(vec![Attribute::string(TRACE_TAGS, spec.tags.join(","))]);
        trace
    }

    pub fn root(&self) -> &Observation {
        self.root.as_ref().expect("trace root exists")
    }

    pub fn begin_observation(&self, mut spec: ObservationSpec) -> Observation {
        let mut metadata = self.propagated.clone();
        metadata.append(&mut spec.metadata);
        spec.metadata = metadata;
        Observation::new(spec, Some(&self.context), self.content.clone())
    }

    pub fn bind(&mut self, attributes: Vec<Attribute>) {
        for attribute in attributes {
            if !matches!(
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
                continue;
            }
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

    pub fn finish(mut self, outcome: TraceOutcome) {
        if let Some(root) = self.root.take() {
            root.finish(outcome);
        }
        self.finished = true;
    }
}

fn is_propagated(key: &str) -> bool {
    matches!(
        key,
        TRACE_NAME
            | TRACE_TAGS
            | TRACE_ENVIRONMENT
            | TRACE_RELEASE
            | super::model::SCHEMA_VERSION
            | super::model::SESSION_ID
            | TRACE_METADATA_STORY_ID
            | TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST
            | TRACE_METADATA_TURN_NUMBER
    )
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
