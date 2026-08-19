use super::*;
use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::{InterpretedPlayerContribution, PlayerContributionUnit};

#[test]
fn renders_units_in_order_with_kind_and_content() {
    let value = InterpretedPlayerContribution {
        units: vec![
            PlayerContributionUnit {
                kind: PlayerContributionKind::Action,
                content: BoundedText::try_new("后退一步".to_owned(), "content", 128).unwrap(),
            },
            PlayerContributionUnit {
                kind: PlayerContributionKind::Speech,
                content: BoundedText::try_new("你是谁".to_owned(), "content", 128).unwrap(),
            },
        ],
    };

    assert_eq!(
        render_interpreted_player_contribution(&value),
        "- kind: action\n  content: \"后退一步\"\n- kind: speech\n  content: \"你是谁\""
    );
}
