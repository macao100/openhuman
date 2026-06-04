//! Provenance tracking for memory entries.
//!
//! Tracks `source` (what produced this memory) and `confidence` (how reliable
//! it is) for every memory entry. Structured as a JSON object stored in the
//! `provenance_json` column of `memory_docs`.
//!
//! Exposes:
//! - [`types`] — `Provenance`, `ConfidenceLevel`, `MemorySource` type definitions.
//! - [`migration`] — Idempotent SQLite schema migration for the provenance column.
//! - [`decay`] — Confidence decay scheduler (demotes/removes old entries).
//! - [`schemas`] — Controller schemas for provenance RPCs.

pub mod types;
pub mod migration;
pub mod decay;
pub mod schemas;

pub use types::{ConfidenceLevel, MemorySource, Provenance};
