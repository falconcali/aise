use crate::core::{CommittedTurnInfo, EngineError, TurnControl, TurnEventSink, TurnRequest, TurnResult};
use crate::trace::Trace;

pub struct Runtime {}

impl Runtime {
    pub fn new() -> Self {
        Runtime {}
    }

    pub async fn run_turn(
        &self,
        _turn_request: TurnRequest,
        _turn_control: TurnControl,
        _sink: &dyn TurnEventSink,
        _trace: &Trace,
    ) -> Result<TurnResult, EngineError> {
        Ok(TurnResult {
            result: CommittedTurnInfo {
                turn_number: 1,
                story_revision: 1,
                story_text: "This is a simulated story text from runtime.".to_string(),
            },
            replayed: false,
        })
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
