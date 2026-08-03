use std::time::Duration;

/// Per-Turn execution trace; diagnostics only, never persisted as state.
#[derive(Debug, Default, Clone)]
pub struct ExecutionTrace {
    pub events: Vec<TraceEvent>,
}

#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Stage name as reported by the pipeline.
    pub stage: &'static str,
    pub elapsed: Duration,
}
