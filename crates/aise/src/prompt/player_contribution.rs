use crate::domain::turn::{InterpretedPlayerContribution, PlayerContributionKind};

pub fn render_interpreted_player_contribution(value: &InterpretedPlayerContribution) -> String {
    value
        .units
        .iter()
        .map(|unit| {
            let kind = match unit.kind {
                PlayerContributionKind::Speech => "speech",
                PlayerContributionKind::Action => "action",
                PlayerContributionKind::PrivateState => "private_state",
                PlayerContributionKind::RequestedOutcome => "requested_outcome",
            };
            format!("- kind: {kind}\n  content: {}", quoted(unit.content.as_str()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
#[path = "tests/player_contribution_tests.rs"]
mod tests;
