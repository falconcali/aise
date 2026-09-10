use crate::domain::asset::ids::Sha256Digest;
use crate::domain::asset::validation::BoundedText;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFragmentKind {
    PlayerContribution,
    RecentStory,
    StorySummary,
    PlayerRoleName,
    PlayerRoleLabel,
    NarrativeDirection,
    NarrativeEvent,
    RecursionContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScanFragmentId(Sha256Digest);

#[derive(Debug, Clone)]
pub struct ScanFragment {
    pub id: ScanFragmentId,
    pub kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_order: u32,
    pub content_hash: Sha256Digest,
    pub text: BoundedText,
}

#[derive(Debug, Clone)]
pub struct ActivationScanBuffer {
    fragments: Vec<ScanFragment>,
}

impl ScanFragmentId {
    pub fn from_parts(kind: ScanFragmentKind, recency_depth: u16, stable_order: u32, text: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(format!("{kind:?}:{recency_depth}:{stable_order}:").as_bytes());
        hasher.update(text.as_bytes());
        Self(Sha256Digest::from_bytes(hasher.finalize().into()))
    }

    pub fn digest(&self) -> &Sha256Digest {
        &self.0
    }
}

impl ScanFragment {
    pub fn try_new(kind: ScanFragmentKind, recency_depth: u16, stable_order: u32, text: BoundedText) -> Self {
        let content_hash = digest(text.as_str());
        Self {
            id: ScanFragmentId::from_parts(kind, recency_depth, stable_order, text.as_str()),
            kind,
            recency_depth,
            stable_order,
            content_hash,
            text,
        }
    }
}

impl ActivationScanBuffer {
    pub fn try_new(
        mut fragments: Vec<ScanFragment>,
        max_fragments: usize,
        max_bytes: usize,
    ) -> Result<Self, ScanBufferError> {
        if max_fragments == 0 || max_bytes == 0 {
            return Err(ScanBufferError::InvalidLimit);
        }
        if fragments.len() > max_fragments {
            return Err(ScanBufferError::FragmentLimit);
        }
        let bytes = fragments
            .iter()
            .try_fold(0usize, |total, fragment| total.checked_add(fragment.text.as_str().len()))
            .ok_or(ScanBufferError::ByteLimit)?;
        if bytes > max_bytes {
            return Err(ScanBufferError::ByteLimit);
        }
        fragments
            .sort_by_key(|fragment| (source_priority(fragment.kind), fragment.recency_depth, fragment.stable_order));
        Ok(Self { fragments })
    }

    pub fn fragments(&self) -> &[ScanFragment] {
        &self.fragments
    }

    pub fn visible_at_depth(&self, depth: u16) -> impl Iterator<Item = &ScanFragment> {
        self.fragments.iter().filter(move |fragment| fragment.recency_depth <= depth)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScanBufferError {
    #[error("scan buffer limit must be positive")]
    InvalidLimit,
    #[error("scan buffer fragment limit exceeded")]
    FragmentLimit,
    #[error("scan buffer byte limit exceeded")]
    ByteLimit,
}

fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(value.as_bytes()).into())
}

fn source_priority(kind: ScanFragmentKind) -> u8 {
    match kind {
        ScanFragmentKind::PlayerContribution => 0,
        ScanFragmentKind::PlayerRoleName | ScanFragmentKind::PlayerRoleLabel => 1,
        ScanFragmentKind::NarrativeDirection | ScanFragmentKind::NarrativeEvent => 2,
        ScanFragmentKind::RecentStory => 3,
        ScanFragmentKind::StorySummary => 4,
        ScanFragmentKind::RecursionContent => 5,
    }
}
