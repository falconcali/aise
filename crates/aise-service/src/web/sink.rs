use aise_core::core::{TurnCancellation, TurnEvent, TurnEventSink};
use axum::response::sse::Event;
use futures::Stream;
use serde_json::json;
use std::convert::Infallible;
use tokio::sync::mpsc;

pub struct WebTurnEventSink {
    sender: mpsc::Sender<Event>,
    cancellation: TurnCancellation,
}

impl WebTurnEventSink {
    pub fn new(sender: mpsc::Sender<Event>, cancellation: TurnCancellation) -> Self {
        Self { sender, cancellation }
    }
}

impl TurnEventSink for WebTurnEventSink {
    fn emit(&self, event: TurnEvent) {
        let event = match event {
            TurnEvent::TurnStarted | TurnEvent::TurnCommitted => return,
            TurnEvent::TurnPipeline { stage } => status_event(&readable_stage(&stage)),
        };
        match self.sender.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(error_kind = "progress_backpressure", "web turn progress event dropped");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.cancellation.cancel();
                tracing::warn!(error_kind = "client_disconnected", "web turn progress receiver closed");
            }
        }
    }
}

pub fn committed_event(text: &str, turn_number: u64) -> Event {
    Event::default()
        .event("committed")
        .data(json!({ "text": text, "turn_number": turn_number }).to_string())
}

pub fn error_event(message: &str) -> Event {
    Event::default().event("error").data(json!({ "message": message }).to_string())
}

pub fn event_stream(
    receiver: mpsc::Receiver<Event>,
    cancellation: TurnCancellation,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct StreamState {
        receiver: mpsc::Receiver<Event>,
        _guard: DisconnectGuard,
    }

    futures::stream::unfold(
        StreamState {
            receiver,
            _guard: DisconnectGuard(cancellation),
        },
        |mut state| async move { state.receiver.recv().await.map(|event| (Ok::<_, Infallible>(event), state)) },
    )
}

fn status_event(label: &str) -> Event {
    Event::default().event("status").data(json!({ "label": label }).to_string())
}

fn readable_stage(stage: &str) -> String {
    match stage {
        "baseline" => "正在搭建初始环境".to_owned(),
        "plan" => "正在规划剧情走向".to_owned(),
        "retrieval" => "正在检索相关记忆".to_owned(),
        "think" => "角色正在思考".to_owned(),
        "generate" => "正在生成故事内容".to_owned(),
        "validate" => "正在检查故事一致性".to_owned(),
        "repair" => "正在修复故事内容".to_owned(),
        "extract" => "正在提取世界状态变化".to_owned(),
        "summary" => "正在整理故事摘要".to_owned(),
        "commit" => "正在提交故事内容".to_owned(),
        other => format!("正在执行 {other}"),
    }
}

struct DisconnectGuard(TurnCancellation);

impl Drop for DisconnectGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
#[path = "tests/sink_tests.rs"]
mod tests;
