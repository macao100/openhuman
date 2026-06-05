//! Types for the semantic skill router.

use serde::{Deserialize, Serialize};

/// A single skill match returned by the router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    /// Skill identifier (human-readable name).
    pub skill_name: String,
    /// Cosine similarity score (0.0 – 1.0) or Jaccard overlap.
    pub score: f64,
    /// Skill description for display.
    pub description: String,
}

/// Pre-computed embedding for a skill.
#[derive(Debug, Clone)]
pub struct SkillEmbedding {
    pub skill_name: String,
    pub description: String,
    /// Dense embedding vector (dimensions vary by provider).
    pub embedding: Vec<f32>,
}

/// Query routed through the semantic skill matcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteQuery {
    pub query: String,
    /// Number of matches to return (default: 3).
    pub top_k: Option<usize>,
}

/// Result of a routing query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    /// Top-k matching skills, ordered by score descending.
    pub matches: Vec<SkillMatch>,
    /// Microseconds elapsed for the lookup (should be <5000).
    pub elapsed_us: u64,
    /// Whether the result used embeddings or keyword fallback.
    pub method: String,
}
