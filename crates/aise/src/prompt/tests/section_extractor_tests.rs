use super::*;

#[test]
fn extracts_multiple_sections_from_shared_file() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
system body
{# @endasset #}

{# @asset {"asset_id":"plan/user","slot_ids":["plan.user"]} #}
user body
{# @endasset #}
"#;

    let sections = extract_asset_sections("files/plan.md.j2", content).unwrap();

    assert_eq!(sections.len(), 2);
    assert_eq!(sections["plan/system"].slot_ids, vec!["plan.system"]);
    assert_eq!(sections["plan/system"].source_anchor, "files/plan.md.j2#plan/system");
    assert_eq!(sections["plan/system"].body, "system body\n");
    assert_eq!(sections["plan/user"].body, "user body\n");
}

#[test]
fn duplicate_asset_ids_fail() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
first
{# @endasset #}

{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
second
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("duplicate asset section"));
}

#[test]
fn nested_sections_fail() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
{# @asset {"asset_id":"plan/user","slot_ids":["plan.user"]} #}
body
{# @endasset #}
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("nested @asset block"));
}

#[test]
fn unexpected_endasset_fails() {
    let content = "{# @endasset #}\n";

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("unexpected @endasset"));
}

#[test]
fn unclosed_section_fails() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
system body
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("unclosed @asset block"));
}

#[test]
fn empty_asset_id_fails() {
    let content = r#"
{# @asset {"asset_id":"","slot_ids":["plan.system"]} #}
system body
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("empty asset_id"));
}

#[test]
fn empty_slot_ids_fail() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":[]} #}
system body
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("non-empty slot_ids array"));
}

#[test]
fn empty_slot_id_fails() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":[""]} #}
system body
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("contains an empty slot id"));
}

#[test]
fn content_outside_section_fails() {
    let content = r#"
unexpected content
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"]} #}
system body
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("outside @asset block"));
}

#[test]
fn template_without_sections_fails() {
    let err = extract_asset_sections("files/plan.md.j2", "\n\n").unwrap_err().to_string();

    assert!(err.contains("must use @asset / @endasset sections"));
}

#[test]
fn invalid_asset_metadata_json_fails() {
    let content = "{# @asset {not json} #}\nbody\n{# @endasset #}\n";

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("invalid @asset metadata JSON"));
}

#[test]
fn unknown_asset_metadata_field_fails() {
    let content = r#"
{# @asset {"asset_id":"plan/system","slot_ids":["plan.system"],"revision":1} #}
body
{# @endasset #}
"#;

    let err = extract_asset_sections("files/plan.md.j2", content).unwrap_err().to_string();

    assert!(err.contains("invalid @asset metadata JSON"));
    assert!(err.contains("unknown field"));
    assert!(err.contains("revision"));
}

#[test]
fn preserves_internal_body_bytes_while_trimming_structural_padding() {
    let content = "{# @asset {\"asset_id\":\"plan/system\",\"slot_ids\":[\"plan.system\"]} #}\r\n\r\nline 1\r\n\r\nline 2\r\n\r\n{# @endasset #}\r\n";

    let sections = extract_asset_sections("files/plan.md.j2", content).unwrap();

    assert_eq!(sections["plan/system"].body, "line 1\r\n\r\nline 2\r\n");
}
