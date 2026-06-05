//! Guardian — Deterministic rule engine (N1) + LLM validator (N3) for
//! tool-call interception.
//!
//! ## Pipeline stages
//!
//! | Stage | Layer | Purpose | Latency |
//! |-------|-------|---------|---------|
//! | N1 | Deterministic rules | Block known-bad patterns (regex, paths, commands) | <1 ms |
//! | N2 | Heuristic classifiers | Score actions for exfiltration, entropy, hidden payloads | <10 ms |
//! | N3 | LLM validator | Escalation-only LLM call for ambiguous cases (~2%) | <500 ms |
//!
//! The Guardian N1 domain formalises the first stage of the defence-in-depth
//! pipeline: **classify -> gate -> validate**. It wraps OpenHuman's existing
//! [`SecurityPolicy`](crate::openhuman::security::SecurityPolicy) with an
//! extensible rule engine that combines:
//!
//! - **Compile-time Rust rules** (path whitelists, regex patterns, command
//!   blocklists) that are always enforced (fail-closed, D-04).
//! - **Additive YAML rules** loaded from `~/.dadou/guardian-rules.yaml` that
//!   can only add restrictions, never override Rust rules (D-05).
//!
//! The pipeline measures latency in microseconds and targets <1ms per
//! evaluation.

// ── Sub-modules ────────────────────────────────────────────────────────

mod types;
mod rules;
mod pipeline;
pub mod bus;
pub mod ops;
pub mod schemas;
pub mod n2;
pub mod n3;

pub use rules::{compile_ruleset, default_rust_rules, load_yaml_rules, BlocklistRule, PathWhitelistRule, RegexPatternRule, RuleSet};
pub use types::{GuardianRule, N1Result, RuleAction, RuleContext, RuleResult};
pub use pipeline::GuardianN1;

pub use schemas::{
    all_controller_schemas as all_guardian_controller_schemas,
    all_registered_controllers as all_guardian_registered_controllers,
};
