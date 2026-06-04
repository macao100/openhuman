//! Guardian N1 -- Deterministic rule engine for tool-call interception.
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

pub use rules::{compile_ruleset, default_rust_rules, load_yaml_rules, BlocklistRule, PathWhitelistRule, RegexPatternRule, RuleSet};
pub use types::{GuardianRule, N1Result, RuleAction, RuleContext, RuleResult};
pub use pipeline::GuardianN1;

pub use schemas::{
    all_controller_schemas as all_guardian_controller_schemas,
    all_registered_controllers as all_guardian_registered_controllers,
};
