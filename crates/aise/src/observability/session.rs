use super::content::ContentCapture;
use super::model::{SessionOutcome, SessionSpec, TraceSpec};
use super::trace::Trace;

pub struct ObservationSession {
    context: SessionSpec,
    content: ContentCapture,
    finished: bool,
}

impl ObservationSession {
    pub fn begin(spec: SessionSpec, content: ContentCapture) -> Self {
        Self {
            context: spec,
            content,
            finished: false,
        }
    }

    pub fn begin_trace(&self, spec: TraceSpec) -> Trace {
        Trace::begin(self.context.clone(), spec, self.content.clone())
    }

    pub fn finish(mut self, _outcome: SessionOutcome) {
        self.finished = true;
    }
}

impl Drop for ObservationSession {
    fn drop(&mut self) {
        if !self.finished {
            tracing::debug!(target: "aise::telemetry", "observation session dropped before finish");
        }
    }
}
