//! DADOU project context — structured facts about the user's projects.
//!
//! This domain stores per-project facts (name, version, goals, architecture
//! decisions, known issues) in the `dadou_project_context` memory namespace,
//! and injects them into the agent's system prompt at session start so DADOU
//! always has the full project picture — not just the current file/directory.
//!
//! ## Modules
//!
//! - [`types`] — `ProjectFact` struct and `ProjectScope` enum.
//! - [`store`] — CRUD operations via the `Memory` trait.
//! - [`ops`] — Business logic: load and format context for prompt injection.
//! - [`schemas`] — Controller schemas for JSON-RPC / CLI.
//!
//! ## Namespace
//!
//! All facts are stored under the `dadou_project_context` namespace in the
//! existing SQLite-backed memory store, so they survive restarts.

pub mod ops;
pub mod schemas;
pub mod store;
pub mod types;
