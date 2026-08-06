use aise::TurnEvent;
use aise::TurnEventSink;
use aise::core::turn_contract::TurnCancellation;
use aise::core::turn_event::TurnEventDeliveryError;
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::mpsc;

pub const SSE_CHANNEL_CAPACITY: usize = 64;

pub struct SseSink {
    progress_tx: mpsc::Sender<Event>,
    terminal_tx: mpsc::Sender<Event>,
    include_trace: bool,
    terminal_sent: AtomicBool,
    dropped: AtomicUsize,
}

impl SseSink {
    pub fn new(progress_tx: mpsc::Sender<Event>, terminal_tx: mpsc::Sender<Event>, include_trace: bool) -> Self {
        Self {
            progress_tx,
            terminal_tx,
            include_trace,
            terminal_sent: AtomicBool::new(false),
            dropped: AtomicUsize::new(0),
        }
    }

    pub fn new_shared(tx: mpsc::Sender<Event>, include_trace: bool) -> Self {
        Self::new(tx.clone(), tx, include_trace)
    }

    pub fn dropped_events(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    fn to_sse(&self, event: &TurnEvent) -> Option<Event> {
        let (name, payload) = match event {
            TurnEvent::StageStarted { stage, .. } => ("stage", serde_json::json!({ "stage": stage.as_str() })),
            TurnEvent::ValidationCompleted {
                attempt,
                decision,
                issue_codes,
                ..
            } => (
                "validation",
                serde_json::json!({
                    "attempt": attempt,
                    "decision": decision.as_str(),
                    "issue_codes": issue_codes.iter().map(|code| code.as_str()).collect::<Vec<_>>(),
                }),
            ),
            TurnEvent::Committed { result, replayed } => (
                "committed",
                serde_json::json!({
                    "turn_id": result.turn_id.as_str(),
                    "story_revision": result.story_revision.get(),
                    "replayed": replayed,
                }),
            ),
            TurnEvent::Failed { turn_id, code } => {
                ("failed", serde_json::json!({ "turn_id": turn_id.as_str(), "code": code }))
            }
            TurnEvent::Cancelled { turn_id, code } => {
                ("cancelled", serde_json::json!({ "turn_id": turn_id.as_str(), "code": code }))
            }
            TurnEvent::Conflict { turn_id, code } => {
                ("conflict", serde_json::json!({ "turn_id": turn_id.as_str(), "code": code }))
            }
            TurnEvent::TraceCompleted { turn_id, trace_id } => {
                if !self.include_trace {
                    return None;
                }
                (
                    "trace",
                    serde_json::json!({ "turn_id": turn_id.as_str(), "trace_id": trace_id.as_str() }),
                )
            }
        };
        let data = match serde_json::to_string(&payload) {
            Ok(data) => data,
            Err(error) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(error = %error, "failed to serialize sse event payload");
                return None;
            }
        };
        Some(Event::default().event(name).data(data))
    }
}

impl TurnEventSink for SseSink {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError> {
        let sse = match self.to_sse(&event) {
            Some(sse) => sse,
            None => return Ok(()),
        };
        if event.is_terminal() {
            if self.terminal_sent.swap(true, Ordering::SeqCst) {
                return Err(TurnEventDeliveryError::TerminalAlreadySent);
            }
            match self.terminal_tx.try_send(sse) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => Err(TurnEventDeliveryError::ProgressBackpressure),
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    Err(TurnEventDeliveryError::ClientDisconnected)
                }
            }
        } else {
            match self.progress_tx.try_send(sse) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(
                        error_kind = "progress_backpressure",
                        dropped_events = self.dropped.load(Ordering::Relaxed),
                        "sse progress lane saturated"
                    );
                    Err(TurnEventDeliveryError::ProgressBackpressure)
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        error_kind = "client_disconnected",
                        dropped_events = self.dropped.load(Ordering::Relaxed),
                        "sse client disconnected during progress delivery"
                    );
                    Err(TurnEventDeliveryError::ClientDisconnected)
                }
            }
        }
    }
}

pub struct ClientDisconnectGuard(TurnCancellation);

impl ClientDisconnectGuard {
    pub fn new(cancellation: TurnCancellation) -> Self {
        Self(cancellation)
    }
}

impl Drop for ClientDisconnectGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub fn sse_stream(
    rx: mpsc::Receiver<Event>,
    guard: ClientDisconnectGuard,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct State {
        rx: mpsc::Receiver<Event>,
        _guard: ClientDisconnectGuard,
    }
    futures::stream::unfold(State { rx, _guard: guard }, |mut state| async move {
        state.rx.recv().await.map(|event| (Ok::<_, Infallible>(event), state))
    })
}

pub fn sse_merged_stream(
    progress_rx: mpsc::Receiver<Event>,
    terminal_rx: mpsc::Receiver<Event>,
    guard: ClientDisconnectGuard,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct State {
        progress: mpsc::Receiver<Event>,
        terminal: mpsc::Receiver<Event>,
        terminal_done: bool,
        _guard: ClientDisconnectGuard,
    }
    futures::stream::unfold(
        State {
            progress: progress_rx,
            terminal: terminal_rx,
            terminal_done: false,
            _guard: guard,
        },
        |mut state| async move {
            loop {
                if !state.terminal_done {
                    match state.terminal.try_recv() {
                        Ok(event) => {
                            state.terminal_done = true;
                            return Some((Ok::<_, Infallible>(event), state));
                        }
                        Err(mpsc::error::TryRecvError::Empty) => {}
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            state.terminal_done = true;
                        }
                    }
                }
                tokio::select! {
                    event = state.progress.recv() => match event {
                        Some(event) => return Some((Ok::<_, Infallible>(event), state)),
                        None => {
                            if state.terminal_done {
                                return None;
                            }
                            match state.terminal.recv().await {
                                Some(event) => {
                                    state.terminal_done = true;
                                    return Some((Ok::<_, Infallible>(event), state));
                                }
                                None => return None,
                            }
                        }
                    },
                    event = state.terminal.recv() => match event {
                        Some(event) => {
                            state.terminal_done = true;
                            return Some((Ok::<_, Infallible>(event), state));
                        }
                        None => {
                            state.terminal_done = true;
                        }
                    },
                }
            }
        },
    )
}
