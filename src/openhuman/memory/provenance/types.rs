//! Provenance types for memory entries.
//!
//! Tracks where a memory entry came from (`MemorySource`) and how reliable
//! its content is (`ConfidenceLevel`). Stored as JSON in the
//! `memory_docs.provenance_json` column.

use serde::{Deserialize, Serialize};

/// Confidence level for a memory entry, ordered from most to least reliable.
///
/// The ordering is used by the decay scheduler and by agent reasoning about
/// how much weight to give a memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    /// Confirmed by explicit user action (correction, confirmation tool).
    Verified = 2,
    /// LLM-inferred or implied — not directly confirmed by the user.
    Inferred = 1,
    /// From an untrusted source (connector data, external skill, scraping).
    External = 0,
}

/// What produced this memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// Extracted from a chat conversation.
    ChatHistory,
    /// Imported from an uploaded file or document.
    UploadedData,
    /// Explicit user correction or preference statement.
    UserCorrection,
    /// Inferred by the LLM from context (not directly observed).
    LlmInferred,
    /// Data arriving through an external connector or skill.
    ExternalSkill,
}

impl MemorySource {
    /// Human-readable static string for each variant.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChatHistory => "chat_history",
            Self::UploadedData => "uploaded_data",
            Self::UserCorrection => "user_correction",
            Self::LlmInferred => "llm_inferred",
            Self::ExternalSkill => "external_skill",
        }
    }
}

impl std::fmt::Display for MemorySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Provenance metadata attached to a memory entry.
///
/// Stored as a JSON object in the `provenance_json` column of `memory_docs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// What produced this memory entry.
    pub source: MemorySource,
    /// How reliable this memory entry is.
    pub confidence: ConfidenceLevel,
    /// Free-text description of the action that produced this memory,
    /// e.g. `"dadou_correct_preference: 'use dark theme'"`.
    #[serde(default)]
    pub source_detail: String,
}

impl Default for Provenance {
    /// Returns a backward-compatible default: chat-history source, inferred
    /// confidence, empty detail.
    fn default() -> Self {
        Self {
            source: MemorySource::ChatHistory,
            confidence: ConfidenceLevel::Inferred,
            source_detail: String::new(),
        }
    }
}

impl ConfidenceLevel {
    /// Returns a human-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Inferred => "inferred",
            Self::External => "external",
        }
    }
}

impl std::fmt::Display for ConfidenceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: Provenance serializes to/from JSON preserving all fields ──

    #[test]
    fn provenance_roundtrip_preserves_all_fields() {
        let p = Provenance {
            source: MemorySource::UserCorrection,
            confidence: ConfidenceLevel::Verified,
            source_detail: "dadou_correct_preference: 'use dark theme'".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, MemorySource::UserCorrection);
        assert_eq!(back.confidence, ConfidenceLevel::Verified);
        assert_eq!(
            back.source_detail,
            "dadou_correct_preference: 'use dark theme'"
        );
    }

    // ── Test 2: ConfidenceLevel has ordering: Verified > Inferred > External ──

    #[test]
    fn confidence_level_ordering() {
        assert!(ConfidenceLevel::Verified > ConfidenceLevel::Inferred);
        assert!(ConfidenceLevel::Inferred > ConfidenceLevel::External);
        assert!(ConfidenceLevel::Verified > ConfidenceLevel::External);
        assert_eq!(ConfidenceLevel::Inferred, ConfidenceLevel::Inferred);
    }

    // ── Test 3: MemorySource round-trips through serde with snake_case ──

    #[test]
    fn memory_source_serde_snake_case() {
        let sources = [
            MemorySource::ChatHistory,
            MemorySource::UploadedData,
            MemorySource::UserCorrection,
            MemorySource::LlmInferred,
            MemorySource::ExternalSkill,
        ];
        let expected = [
            "\"chat_history\"",
            "\"uploaded_data\"",
            "\"user_correction\"",
            "\"llm_inferred\"",
            "\"external_skill\"",
        ];
        for (src, exp) in sources.iter().zip(expected.iter()) {
            let json = serde_json::to_string(src).unwrap();
            assert_eq!(json, *exp, "expected {exp}, got {json}");
        }
        // Round-trip
        for src in &sources {
            let json = serde_json::to_string(src).unwrap();
            let back: MemorySource = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *src);
        }
    }

    // ── Test 4: Default provenance matches backward-compat expectations ──

    #[test]
    fn default_provenance_is_backward_compatible() {
        let p = Provenance::default();
        assert_eq!(p.source, MemorySource::ChatHistory);
        assert_eq!(p.confidence, ConfidenceLevel::Inferred);
        assert!(p.source_detail.is_empty());
    }

    // ── Test 5: MemorySource as_str and Display ──

    #[test]
    fn memory_source_as_str_and_display() {
        assert_eq!(MemorySource::ChatHistory.as_str(), "chat_history");
        assert_eq!(MemorySource::ChatHistory.to_string(), "chat_history");
        assert_eq!(MemorySource::ExternalSkill.as_str(), "external_skill");
        assert_eq!(MemorySource::ExternalSkill.to_string(), "external_skill");
    }

    // ── Test 6: ConfidenceLevel as_str and Display ──

    #[test]
    fn confidence_level_as_str_and_display() {
        assert_eq!(ConfidenceLevel::Verified.as_str(), "verified");
        assert_eq!(ConfidenceLevel::Verified.to_string(), "verified");
        assert_eq!(ConfidenceLevel::External.as_str(), "external");
        assert_eq!(ConfidenceLevel::External.to_string(), "external");
    }
}
