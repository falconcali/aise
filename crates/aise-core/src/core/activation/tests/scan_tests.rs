use super::*;

#[test]
fn normalization_collapses_whitespace_and_folds_case() {
    assert_eq!(normalize_text("  West\t\n LAKE  ", false), "west lake");
    assert_eq!(normalize_text("  West\t\n LAKE  ", true), "West LAKE");
}

#[test]
fn literal_matching_respects_case_and_whole_words() {
    let fragment = ScanFragment::new(
        ScanFragmentKind::PlayerContribution,
        0,
        "The Gate, the gatekeeper and the GATE.",
    );

    assert_eq!(fragment.count("gate", false, false), 3);
    assert_eq!(fragment.count("gate", false, true), 2);
    assert_eq!(fragment.count("gate", true, false), 1);
    assert_eq!(fragment.count("gate", true, true), 0);
    assert_eq!(fragment.count("", false, false), 0);
}

#[test]
fn chinese_text_matches_without_word_boundaries() {
    let fragment = ScanFragment::new(ScanFragmentKind::RecentStory, 1, "许仙在西湖边遇见了白娘子。");

    assert_eq!(fragment.count("白娘子", false, false), 1);
}

#[test]
fn buffer_orders_by_source_priority_and_drops_empty_fragments() {
    let buffer = ScanBuffer::new([
        ScanFragment::new(ScanFragmentKind::StorySummary, 3, "summary"),
        ScanFragment::new(ScanFragmentKind::RecentStory, 2, "older"),
        ScanFragment::new(ScanFragmentKind::RecentStory, 1, "newer"),
        ScanFragment::new(ScanFragmentKind::PlayerContribution, 0, "   "),
        ScanFragment::new(ScanFragmentKind::PlayerRoleName, 0, "许仙"),
        ScanFragment::new(ScanFragmentKind::RecursionContent, 1, "not base text"),
    ]);

    let order = buffer
        .fragments()
        .iter()
        .map(|fragment| (fragment.kind(), fragment.depth()))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            (ScanFragmentKind::PlayerRoleName, 0),
            (ScanFragmentKind::RecentStory, 1),
            (ScanFragmentKind::RecentStory, 2),
            (ScanFragmentKind::StorySummary, 3),
        ]
    );
}
