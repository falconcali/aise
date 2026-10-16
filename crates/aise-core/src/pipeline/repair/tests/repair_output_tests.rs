use super::*;

fn config(max_proposal_bytes: usize) -> RepairConfig {
    RepairConfig { max_proposal_bytes }
}

#[test]
fn rejects_empty_output() {
    let result = validate_repaired_proposal(String::new(), &config(16));

    assert!(matches!(
        result,
        Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Repair
        })
    ));
}

#[test]
fn rejects_whitespace_only_output() {
    let result = validate_repaired_proposal(" \n\t".to_owned(), &config(16));

    assert!(matches!(
        result,
        Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Repair
        })
    ));
}

#[test]
fn accepts_exact_byte_boundary() {
    let raw = "éé".to_owned();

    assert_eq!(
        validate_repaired_proposal(raw.clone(), &config(4)).expect("boundary accepted"),
        raw
    );
}

#[test]
fn rejects_oversized_output() {
    let result = validate_repaired_proposal("12345".to_owned(), &config(4));

    assert!(matches!(
        result,
        Err(PipelineError::InvalidOutput {
            stage: PipelineStage::Repair,
            code: "repair_proposal_too_long",
            ..
        })
    ));
}

#[test]
fn returns_valid_output_unchanged() {
    let raw = "  ```text\nstory\n```  ".to_owned();

    assert_eq!(validate_repaired_proposal(raw.clone(), &config(64)).expect("valid output"), raw);
}
