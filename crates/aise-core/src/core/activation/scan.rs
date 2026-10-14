use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFragmentKind {
    PlayerContribution,
    PlayerRoleName,
    NarrativeDirection,
    RecentStory,
    StoryOpening,
    StorySummary,
    RecursionContent,
}

#[derive(Debug, Clone)]
pub struct ScanFragment {
    kind: ScanFragmentKind,
    depth: u16,
    case_sensitive_text: String,
    case_insensitive_text: String,
}

impl ScanFragment {
    pub fn new(kind: ScanFragmentKind, depth: u16, text: &str) -> Self {
        Self {
            kind,
            depth,
            case_sensitive_text: normalize_text(text, true),
            case_insensitive_text: normalize_text(text, false),
        }
    }

    pub fn kind(&self) -> ScanFragmentKind {
        self.kind
    }

    pub fn depth(&self) -> u16 {
        self.depth
    }

    pub fn is_empty(&self) -> bool {
        self.case_sensitive_text.is_empty()
    }

    pub(super) fn count(&self, needle: &str, case_sensitive: bool, whole_words: bool) -> usize {
        let haystack = if case_sensitive {
            &self.case_sensitive_text
        } else {
            &self.case_insensitive_text
        };
        literal_count(haystack, needle, whole_words)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanBuffer {
    fragments: Vec<ScanFragment>,
}

impl ScanBuffer {
    pub fn new(fragments: impl IntoIterator<Item = ScanFragment>) -> Self {
        let mut fragments = fragments
            .into_iter()
            .filter(|fragment| !fragment.is_empty() && fragment.kind != ScanFragmentKind::RecursionContent)
            .collect::<Vec<_>>();
        fragments.sort_by_key(|fragment| (fragment.kind, fragment.depth));
        Self { fragments }
    }

    pub fn fragments(&self) -> &[ScanFragment] {
        &self.fragments
    }
}

pub(super) fn normalize_text(value: &str, case_sensitive: bool) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        if case_sensitive {
            normalized.push(character);
        } else {
            normalized.extend(character.to_lowercase());
        }
    }
    normalized
}

fn literal_count(haystack: &str, needle: &str, whole_words: bool) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack
        .match_indices(needle)
        .filter(|(offset, value)| {
            !whole_words || {
                let before = haystack[..*offset].chars().next_back();
                let after = haystack[*offset + value.len()..].chars().next();
                !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
            }
        })
        .count()
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
#[path = "tests/scan_tests.rs"]
mod tests;
