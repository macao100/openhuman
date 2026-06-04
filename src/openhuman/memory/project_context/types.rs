//! Types for the `dadou_project_context` namespace.
//!
//! A `ProjectFact` is a structured fact about a software project (name, version,
//! goals, architecture decisions, known issues, etc.) stored in the
//! `dadou_project_context` memory namespace. Facts survive restarts because
//! the underlying store is SQLite-backed.
//!
//! `ProjectScope` controls which facts are returned by the listing query.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One structured fact about a project.
///
/// A fact is uniquely identified within a project by its `fact_key`.
/// Only the most recent version of a given `(project_name, fact_key)` pair
/// is kept — `upsert_fact` overwrites any existing value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectFact {
    /// Project name, e.g. `"openhuman-backend"`, `"dadou"`.
    pub project_name: String,
    /// Unique key within the project, e.g. `"version"`, `"goal"`,
    /// `"architecture"`.
    pub fact_key: String,
    /// The fact value as a free-form string.
    pub fact_value: String,
    /// Category label for grouping, e.g. `"goal"`, `"architecture"`,
    /// `"decision"`, `"issue"`, `"version"`.
    pub category: String,
    /// How this fact was obtained, e.g. `"user"`, `"agent_analysis"`,
    /// `"readme_scan"`.
    pub source: String,
    /// When this fact was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Controls which projects are included in a listing query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectScope {
    /// Only return facts for currently active projects.
    Active,
    /// Only return facts for archived projects.
    Archived,
    /// Return facts for all projects regardless of status.
    All,
}

impl Default for ProjectScope {
    fn default() -> Self {
        Self::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: ProjectFact fields are accessible ──

    #[test]
    fn project_fact_holds_all_fields() {
        let fact = ProjectFact {
            project_name: "my-project".to_string(),
            fact_key: "version".to_string(),
            fact_value: "0.1.0".to_string(),
            category: "version".to_string(),
            source: "user".to_string(),
            updated_at: DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        assert_eq!(fact.project_name, "my-project");
        assert_eq!(fact.fact_key, "version");
        assert_eq!(fact.fact_value, "0.1.0");
        assert_eq!(fact.category, "version");
        assert_eq!(fact.source, "user");
    }

    // ── Test 2: ProjectFact round-trips through JSON ──

    #[test]
    fn project_fact_json_roundtrip() {
        let fact = ProjectFact {
            project_name: "openhuman".to_string(),
            fact_key: "goal".to_string(),
            fact_value: "Build an AI assistant".to_string(),
            category: "goal".to_string(),
            source: "user".to_string(),
            updated_at: DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        let json = serde_json::to_string(&fact).unwrap();
        let back: ProjectFact = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fact);
    }

    // ── Test 3: ProjectScope defaults to Active ──

    #[test]
    fn project_scope_defaults_to_active() {
        assert_eq!(ProjectScope::default(), ProjectScope::Active);
    }

    // ── Test 4: ProjectScope serializes to snake_case ──

    #[test]
    fn project_scope_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ProjectScope::Active).unwrap(),
            serde_json::json!("active")
        );
        assert_eq!(
            serde_json::to_value(ProjectScope::Archived).unwrap(),
            serde_json::json!("archived")
        );
        assert_eq!(
            serde_json::to_value(ProjectScope::All).unwrap(),
            serde_json::json!("all")
        );
    }
}
