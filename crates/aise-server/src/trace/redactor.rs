pub trait TraceRedactor: Send + Sync {
    fn redact_value(&self, value: &mut serde_json::Value);
}

#[derive(Default)]
pub struct NoopRedactor;

impl TraceRedactor for NoopRedactor {
    fn redact_value(&self, _value: &mut serde_json::Value) {}
}
