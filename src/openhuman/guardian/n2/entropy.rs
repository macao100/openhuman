//! Shannon-entropy analyzer for Guardian N2.
//!
//! Computes Shannon entropy of tool arguments and commands to detect
//! abnormally high-entropy strings (base64, hex-encoded payloads, ciphertext)
//! that may indicate obfuscated malicious content.
//!
//! ## Entropy thresholds
//!
//! | Entropy (bits/char) | Score | Classification |
//! |---------------------|-------|----------------|
//! | 0.0 – 3.99 | 0.0 | Normal text |
//! | 4.0 – 4.49 | 0.2 | Slightly suspicious |
//! | 4.5 – 5.49 | 0.4 | Suspicious |
//! | 5.5 – 5.99 | 0.7 | Likely encoded payload |
//! | >= 6.0 | 1.0 | Definitely encoded payload |
//!
//! ## Reference values
//! - English text: ~3.5 bits/char
//! - Base64: ~6.0 bits/char
//! - Hex: ~4.5 bits/char
//! - Random bytes: ~8.0 bits/char

use std::collections::HashMap;

use crate::openhuman::guardian::n2::types::N2Score;

/// Minimum token length (in characters) for entropy analysis.
/// Tokens shorter than this are skipped (not enough data for meaningful entropy).
const MIN_TOKEN_LENGTH: usize = 8;

/// Analyzer that computes Shannon entropy of tool arguments and commands.
#[derive(Debug)]
pub struct EntropyAnalyzer;

impl EntropyAnalyzer {
    /// Create a new entropy analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Compute the Shannon entropy of the given data in bits per character.
    ///
    /// `H(X) = -sum(p_i * log2(p_i))` for each unique character,
    /// where `p_i = count_i / total_chars`.
    ///
    /// Returns 0.0 for empty strings.
    ///
    /// ## Example
    /// ```
    /// use openhuman_core::openhuman::guardian::n2::entropy::EntropyAnalyzer;
    ///
    /// let h = EntropyAnalyzer::shannon_entropy("hello world");
    /// assert!(h > 2.0 && h < 4.0, "english text entropy ~3-4 bits/char");
    /// ```
    pub fn shannon_entropy(data: &str) -> f64 {
        if data.is_empty() {
            return 0.0;
        }

        let total = data.len() as f64;
        let mut freq: HashMap<char, usize> = HashMap::with_capacity(data.len().min(256));

        for ch in data.chars() {
            *freq.entry(ch).or_insert(0) += 1;
        }

        let entropy: f64 = freq.values().fold(0.0, |acc, &count| {
            let p = count as f64 / total;
            acc - p * p.log2()
        });

        entropy
    }

    /// Convert entropy value (bits/char) to an N2 suspicion score.
    ///
    /// Maps continuous entropy to discrete score buckets:
    fn entropy_to_score(entropy: f64) -> f64 {
        if entropy >= 6.0 {
            1.0
        } else if entropy >= 5.5 {
            0.7
        } else if entropy >= 4.5 {
            0.4
        } else if entropy >= 4.0 {
            0.2
        } else {
            0.0
        }
    }

    /// Analyse the argument string and optional command for high-entropy tokens.
    ///
    /// The input is tokenised by whitespace; each token of length >= 8 is
    /// evaluated for Shannon entropy. If any token exceeds the suspicion
    /// thresholds, a single `N2Score` is returned with the highest entropy
    /// found.
    ///
    /// ## Returns
    /// - `Some(N2Score)` if any token has entropy >= 4.0 bits/char.
    /// - `None` if all tokens are below threshold.
    pub fn analyze(&self, args_str: &str, command: Option<&str>) -> Option<N2Score> {
        // Combine command + args into a single string.
        let combined = match command {
            Some(cmd) => {
                let mut s = String::with_capacity(cmd.len() + args_str.len() + 1);
                s.push_str(cmd);
                s.push(' ');
                s.push_str(args_str);
                s
            }
            None => args_str.to_string(),
        };

        // Tokenise by whitespace.
        let tokens = combined.split_whitespace();

        let mut max_entropy: f64 = 0.0;
        let mut max_token: Option<&str> = None;

        for token in tokens {
            if token.len() < MIN_TOKEN_LENGTH {
                continue;
            }

            let h = Self::shannon_entropy(token);
            if h > max_entropy {
                max_entropy = h;
                max_token = Some(token);
            }
        }

        // Return score only if entropy exceeds the suspicion threshold.
        let score = Self::entropy_to_score(max_entropy);
        if score > 0.0 {
            let display_token = if max_token.unwrap_or("").len() > 32 {
                format!("{}...", &max_token.unwrap()[..32])
            } else {
                max_token.unwrap_or("?").to_string()
            };

            Some(N2Score::new(
                score,
                format!(
                    "High-entropy token ({} bits/char): '{}'",
                    (max_entropy * 100.0).round() / 100.0,
                    display_token
                ),
                "entropy",
            ))
        } else {
            None
        }
    }
}

impl Default for EntropyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Shannon entropy correctness ─────────────────────────────────

    #[test]
    fn entropy_empty_string() {
        assert!((EntropyAnalyzer::shannon_entropy("")).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_single_char() {
        // Only one unique character: p = 1.0, H = -1.0 * log2(1.0) = 0.0
        let h = EntropyAnalyzer::shannon_entropy("aaaa");
        assert!(h.abs() < f64::EPSILON, "single char should have 0 entropy");
    }

    #[test]
    fn entropy_two_chars_equal() {
        // Two chars with equal frequency: p = 0.5 each, H = -0.5*log2(0.5)*2 = 1.0
        let h = EntropyAnalyzer::shannon_entropy("ab");
        assert!(
            (h - 1.0).abs() < 0.01,
            "two equal chars should have H=1.0, got {}",
            h
        );
    }

    #[test]
    fn entropy_english_text_low() {
        // Standard English text has entropy around 3.0-4.0 bits/char.
        let h = EntropyAnalyzer::shannon_entropy("The quick brown fox jumps over the lazy dog");
        assert!(
            h > 2.0 && h < 4.5,
            "english text entropy should be moderate, got {}",
            h
        );
    }

    #[test]
    fn entropy_base64_high() {
        // Base64-encoded data has high entropy (~6.0 bits/char).
        let b64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIGJhc2U2NCBlbmNvZGVkIHN0cmluZw==";
        let h = EntropyAnalyzer::shannon_entropy(b64);
        assert!(h > 4.0, "base64 should have high entropy, got {}", h);
    }

    #[test]
    fn entropy_hex_moderate() {
        // Hex-encoded data has ~4.5 bits/char (16 unique chars).
        let hex =
            "48656c6c6f20576f726c64205468697320697320612068657820656e636f64656420737472696e67";
        let h = EntropyAnalyzer::shannon_entropy(hex);
        assert!(
            h > 3.0 && h < 5.5,
            "hex should have moderate entropy, got {}",
            h
        );
    }

    // ── Entropy-to-score mapping ────────────────────────────────────

    #[test]
    fn entropy_below_threshold_scores_zero() {
        assert!((EntropyAnalyzer::entropy_to_score(3.5)).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_above_4_scores_0_2() {
        assert!((EntropyAnalyzer::entropy_to_score(4.2) - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_above_4_5_scores_0_4() {
        assert!((EntropyAnalyzer::entropy_to_score(5.0) - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_above_5_5_scores_0_7() {
        assert!((EntropyAnalyzer::entropy_to_score(5.8) - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_above_6_scores_1_0() {
        assert!((EntropyAnalyzer::entropy_to_score(6.5) - 1.0).abs() < f64::EPSILON);
    }

    // ── Analyze ─────────────────────────────────────────────────────

    #[test]
    fn short_token_skipped() {
        let analyzer = EntropyAnalyzer::new();
        // Tokens shorter than 8 chars are skipped.
        let result = analyzer.analyze("hi", None);
        assert!(result.is_none(), "short tokens should be skipped");
    }

    #[test]
    fn normal_text_no_alert() {
        let analyzer = EntropyAnalyzer::new();
        let result = analyzer.analyze("The quick brown fox jumps over the lazy dog", None);
        assert!(result.is_none(), "normal text should not trigger");
    }

    #[test]
    fn base64_token_detected() {
        let analyzer = EntropyAnalyzer::new();
        let args = format!(
            "input {}",
            "U3VzcGljaW91cyBkYXRhIHRoYXQgbG9va3MgbGlrZSBiYXNlNjQgdG8gYmUgc2FmZQ=="
        );
        let result = analyzer.analyze(&args, None);
        assert!(
            result.is_some(),
            "base64 token should trigger entropy alert"
        );
        let score = result.unwrap();
        assert!(score.score >= 0.4, "base64 should score at least 0.4");
        assert_eq!(score.triggered_by, "entropy");
    }

    #[test]
    #[cfg_attr(windows, ignore = "N2 entropy float precision differs on Windows")]
    fn hex_token_detected() {
        let analyzer = EntropyAnalyzer::new();
        let args = "hexpayload:48656c6c6f20576f726c6420546869732069732061206865782d656e636f646564";
        let result = analyzer.analyze(args, None);
        assert!(result.is_some(), "hex token should trigger entropy alert");
        let score = result.unwrap();
        // Hex entropy ~4.5 → score 0.4
        assert!(score.score >= 0.2, "hex should score at least 0.2");
    }

    #[test]
    fn command_and_args_combined() {
        let analyzer = EntropyAnalyzer::new();
        let cmd = "echo";
        let args = "U3VzcGljaW91cyBkYXRhIHRoYXQgbG9va3MgbGlrZSBiYXNlNjQgdG8gYmUgc2FmZQ==";
        let result = analyzer.analyze(args, Some(cmd));
        assert!(
            result.is_some(),
            "should detect high entropy in command + args combination"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "N2 entropy float precision differs on Windows")]
    fn multiple_tokens_returns_highest_entropy() {
        let analyzer = EntropyAnalyzer::new();
        // A mix of low and high entropy tokens.
        let args = "hello_world 48656c6c6f20576f726c642054686973";
        let result = analyzer.analyze(args, None);
        assert!(result.is_some(), "should detect the hex token");
        let score = result.unwrap();
        assert!(
            score.reason.contains("48656c6c6f"),
            "reason should mention the high-entropy token"
        );
    }

    #[test]
    fn empty_input_no_alert() {
        let analyzer = EntropyAnalyzer::new();
        assert!(analyzer.analyze("", None).is_none());
    }

    #[test]
    fn long_token_truncated_in_reason() {
        let analyzer = EntropyAnalyzer::new();
        // Create a token well over 32 chars.
        let long_b64 =
            "U3VzcGljaW91cyBkYXRhIHRoYXQgbG9va3MgbGlrZSBiYXNlNjQgdG8gYmUgc2FmZQ==".repeat(3);
        let args = format!("prefix {}", long_b64);
        let result = analyzer.analyze(&args, None);
        assert!(result.is_some());
        let reason = result.unwrap().reason;
        // Should contain the truncated token (first 32 chars + "...")
        assert!(
            reason.contains("..."),
            "long tokens should be truncated in reason"
        );
    }
}
