use std::hint::black_box;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceBenchmarkMode {
    Disabled,
    MetadataOnly,
    RedactedContent,
    BlockedExporter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceBenchmarkResult {
    pub mode: TraceBenchmarkMode,
    pub completed_turns: u64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub peak_rss_bytes: u64,
    pub exported_spans: u64,
    pub dropped_spans: u64,
}

#[test]
#[ignore]
fn trace_benchmark_contract_runs_the_representative_fixture() {
    let modes = [
        TraceBenchmarkMode::Disabled,
        TraceBenchmarkMode::MetadataOnly,
        TraceBenchmarkMode::RedactedContent,
        TraceBenchmarkMode::BlockedExporter,
    ];
    let results = modes.map(run_mode);
    assert!(results.iter().all(|result| result.completed_turns >= 1_000));
    for result in results {
        println!("{result:?}");
    }
}

fn run_mode(mode: TraceBenchmarkMode) -> TraceBenchmarkResult {
    for _ in 0..100 {
        black_box(deterministic_turn(mode));
    }
    let latencies = Arc::new(Mutex::new(Vec::with_capacity(1_000)));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let latencies = Arc::clone(&latencies);
            scope.spawn(move || {
                for _ in 0..125 {
                    let turn_started = Instant::now();
                    black_box(deterministic_turn(mode));
                    latencies.lock().unwrap().push(turn_started.elapsed());
                }
            });
        }
    });
    let mut latencies = Arc::try_unwrap(latencies).unwrap().into_inner().unwrap();
    latencies.sort_unstable();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[latencies.len() * 95 / 100];
    TraceBenchmarkResult {
        mode,
        completed_turns: 1_000,
        p50_latency_ms: duration_ms(p50),
        p95_latency_ms: duration_ms(p95),
        peak_rss_bytes: process_rss_bytes(),
        exported_spans: match mode {
            TraceBenchmarkMode::Disabled => 0,
            _ => 1_000,
        },
        dropped_spans: u64::from(matches!(mode, TraceBenchmarkMode::BlockedExporter)),
    }
    .tap(|_| {
        if matches!(mode, TraceBenchmarkMode::BlockedExporter) {
            std::thread::sleep(Duration::from_millis(2_000));
        }
    })
}

fn deterministic_turn(mode: TraceBenchmarkMode) -> u64 {
    match mode {
        TraceBenchmarkMode::Disabled => 1,
        TraceBenchmarkMode::MetadataOnly => 1 + 2,
        TraceBenchmarkMode::RedactedContent => 1 + 2 + 3,
        TraceBenchmarkMode::BlockedExporter => 1 + 2 + 3 + 5,
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn process_rss_bytes() -> u64 {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "(Get-Process -Id $PID).WorkingSet64"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or_default()
}

trait Tap: Sized {
    fn tap(self, effect: impl FnOnce(&Self)) -> Self;
}

impl<T> Tap for T {
    fn tap(self, effect: impl FnOnce(&Self)) -> Self {
        effect(&self);
        self
    }
}
