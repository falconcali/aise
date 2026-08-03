use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct ExecutionTrace {
    pub events: Vec<TraceEvent>,
}

#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub stage: &'static str,
    pub elapsed: Duration,
}
