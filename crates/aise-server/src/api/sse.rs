use aise::turn::turn_contract::TurnCancellation;
use aise_core::core::{TurnEvent, TurnEventDeliveryError, TurnEventSink};
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::mpsc;

pub const SSE_CHANNEL_CAPACITY: usize = 64;

pub struct SseSink {
    progress_tx: mpsc::Sender<Event>,
    terminal_tx: mpsc::Sender<Event>,
    terminal_sent: AtomicBool,
    dropped: AtomicUsize,
}

impl SseSink {
    pub fn new(progress_tx: mpsc::Sender<Event>, terminal_tx: mpsc::Sender<Event>) -> Self {
        Self {
            progress_tx,
            terminal_tx,
            terminal_sent: AtomicBool::new(false),
            dropped: AtomicUsize::new(0),
        }
    }

    pub fn new_shared(tx: mpsc::Sender<Event>) -> Self {
        Self::new(tx.clone(), tx)
    }

    pub fn dropped_events(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    fn to_sse(&self, event: &TurnEvent) -> Option<Event> {
        let (name, payload) = match event {
            TurnEvent::StageStarted { stage } => ("stage", serde_json::json!({ "stage": stage })),
            TurnEvent::Committed { result, replayed } => (
                "committed",
                serde_json::json!({
                    "turn_number": result.turn_number,
                    "story_revision": result.story_revision,
                    "story_text": result.story_text,
                    "replayed": replayed,
                }),
            ),
            TurnEvent::Failed { code } => ("failed", serde_json::json!({ "code": code })),
            TurnEvent::Cancelled { code } => ("cancelled", serde_json::json!({ "code": code })),
            TurnEvent::Conflict { code } => ("conflict", serde_json::json!({ "code": code })),
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
                Err(mpsc::error::TrySendError::Full(_)) => Err(TurnEventDeliveryError::Backpressure),
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    Err(TurnEventDeliveryError::Disconnected)
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
                    Err(TurnEventDeliveryError::Backpressure)
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        error_kind = "client_disconnected",
                        dropped_events = self.dropped.load(Ordering::Relaxed),
                        "sse client disconnected during progress delivery"
                    );
                    Err(TurnEventDeliveryError::Disconnected)
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
