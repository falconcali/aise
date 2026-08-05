use aise::TurnEvent;
use aise::TurnEventSink;
use aise::core::turn_contract::TurnCancellation;
use axum::response::sse::Event;
use futures::channel::mpsc::{Receiver, Sender};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const SSE_CHANNEL_CAPACITY: usize = 64;

pub struct SseSink {
    tx: Mutex<Sender<Event>>,
    include_trace: bool,
    dropped: AtomicUsize,
}

impl SseSink {
    pub fn new(tx: Sender<Event>, include_trace: bool) -> Self {
        Self {
            tx: Mutex::new(tx),
            include_trace,
            dropped: AtomicUsize::new(0),
        }
    }

    pub fn dropped_events(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl TurnEventSink for SseSink {
    fn emit(&self, event: TurnEvent) {
        let sse = match event {
            TurnEvent::StageStarted(stage) => Event::default().event("stage").data(stage.as_str()),
            TurnEvent::Token(text) => Event::default().event("token").data(text),
            TurnEvent::Validation { pass } => {
                Event::default().event("validation").data(if pass { "pass" } else { "fail" })
            }
            TurnEvent::Finished { turn_id } => Event::default().event("done").data(turn_id.to_string()),
            TurnEvent::Trace(trace) => {
                if !self.include_trace {
                    return;
                }
                match serde_json::to_string(&trace) {
                    Ok(json) => Event::default().event("trace").data(json),
                    Err(_) => return,
                }
            }
        };
        if let Err(error) = self.tx.lock().unwrap().try_send(sse) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                error = %error,
                dropped_events = self.dropped.load(Ordering::Relaxed),
                "sse channel is full; dropping observer event"
            );
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

pub fn sse_stream(rx: Receiver<Event>, guard: ClientDisconnectGuard) -> impl Stream<Item = Result<Event, Infallible>> {
    struct State {
        rx: Receiver<Event>,
        _guard: ClientDisconnectGuard,
    }
    futures::stream::unfold(State { rx, _guard: guard }, |mut state| async move {
        state.rx.next().await.map(|event| (Ok::<_, Infallible>(event), state))
    })
}
