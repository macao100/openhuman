//! Dashboard configuration.
//!
//! Controls the local observability dashboard served on a dedicated port.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DashboardConfig {
    /// Whether the dashboard server starts alongside the main RPC server.
    /// Defaults to `true`. Set to `false` to disable.
    #[serde(default = "default_dashboard_enabled")]
    pub enabled: bool,

    /// Listen port for the dashboard HTTP server.
    /// Defaults to `7790`.
    #[serde(default = "default_dashboard_port")]
    pub port: u16,

    /// Listen address for the dashboard HTTP server.
    /// Always localhost-only for security; no remote access.
    #[serde(default = "default_dashboard_host")]
    pub host: String,

    /// Number of days to retain dashboard events before pruning.
    /// Events older than this are deleted during periodic cleanup.
    /// Defaults to `7`.
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,

    /// Maximum number of stored events before the oldest are pruned.
    /// Prevents unbounded growth of the dashboard database.
    /// Defaults to `10_000`.
    #[serde(default = "default_max_events")]
    pub max_events: u64,
}

fn default_dashboard_enabled() -> bool {
    true
}

fn default_dashboard_port() -> u16 {
    7790
}

fn default_dashboard_host() -> String {
    "127.0.0.1".to_string()
}

fn default_retention_days() -> u64 {
    7
}

fn default_max_events() -> u64 {
    10_000
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 7790,
            host: "127.0.0.1".to_string(),
            retention_days: 7,
            max_events: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_enables_dashboard() {
        let cfg = DashboardConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.port, 7790);
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.retention_days, 7);
        assert_eq!(cfg.max_events, 10_000);
    }

    #[test]
    fn deserialize_missing_fields_uses_defaults() {
        let cfg: DashboardConfig = serde_json::from_value(json!({})).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.port, 7790);
    }

    #[test]
    fn deserialize_respects_explicit_fields() {
        let cfg: DashboardConfig = serde_json::from_value(json!({
            "enabled": false,
            "port": 9999,
            "host": "0.0.0.0",
            "retention_days": 30,
            "max_events": 5000
        }))
        .unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.port, 9999);
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.retention_days, 30);
        assert_eq!(cfg.max_events, 5000);
    }
}
