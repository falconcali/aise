use crate::trace::redactor::TraceRedactor;
use aise::core::turn_trace::{TraceId, TraceRecord, TraceSpan, TraceSpanSink, TurnTrace};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct TraceWriterConfig {
    pub channel_capacity: usize,
    pub max_record_bytes: usize,
    pub rotation_bytes: u64,
    pub retention_files: usize,
    pub shutdown_grace_ms: u64,
}

impl Default for TraceWriterConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 256,
            max_record_bytes: 128 * 1024,
            rotation_bytes: 64 * 1024 * 1024,
            retention_files: 16,
            shutdown_grace_ms: 5_000,
        }
    }
}

impl TraceWriterConfig {
    pub fn validate(&self) -> Result<(), TraceSinkError> {
        if self.channel_capacity == 0 {
            return Err(TraceSinkError::InvalidConfig("channel_capacity must be positive".into()));
        }
        if self.max_record_bytes == 0 {
            return Err(TraceSinkError::InvalidConfig("max_record_bytes must be positive".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TraceSinkError {
    #[error("trace queue is full")]
    ChannelFull,
    #[error("trace writer is shutting down")]
    ShuttingDown,
    #[error("invalid trace writer configuration: {0}")]
    InvalidConfig(String),
    #[error("trace write failed: {0}")]
    Io(String),
}

pub trait TraceSink: Send + Sync {
    fn try_write(&self, record: TraceRecord) -> Result<(), TraceSinkError>;
}

pub struct TraceWriter {
    tx: mpsc::Sender<TraceRecord>,
    shutdown_token: CancellationToken,
    done: Arc<Notify>,
}

impl TraceWriter {
    pub fn new(
        config: TraceWriterConfig,
        trace_dir: PathBuf,
        redactor: Arc<dyn TraceRedactor>,
    ) -> Result<Arc<Self>, TraceSinkError> {
        config.validate()?;
        let (tx, rx) = mpsc::channel(config.channel_capacity);
        let shutdown_token = CancellationToken::new();
        let done = Arc::new(Notify::new());
        tokio::spawn(run_writer(
            rx,
            trace_dir,
            config,
            redactor,
            shutdown_token.clone(),
            done.clone(),
        ));
        Ok(Arc::new(Self {
            tx,
            shutdown_token,
            done,
        }))
    }

    pub async fn shutdown_with_grace(&self) {
        if self.shutdown_token.is_cancelled() {
            return;
        }
        self.shutdown_token.cancel();
        self.done.notified().await;
    }
}

impl TraceSink for TraceWriter {
    fn try_write(&self, record: TraceRecord) -> Result<(), TraceSinkError> {
        match self.tx.try_send(record) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(record)) => {
                tracing::warn!(
                    trace_id = %record_trace_id(&record),
                    record_kind = %record_kind(&record),
                    error = "queue_full",
                    "aise.trace.queue_overflow"
                );
                Err(TraceSinkError::ChannelFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(TraceSinkError::ShuttingDown),
        }
    }
}

impl TraceSpanSink for TraceWriter {
    fn write_span(&self, trace_id: &TraceId, span: &TraceSpan) {
        let _ = self.try_write(TraceRecord::Span {
            trace_id: trace_id.clone(),
            span: span.clone(),
        });
    }

    fn write_trace(&self, trace: &TurnTrace) {
        let _ = self.try_write(TraceRecord::Completed(trace.clone()));
    }
}

async fn run_writer(
    mut rx: mpsc::Receiver<TraceRecord>,
    trace_dir: PathBuf,
    config: TraceWriterConfig,
    redactor: Arc<dyn TraceRedactor>,
    shutdown: CancellationToken,
    done: Arc<Notify>,
) {
    if let Err(error) = tokio::fs::create_dir_all(&trace_dir).await {
        tracing::warn!(path = %trace_dir.display(), error = %error, "aise.trace.create_dir_failed");
    }
    let grace = Duration::from_millis(config.shutdown_grace_ms);
    loop {
        tokio::select! {
            record = rx.recv() => {
                match record {
                    Some(record) => {
                        if let Err(error) = write_record(&trace_dir, &config, redactor.as_ref(), &record).await {
                            tracing::warn!(
                                trace_id = %record_trace_id(&record),
                                record_kind = %record_kind(&record),
                                error = %error,
                                "aise.trace.write_failed"
                            );
                        }
                    }
                    None => break,
                }
            }
            _ = shutdown.cancelled() => {
                drain_within(&mut rx, &trace_dir, &config, redactor.as_ref(), grace).await;
                break;
            }
        }
    }
    done.notify_waiters();
}
async fn drain_within(
    rx: &mut mpsc::Receiver<TraceRecord>,
    trace_dir: &Path,
    config: &TraceWriterConfig,
    redactor: &dyn TraceRedactor,
    grace: Duration,
) {
    let deadline = Instant::now() + grace;
    loop {
        while let Ok(record) = rx.try_recv() {
            if let Err(error) = write_record(trace_dir, config, redactor, &record).await {
                tracing::warn!(
                    trace_id = %record_trace_id(&record),
                    record_kind = %record_kind(&record),
                    error = %error,
                    "aise.trace.write_failed"
                );
            }
        }
        if Instant::now() >= deadline {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let poll = remaining.min(Duration::from_millis(25));
        match tokio::time::timeout(poll, rx.recv()).await {
            Ok(Some(record)) => {
                if let Err(error) = write_record(trace_dir, config, redactor, &record).await {
                    tracing::warn!(
                        trace_id = %record_trace_id(&record),
                        record_kind = %record_kind(&record),
                        error = %error,
                        "aise.trace.write_failed"
                    );
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

async fn write_record(
    trace_dir: &Path,
    config: &TraceWriterConfig,
    redactor: &dyn TraceRedactor,
    record: &TraceRecord,
) -> Result<(), TraceSinkError> {
    match record {
        TraceRecord::Span { trace_id, span } => {
            let value = serde_json::to_value(span).map_err(|e| TraceSinkError::Io(e.to_string()))?;
            let line = render_record(value, redactor, config.max_record_bytes)?;
            let path = trace_dir.join(format!("{}.jsonl", trace_id.as_str()));
            append_rotating(&path, &line, config.rotation_bytes).await?;
        }
        TraceRecord::Completed(trace) => {
            let value = serde_json::to_value(trace).map_err(|e| TraceSinkError::Io(e.to_string()))?;
            let body = render_record(value, redactor, config.max_record_bytes)?;
            let path = trace_dir.join(format!("{}.json", trace.trace_id.as_str()));
            write_rotating(&path, &body, config.rotation_bytes).await?;
        }
    }
    enforce_retention(trace_dir, config.retention_files).await;
    Ok(())
}

fn render_record(
    mut value: serde_json::Value,
    redactor: &dyn TraceRedactor,
    max_bytes: usize,
) -> Result<Vec<u8>, TraceSinkError> {
    redactor.redact_value(&mut value);
    let mut body = serde_json::to_vec(&value).map_err(|e| TraceSinkError::Io(e.to_string()))?;
    if body.len() > max_bytes {
        body.truncate(max_bytes);
    }
    body.push(b'\n');
    Ok(body)
}

async fn append_rotating(path: &Path, line: &[u8], rotation_bytes: u64) -> Result<(), TraceSinkError> {
    if rotation_bytes > 0 {
        let current = tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
        if current + line.len() as u64 > rotation_bytes {
            rotate(path).await?;
        }
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(io_error)?;
    file.write_all(line).await.map_err(io_error)?;
    file.flush().await.map_err(io_error)?;
    Ok(())
}

async fn write_rotating(path: &Path, body: &[u8], rotation_bytes: u64) -> Result<(), TraceSinkError> {
    if rotation_bytes > 0 {
        let current = tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
        if current > 0 && current + body.len() as u64 > rotation_bytes {
            rotate(path).await?;
        }
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .await
        .map_err(io_error)?;
    file.write_all(body).await.map_err(io_error)?;
    file.flush().await.map_err(io_error)?;
    Ok(())
}

async fn rotate(path: &Path) -> Result<(), TraceSinkError> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(());
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("trace");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    for seq in 1..10_000u32 {
        let rotated = path.with_file_name(format!("{stem}.{seq}.{ext}"));
        if !tokio::fs::try_exists(&rotated).await.unwrap_or(false) {
            tokio::fs::rename(path, &rotated).await.map_err(io_error)?;
            return Ok(());
        }
    }
    Ok(())
}

async fn enforce_retention(trace_dir: &Path, retention_files: usize) {
    if retention_files == 0 {
        return;
    }
    let Ok(mut entries) = tokio::fs::read_dir(trace_dir).await else {
        return;
    };
    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_file() {
                files.push((meta.modified().unwrap_or(UNIX_EPOCH), entry.path()));
            }
        }
    }
    if files.len() <= retention_files {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    let removed = files.len() - retention_files;
    for (_, path) in files.into_iter().take(removed) {
        if let Err(error) = tokio::fs::remove_file(&path).await {
            tracing::warn!(path = %path.display(), error = %error, "aise.trace.retention_remove_failed");
        }
    }
}

fn io_error(error: std::io::Error) -> TraceSinkError {
    TraceSinkError::Io(error.to_string())
}

fn record_trace_id(record: &TraceRecord) -> &str {
    match record {
        TraceRecord::Span { trace_id, .. } => trace_id.as_str(),
        TraceRecord::Completed(trace) => trace.trace_id.as_str(),
    }
}

fn record_kind(record: &TraceRecord) -> &'static str {
    match record {
        TraceRecord::Span { .. } => "span",
        TraceRecord::Completed(_) => "completed",
    }
}
