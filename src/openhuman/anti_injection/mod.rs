//! Anti-Injection — Semantic output validation for DADOU skill results.
//!
//! This domain implements INJ-03: semantic validation of skill outputs before
//! they reach the LLM conversation context. It provides:
//!
//! - **Rule-based validation** (`validator::rules`): 16+ regex patterns
//!   detecting known prompt injection techniques (instruction overrides,
//!   role switching, system prompt manipulation, encoded payloads, etc.).
//! - **Optional LLM deep-check** (`validator::llm_check`): second-opinion
//!   LLM evaluation for ambiguous cases flagged by the rules engine.
//! - **SemanticOutputValidator facade** (`validator::mod`): unified entry
//!   point combining rule-based + optional LLM validation.
//! - **JSON-RPC controllers** (`schemas`): introspection and control over
//!   the validation configuration.
//!
//! ## Flow
//!
//! 1. Tool loop executes a skill → gets raw output.
//! 2. INJ-02 wraps output in `SkillOutputEnvelope` (structured JSON).
//! 3. **INJ-03** (`SemanticOutputValidator`) checks the envelope for
//!    injection patterns.
//! 4. If blocked → skill output is replaced with a policy-blocked message.
//!    The agent receives a clear error explaining why.
//! 5. If passed → INJ-01 `<external_data>` wrapping proceeds normally.

pub mod schemas;
pub mod validator;

pub use validator::{
    llm_check::{llm_deep_check, LlmVerdict, LlmVerdictKind},
    rules::{check_injection_patterns, InjectionFinding, InjectionRule},
    SemanticOutputValidator, ValidationMode, ValidationResult, ValidatorConfig,
};

// Re-export from schemas for core/all.rs wiring.
pub use schemas::{
    all_controller_schemas as all_anti_injection_controller_schemas,
    all_registered_controllers as all_anti_injection_registered_controllers,
};
