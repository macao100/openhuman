//! Guardian N3 — Lightweight LLM validator for ambiguous tool actions.
//!
//! N3 is the third and final layer of the Guardian pipeline. It is only
//! invoked when N2 (deterministic classifier) escalates an action as
//! uncertain (~2% of all actions). N3 uses the local LLM with a specialised
//! security validation prompt to determine whether the action is legitimate
//! or malicious.
//!
//! Target latency: **<500 ms** (configurable via [`N3Config::timeout_ms`]).
//!
//! ## Call flow
//!
//! 1. Cache check — if the exact `(tool_name, args, command)` tuple was
//!    already validated in this session, return the cached [`N3Result`].
//! 2. Prompt build — construct the system + user prompt from the action
//!    details and the N2 suspicion scores that caused the escalation.
//! 3. LLM call — call `local_ai_prompt()` with a short timeout
//!    ([`N3Config::timeout_ms`], default 450 ms).
//! 4. Parse — extract `{verdict, reason}` from the LLM's JSON response.
//!    On parse failure or timeout, return [`N3Verdict::Uncertain`]
//!    (fail-closed ⇒ action is blocked).
//! 5. Cache — store the result so the same action in the same session
//!    skips the LLM call.

mod cache;
mod prompt;
pub mod types;

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use tokio::time::{timeout, Duration};

use self::cache::LruCache;

pub use self::prompt::N3PromptBuilder;
pub use self::types::{N3Config, N3Result, N3Verdict};

/// The Guardian N3 validator.
///
/// Evaluates ambiguous tool actions by calling the local LLM with a
/// security validation prompt. Results are cached in an LRU cache to
/// avoid redundant LLM calls for the same action within a session.
pub struct GuardianN3 {
    config: N3Config,
    cache: Arc<Mutex<LruCache<N3Result>>>,
}

impl GuardianN3 {
    /// Create a new N3 validator with the given configuration.
    pub fn new(config: N3Config) -> Self {
        let cache = LruCache::new(config.cache_size);
        Self {
            config,
            cache: Arc::new(Mutex::new(cache)),
        }
    }

    /// Create a new N3 validator with default configuration values.
    pub fn with_defaults() -> Self {
        Self::new(N3Config::default())
    }

    /// Get a reference to the N3 configuration.
    pub fn config(&self) -> &N3Config {
        &self.config
    }

    /// Evaluate an ambiguous tool action via the local LLM.
    ///
    /// Steps:
    /// 1. Check LRU cache for the exact same `(tool_name, args, command)`.
    ///    If found (cache hit), return the cached result immediately.
    /// 2. Build the N3 system + user prompt from the action details and N2
    ///    suspicion scores.
    /// 3. Call the local LLM ([`LocalAiService::prompt_interactive`]) with
    ///    a configurable timeout.
    /// 4. Parse the LLM response as JSON and convert to [`N3Result`].
    ///    - On parse failure → [`N3Verdict::Uncertain`] (fail-closed).
    ///    - On timeout → [`N3Verdict::Uncertain`] (fail-closed).
    ///    - On LLM error → [`N3Verdict::Uncertain`] (fail-closed).
    /// 5. Cache the result and return.
    ///
    /// # Arguments
    ///
    /// * `tool_name` — Name of the tool being invoked (e.g. "file_write", "shell").
    /// * `tool_args` — JSON arguments passed to the tool.
    /// * `command` — Shell command string, if the tool is a shell executor.
    /// * `file_path` — File path being accessed, if the tool operates on files.
    /// * `n2_scores` — N2 suspicion scores that caused the escalation
    ///   (tuple of `(detector_name, score)`).
    pub async fn evaluate(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        command: Option<&str>,
        file_path: Option<&str>,
        n2_scores: &[(String, f64)],
    ) -> N3Result {
        let start = Instant::now();

        // Step 1: Build cache key and check cache.
        let key = N3PromptBuilder::cache_key(tool_name, tool_args, command);
        {
            let mut cache = self.cache.lock();
            if let Some(cached) = cache.get(&key) {
                let mut result = cached.clone();
                result.cached = true;
                return result;
            }
        }

        // Step 2: Build the full prompt (system + user context).
        let system_prompt = N3PromptBuilder::system_prompt();
        let user_prompt =
            N3PromptBuilder::build_user_prompt(tool_name, tool_args, command, file_path, n2_scores);
        let full_prompt = format!("{}\n\n{}", system_prompt, user_prompt);

        // Step 3: Call LLM with timeout.
        let llm_result = timeout(
            Duration::from_millis(self.config.timeout_ms),
            Self::call_llm(&full_prompt, self.config.max_tokens),
        )
        .await;

        let latency_us = start.elapsed().as_micros() as u64;

        // Step 4: Parse LLM response into N3Result.
        let mut n3_result = match llm_result {
            Ok(Ok(response)) => {
                log::info!(
                    "[guardian:n3] LLM response received ({} chars)",
                    response.len()
                );
                N3Result::from_llm_response(&response).unwrap_or_else(|| {
                    log::warn!(
                        "[guardian:n3] Failed to parse LLM response: {}",
                        &response[..response.len().min(200)]
                    );
                    N3Result {
                        verdict: N3Verdict::Uncertain,
                        reason: "Failed to parse LLM response".into(),
                        latency_us,
                        cached: false,
                        model_used: "local".into(),
                    }
                })
            }
            Ok(Err(e)) => {
                log::error!("[guardian:n3] LLM error: {}", e);
                N3Result {
                    verdict: N3Verdict::Uncertain,
                    reason: format!("LLM error: {}", e),
                    latency_us,
                    cached: false,
                    model_used: "local".into(),
                }
            }
            Err(_) => {
                log::warn!(
                    "[guardian:n3] LLM timed out after {}ms",
                    self.config.timeout_ms
                );
                N3Result {
                    verdict: N3Verdict::Uncertain,
                    reason: format!("N3 validation timed out (>{}ms)", self.config.timeout_ms),
                    latency_us,
                    cached: false,
                    model_used: "local".into(),
                }
            }
        };

        n3_result.latency_us = latency_us;

        // Step 5: Cache the result.
        {
            let mut cache = self.cache.lock();
            cache.insert(key, n3_result.clone());
        }

        log::debug!(
            "[guardian:n3] Evaluation complete — verdict={:?}, latency={}us",
            n3_result.verdict,
            latency_us
        );

        n3_result
    }

    /// Call the local LLM with the N3 prompt.
    ///
    /// Loads the config and uses the `prompt_interactive` method on the
    /// `LocalAiService` singleton. Returns the raw text response.
    async fn call_llm(prompt: &str, max_tokens: u32) -> Result<String, String> {
        let config = crate::openhuman::config::ops::load_config_with_timeout().await?;
        let service = crate::openhuman::inference::local::global(&config);
        service
            .prompt_interactive(&config, prompt, Some(max_tokens), true)
            .await
    }

    /// Returns `true` if N3 validation is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Reset the internal cache (useful for testing and config reload).
    pub fn reset_cache(&self) {
        let mut cache = self.cache.lock();
        *cache = LruCache::new(self.config.cache_size);
        log::debug!("[guardian:n3] Cache reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Constructor tests
    // -----------------------------------------------------------------------

    #[test]
    fn guardian_n3_creates_with_default_config() {
        let g = GuardianN3::with_defaults();
        assert!(g.config.enabled);
        assert_eq!(g.config.max_tokens, 256);
        assert_eq!(g.config.timeout_ms, 450);
    }

    #[test]
    fn guardian_n3_creates_with_custom_config() {
        let config = N3Config {
            enabled: false,
            max_tokens: 128,
            timeout_ms: 500,
            cache_size: 50,
            model_override: Some("llama3.2:3b".into()),
        };
        let g = GuardianN3::new(config.clone());
        assert!(!g.config.enabled);
        assert_eq!(g.config.max_tokens, 128);
        assert_eq!(g.config.model_override, Some("llama3.2:3b".into()));
    }

    #[test]
    fn guardian_n3_reset_cache_works() {
        let g = GuardianN3::with_defaults();
        // Insert something into the cache.
        {
            let mut cache = g.cache.lock();
            cache.insert(
                "test-key".into(),
                N3Result {
                    verdict: N3Verdict::Allow,
                    reason: "test".into(),
                    latency_us: 0,
                    cached: false,
                    model_used: "test".into(),
                },
            );
            assert!(cache.get("test-key").is_some());
        }
        // Reset cache.
        g.reset_cache();
        {
            let cache = g.cache.lock();
            assert!(cache.get("test-key").is_none());
        }
    }
}
