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
//! ## Initialisation order
//!
//! The full Guardian pipeline (N1 + N2 + N3) is initialised at core startup:
//!
//! ```rust,ignore
//! // 1. Load config sections
//! let n2_cfg = N2EngineConfig::from(config.guardian_n2);
//! let n3_cfg = N3Config::from(config.guardian_n3);
//!
//! // 2. Create each pipeline stage
//! let n1 = GuardianN1::new(policy, yaml_path);
//! let n2 = GuardianN2::new(n2_cfg);
//! let n3 = GuardianN3::new(n3_cfg);
//!
//! // 3. Initialise the pipeline singleton
//! GuardianN1::init_global(n1);
//! // N2 and N3 are created per-evaluation from global config
//! ```
//!
//! Within the tool loop, use `GuardianPipeline` to evaluate all three stages:
//! N1 first (fast), then N2 if N1 allows, then N3 only if N2 escalates.
//! If N2 or N3 is disabled, the pipeline adapts:
//! - N2 disabled → skip heuristic analysis, pass through to N3 if configured.
//! - N3 disabled → fail-closed on N2 escalation (action is blocked).
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
pub use types::{GuardianPipelineResult, GuardianRule, N1Result, RuleAction, RuleContext, RuleResult};
pub use pipeline::{GuardianN1, GuardianPipeline};

pub use schemas::{
    all_controller_schemas as all_guardian_controller_schemas,
    all_registered_controllers as all_guardian_registered_controllers,
};
