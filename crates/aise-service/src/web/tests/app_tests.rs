use super::*;

#[test]
fn player_input_rejects_blank_message() {
    let error = validate_player_input(" \n ", 32).expect_err("blank input should fail");
    assert_eq!(error.0, StatusCode::BAD_REQUEST);
}

#[test]
fn player_input_rejects_oversized_message() {
    let error = validate_player_input("你好", 5).expect_err("oversized input should fail");
    assert_eq!(error.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn player_input_accepts_non_empty_message_within_limit() {
    validate_player_input("你好", 6).expect("bounded non-empty input should pass");
}
