use super::*;

#[test]
fn parse_slots_yaml_basic() {
    let yaml = r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    required: true
  - slot_id: intent.few_shot
    allowed_kinds: [few_shot, messages]
    required: false
"#;

    let registry = parse_slots_yaml(yaml).unwrap();

    assert_eq!(registry.len(), 2);
    assert_eq!(registry["intent.analysis"].allowed_kinds, vec![PromptKind::Text]);
    assert!(!registry["intent.few_shot"].required);
}

#[test]
fn parse_slots_yaml_applies_defaults() {
    let yaml = r#"
slots:
  - slot_id: minimal
    allowed_kinds: [text]
"#;

    let registry = parse_slots_yaml(yaml).unwrap();
    let slot = &registry["minimal"];

    assert!(slot.required);
    assert!(!slot.structured_output);
    assert!(!slot.output_contract_required);
    assert!(!slot.optimizable);
    assert!(!slot.allow_child_render);
    assert!(slot.notes.is_none());
    assert!(slot.vars.is_empty());
    assert!(slot.output_contract.is_none());
}

#[test]
fn parse_slots_yaml_reads_vars_and_output_contract() {
    let yaml = r#"
slots:
  - slot_id: plan.user
    allowed_kinds: [text]
    vars:
      - { name: title, var_type: string, required: true }
      - { name: context, var_type: object }
    output_contract:
      min_length: 10
      must_contain: ["title"]
"#;

    let registry = parse_slots_yaml(yaml).unwrap();
    let slot = &registry["plan.user"];

    assert_eq!(slot.vars.len(), 2);
    assert_eq!(slot.vars[0].name, "title");
    assert_eq!(slot.vars[0].var_type, VarType::String);
    assert!(slot.vars[0].required);
    assert_eq!(slot.vars[1].var_type, VarType::Object);
    assert_eq!(slot.output_contract.as_ref().unwrap().min_length, Some(10));
    assert_eq!(slot.output_contract.as_ref().unwrap().must_contain, vec!["title".to_string()]);
}

#[test]
fn parse_slots_yaml_rejects_duplicate_slot_ids() {
    let yaml = r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#;

    let err = parse_slots_yaml(yaml).unwrap_err().to_string();

    assert!(err.contains("duplicate slot_id `intent.analysis`"));
}

#[test]
fn parse_slots_yaml_rejects_unsupported_options() {
    let yaml = r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    structured_output: true
    optimizable: true
"#;

    let err = parse_slots_yaml(yaml).unwrap_err().to_string();

    assert!(err.contains("unsupported options"));
    assert!(err.contains("structured_output"));
    assert!(err.contains("optimizable"));
}

#[test]
fn parse_slots_yaml_rejects_unknown_fields() {
    let yaml = r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    requiredd: true
"#;

    let err = parse_slots_yaml(yaml).unwrap_err().to_string();

    assert!(err.contains("unknown field"));
    assert!(err.contains("requiredd"));
}

#[test]
fn accepts_kind_checks_membership() {
    let slot = SlotSpec {
        slot_id: "test.slot".into(),
        allowed_kinds: vec![PromptKind::Text, PromptKind::Fragment],
        required: true,
        structured_output: false,
        output_contract_required: false,
        optimizable: false,
        allow_child_render: false,
        notes: None,
        vars: Vec::new(),
        output_contract: None,
    };

    assert!(slot.accepts_kind(&PromptKind::Text));
    assert!(slot.accepts_kind(&PromptKind::Fragment));
    assert!(!slot.accepts_kind(&PromptKind::Messages));
}
