use super::*;

#[test]
fn known_stage_uses_readable_status() {
    assert_eq!(readable_stage("think"), "角色正在思考");
}

#[test]
fn unknown_stage_keeps_stage_name() {
    assert_eq!(readable_stage("custom"), "正在执行 custom");
}
