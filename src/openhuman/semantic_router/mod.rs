//! Semantic skill router — embedding-based skill discovery.
//!
//! Ranks installed skills against a user query using local embedding similarity.
//! Falls back to keyword (Jaccard) matching when no embedder is configured.
//! Everything runs on-device with zero external API calls.

pub mod ops;
pub mod schemas;
pub mod types;
