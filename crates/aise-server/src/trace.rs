use aise::core::turn_trace::{TraceSpan, TraceSpanSink, TurnTrace};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct FileTraceSpanSink {
    trace_dir: PathBuf,
}

impl FileTraceSpanSink {
    pub fn new(trace_dir: PathBuf) -> Self {
        if let Err(error) = std::fs::create_dir_all(&trace_dir) {
            tracing::warn!(path = %trace_dir.display(), error = %error, "aise.trace.create_dir_failed");
        }
        Self { trace_dir }
    }
}

impl TraceSpanSink for FileTraceSpanSink {
    fn write_span(&self, trace_id: &str, span: &TraceSpan) {
        let path = self.trace_dir.join(format!("{trace_id}.jsonl"));
        let line = match serde_json::to_string(span) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "aise.trace.span_serialize_failed");
                return;
            }
        };
        append_line(&path, &line);
    }

    fn write_trace(&self, trace: &TurnTrace) {
        let path = self.trace_dir.join(format!("{}.json", trace.trace_id));
        let body = match serde_json::to_string_pretty(trace) {
            Ok(body) => body,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "aise.trace.serialize_failed");
                return;
            }
        };
        let mut file = match OpenOptions::new().create(true).write(true).truncate(true).open(&path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "aise.trace.write_failed");
                return;
            }
        };
        if let Err(error) = writeln!(file, "{body}") {
            tracing::warn!(path = %path.display(), error = %error, "aise.trace.write_failed");
        }
        if let Err(error) = file.flush() {
            tracing::warn!(path = %path.display(), error = %error, "aise.trace.flush_failed");
        }
    }
}

fn append_line(path: &Path, line: &str) {
    let mut file = match OpenOptions::new().create(true).append(true).open(path) {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!(path = %path.display(), error = %error, "aise.trace.append_failed");
            return;
        }
    };
    if let Err(error) = writeln!(file, "{line}") {
        tracing::warn!(path = %path.display(), error = %error, "aise.trace.append_failed");
    }
    if let Err(error) = file.flush() {
        tracing::warn!(path = %path.display(), error = %error, "aise.trace.flush_failed");
    }
}
