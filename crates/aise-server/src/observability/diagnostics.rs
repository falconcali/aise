use crate::observability::runtime::ShutdownReport;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const RATE_LIMIT: Duration = Duration::from_secs(60);
const MAX_RATE_LIMIT_KEYS: usize = 128;
static LAST_EMITTED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct TelemetryDiagnostics;

impl TelemetryDiagnostics {
    pub fn initialization(&self, error_kind: &'static str, endpoint_host: Option<&str>) {
        if should_emit(error_kind) {
            tracing::warn!(
                target: "aise::telemetry",
                error_kind,
                endpoint_host = endpoint_host.unwrap_or("unknown"),
                "OpenTelemetry initialization failed"
            );
        }
    }

    pub fn configuration(&self, field: &'static str, error_kind: &'static str, endpoint_host: Option<&str>) {
        if should_emit(&format!("{field}:{error_kind}")) {
            tracing::warn!(
                target: "aise::telemetry",
                field,
                error_kind,
                endpoint_host = endpoint_host.unwrap_or("unknown"),
                "OpenTelemetry configuration was disabled"
            );
        }
    }

    pub fn export(&self, error_kind: &'static str, status: Option<u16>, endpoint_host: &str) {
        if should_emit(error_kind) {
            tracing::warn!(
                target: "aise::telemetry",
                error_kind,
                status = status
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                endpoint_host,
                "OpenTelemetry export failed"
            );
        }
    }

    pub fn dropped(&self, dropped_span_count: u64, queue_capacity: usize) {
        if should_emit("queue_full") {
            tracing::warn!(
                target: "aise::telemetry",
                error_kind = "queue_full",
                dropped_span_count,
                queue_capacity,
                "OpenTelemetry spans were dropped"
            );
        }
    }

    pub fn shutdown(&self, report: &ShutdownReport) {
        if report.completed {
            tracing::info!(
                target: "aise::telemetry",
                dropped_span_count = report.dropped_span_count,
                "OpenTelemetry shutdown completed"
            );
        } else if should_emit(report.error_kind.unwrap_or("shutdown_failed")) {
            tracing::warn!(
                target: "aise::telemetry",
                error_kind = report.error_kind.unwrap_or("shutdown_failed"),
                dropped_span_count = report.dropped_span_count,
                "OpenTelemetry shutdown failed"
            );
        }
    }
}

fn should_emit(error_kind: &str) -> bool {
    let now = Instant::now();
    let entries = LAST_EMITTED.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut entries) = entries.try_lock() else {
        return false;
    };
    match entries.get(error_kind) {
        Some(last) if now.duration_since(*last) < RATE_LIMIT => false,
        _ => {
            if entries.len() >= MAX_RATE_LIMIT_KEYS {
                if let Some(oldest) = entries
                    .iter()
                    .min_by_key(|(_, emitted_at)| **emitted_at)
                    .map(|(key, _)| key.clone())
                {
                    entries.remove(&oldest);
                }
            }
            entries.insert(error_kind.to_owned(), now);
            true
        }
    }
}

#[cfg(test)]
#[path = "tests/diagnostics_tests.rs"]
mod tests;
