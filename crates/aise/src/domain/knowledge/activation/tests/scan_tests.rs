use super::*;

#[test]
fn empty_scan_buffer_respects_positive_limits() {
    let buffer = ActivationScanBuffer::try_new(Vec::new(), 1, 1).unwrap();
    assert!(buffer.fragments().is_empty());
}
