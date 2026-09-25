use super::model::{BoundedContent, ContentCapturePolicy, ObservabilityContentConfig};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

#[derive(Clone)]
pub struct ContentCapture {
    config: ObservabilityContentConfig,
}

impl ContentCapture {
    pub fn new(config: ObservabilityContentConfig) -> Self {
        Self { config }
    }

    pub fn encode<T: Serialize>(&self, value: &T, remaining_observation_bytes: usize) -> CaptureResult {
        if self.config.policy == ContentCapturePolicy::MetadataOnly {
            return CaptureResult {
                content: None,
                encode_failed: false,
            };
        }
        let field_limit = self.config.max_field_bytes.saturating_add(self.config.detector_overlap_bytes);
        let observation_limit = remaining_observation_bytes
            .min(self.config.max_observation_bytes)
            .saturating_add(self.config.detector_overlap_bytes);
        let mut writer = BoundedHashWriter::new(field_limit.min(observation_limit));
        match serde_json::to_writer(&mut writer, value) {
            Ok(()) => CaptureResult {
                content: Some(writer.finish()),
                encode_failed: false,
            },
            Err(_) => CaptureResult {
                content: None,
                encode_failed: true,
            },
        }
    }

    pub fn max_observation_bytes(&self) -> usize {
        self.config.max_observation_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureResult {
    pub content: Option<BoundedContent>,
    pub encode_failed: bool,
}

struct BoundedHashWriter {
    captured: Vec<u8>,
    limit: usize,
    original_bytes: usize,
    hasher: Sha256,
}

impl BoundedHashWriter {
    fn new(limit: usize) -> Self {
        Self {
            captured: Vec::new(),
            limit,
            original_bytes: 0,
            hasher: Sha256::new(),
        }
    }

    fn finish(mut self) -> BoundedContent {
        while std::str::from_utf8(&self.captured).is_err() {
            self.captured.pop();
        }
        let captured_bytes = self.captured.len();
        let json = String::from_utf8(self.captured).unwrap_or_default();
        BoundedContent {
            json,
            original_bytes: self.original_bytes,
            captured_bytes,
            truncated: self.original_bytes > captured_bytes,
            sha256: format!("{:x}", self.hasher.finalize()),
        }
    }
}

impl Write for BoundedHashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.update(bytes);
        self.original_bytes = self.original_bytes.saturating_add(bytes.len());
        let remaining = self.limit.saturating_sub(self.captured.len());
        self.captured.extend_from_slice(&bytes[..remaining.min(bytes.len())]);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
