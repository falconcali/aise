use super::*;

#[test]
fn diagnostics_is_cloneable_without_per_runtime_queue() {
    let diagnostics = TelemetryDiagnostics;
    let cloned = diagnostics.clone();
    let report = ShutdownReport {
        completed: true,
        dropped_span_count: 0,
        error_kind: None,
    };

    cloned.shutdown(&report);
}
