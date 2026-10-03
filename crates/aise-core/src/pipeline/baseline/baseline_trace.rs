use crate::core::PlayerContribution;
use crate::trace::{Observation, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus};

pub fn begin_player_contribution(observation: &Observation, player_input: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "player_contribution",
        kind: ObservationKind::Span,
        input: observation.capture_content(&player_input),
        metadata: Vec::new(),
    })
}

pub fn finish_player_contribution(observation: Observation, player_contribution: &PlayerContribution) {
    let output = observation.capture_content(player_contribution);
    observation.finish(ObservationOutcome {
        status: ObservationStatus::Ok,
        output,
        ..ObservationOutcome::default()
    });
}
