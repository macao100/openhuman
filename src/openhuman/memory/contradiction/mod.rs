//! Contradiction detection and resolution for DADOU memory.
//!
//! When DADOU learns a new fact that conflicts with a previously verified
//! memory entry, this module detects the conflict and lets the user resolve
//! it (replace, merge, or dismiss) before the new entry is committed.
//!
//! ## Detection strategy
//!
//! 1. Only trigger when the **new** entry has `confidence == Verified`.
//! 2. Search for semantically-similar entries via `recall_relevant_by_vector`.
//! 3. Filter results to existing entries that also have `confidence == Verified`.
//! 4. Return any such entries as contradiction candidates.
//!
//! The detection is best-effort — a simple string-difference gate on top of
//! vector recall. This catches the common case (e.g. "use dark theme" vs
//! "prefer light theme"). A future version could use an LLM classifier.
//!
//! ## Modules
//!
//! - [`detector`] — `check_for_contradictions` engine.
//! - [`resolver`] — `resolve_contradiction` (Replace / Merge / Dismiss).
//! - [`schemas`] — JSON-RPC controllers under `dadou_contradiction`.
//!
//! ## Namespace
//!
//! Controllers are registered under the `dadou_contradiction` RPC namespace.
//! Events are published as `DomainEvent::ContradictionDetected` and
//! `DomainEvent::ContradictionResolved`.

pub mod detector;
pub mod resolver;
pub mod schemas;

pub use detector::{check_for_contradictions, ContradictionCandidate, ContradictionReport};
pub use resolver::{resolve_contradiction, ContradictionAction, ContradictionResolution};
