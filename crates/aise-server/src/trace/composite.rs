use aise::turn::turn_trace::{TraceId, TraceSpan, TraceSpanSink, TurnTrace};
use std::sync::Arc;

pub struct CompositeTraceSink {
    sinks: Vec<Arc<dyn TraceSpanSink>>,
}

impl CompositeTraceSink {
    pub fn new(sinks: Vec<Arc<dyn TraceSpanSink>>) -> Arc<Self> {
        Arc::new(Self { sinks })
    }
}

impl TraceSpanSink for CompositeTraceSink {
    fn write_span(&self, trace_id: &TraceId, span: &TraceSpan) {
        for sink in &self.sinks {
            sink.write_span(trace_id, span);
        }
    }

    fn write_trace(&self, trace: &TurnTrace) {
        for sink in &self.sinks {
            sink.write_trace(trace);
        }
    }
}
