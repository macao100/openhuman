//! Semantic router operations — index building and query routing.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::openhuman::embeddings::EmbeddingProvider;
use crate::openhuman::memory_store::vectors::store::cosine_similarity;
use crate::openhuman::skills::store::SkillsStore;

use super::types::{RouteResult, SkillEmbedding, SkillMatch};

/// Global singleton — initialised once at startup.
static GLOBAL_ROUTER: OnceLock<Arc<SemanticRouter>> = OnceLock::new();

// ── Public API ────────────────────────────────────────────────────────────

/// Initialise the global semantic router.
///
/// `embedder` may be `None` — the router will fall back to keyword matching.
pub fn init_global(embedder: Arc<dyn EmbeddingProvider>, skills_store: &SkillsStore) {
    let router = SemanticRouter::new(embedder);
    if let Err(e) = router.build_index(skills_store) {
        log::warn!("[semantic_router] failed to build initial index: {e}");
    }
    GLOBAL_ROUTER.get_or_init(|| Arc::new(router));
    log::info!("[semantic_router] global router initialised");
}

/// Return the global router, if initialised.
pub fn global() -> Option<Arc<SemanticRouter>> {
    GLOBAL_ROUTER.get().cloned()
}

/// Rebuild the skill index from the current skills store.
///
/// Call this after skills are installed or removed.
pub fn rebuild_index(skills_store: &SkillsStore) -> Result<(), String> {
    let router = global().ok_or("semantic router not initialised")?;
    router.build_index(skills_store).map_err(|e| format!("{e}"))
}

// ── Router ────────────────────────────────────────────────────────────────

/// Searches installed skills by cosine similarity (or keyword fallback).
pub struct SemanticRouter {
    embedder: Arc<dyn EmbeddingProvider>,
    pub(crate) index: std::sync::RwLock<Vec<SkillEmbedding>>,
    pub(crate) has_embedder: bool,
}

impl SemanticRouter {
    pub fn new(embedder: Arc<dyn EmbeddingProvider>) -> Self {
        // Detect whether the embedder is real (not a noop).
        let has_embedder = embedder.name() != "none" && embedder.dimensions() > 0;
        Self {
            embedder,
            index: std::sync::RwLock::new(Vec::new()),
            has_embedder,
        }
    }

    /// Rebuild the skill embedding index from the skills store.
    pub fn build_index(&self, skills_store: &SkillsStore) -> anyhow::Result<()> {
        let skills = skills_store.installed();
        if skills.is_empty() {
            let mut idx = self
                .index
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
            idx.clear();
            return Ok(());
        }

        // Build embedding texts.
        let texts: Vec<String> = skills
            .iter()
            .map(|s| {
                format!(
                    "{}. {}. Tags: skill, tool",
                    s.name,
                    s.version // Use version as description placeholder
                )
            })
            .collect();

        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        // Compute embeddings (or fall back to empty vectors for keyword mode).
        let embeddings: Vec<Vec<f32>> = if self.has_embedder {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|_| anyhow::anyhow!("no tokio runtime"))?;
            rt.block_on(self.embedder.embed(&text_refs))?
        } else {
            // Keyword mode: no real embeddings needed.
            vec![vec![0.0_f32]; skills.len()]
        };

        let index: Vec<SkillEmbedding> = skills
            .iter()
            .zip(embeddings.into_iter())
            .map(|(s, emb)| SkillEmbedding {
                skill_name: s.name.clone(),
                description: s.version.clone(),
                embedding: emb,
            })
            .collect();

        let count = index.len();
        let mut idx = self
            .index
            .write()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        *idx = index;

        log::info!("[semantic_router] index built with {count} skills");
        Ok(())
    }

    /// Route a user query to the top-k matching skills.
    pub fn route_query(&self, query: &str, top_k: usize) -> RouteResult {
        let start = Instant::now();

        let index = match self.index.read() {
            Ok(idx) => idx.clone(),
            Err(_) => {
                return RouteResult {
                    matches: Vec::new(),
                    elapsed_us: start.elapsed().as_micros() as u64,
                    method: "error".to_string(),
                };
            }
        };

        if index.is_empty() {
            return RouteResult {
                matches: Vec::new(),
                elapsed_us: start.elapsed().as_micros() as u64,
                method: "empty_index".to_string(),
            };
        }

        let method: String;
        let mut scored: Vec<SkillMatch>;

        if self.has_embedder {
            // Embedding-based: embed the query, compare via cosine similarity.
            let query_embedding = {
                let rt = match tokio::runtime::Handle::try_current() {
                    Ok(rt) => rt,
                    Err(_) => {
                        return RouteResult {
                            matches: Vec::new(),
                            elapsed_us: start.elapsed().as_micros() as u64,
                            method: "no_runtime".to_string(),
                        };
                    }
                };
                match rt.block_on(self.embedder.embed_one(query)) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!(
                            "[semantic_router] embed_one failed: {e} — falling back to keyword"
                        );
                        return self.route_keyword(query, top_k, &index, &start);
                    }
                }
            };

            scored = index
                .iter()
                .map(|se| {
                    let score = cosine_similarity(&query_embedding, &se.embedding);
                    SkillMatch {
                        skill_name: se.skill_name.clone(),
                        score: score as f64,
                        description: se.description.clone(),
                    }
                })
                .collect();

            method = "cosine_embedding".to_string();
        } else {
            return self.route_keyword(query, top_k, &index, &start);
        }

        // Sort descending by score.
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        RouteResult {
            matches: scored,
            elapsed_us: start.elapsed().as_micros() as u64,
            method,
        }
    }

    /// Fallback: Jaccard token-overlap scoring.
    fn route_keyword(
        &self,
        query: &str,
        top_k: usize,
        index: &[SkillEmbedding],
        start: &Instant,
    ) -> RouteResult {
        let query_tokens = tokenize(query);

        let mut scored: Vec<SkillMatch> = index
            .iter()
            .map(|se| {
                let skill_text = format!("{} {}", se.skill_name, se.description);
                let skill_tokens = tokenize(&skill_text);

                let intersection = query_tokens
                    .iter()
                    .filter(|t| skill_tokens.contains(t))
                    .count() as f64;
                let union = (query_tokens.len() + skill_tokens.len()) as f64 - intersection;
                let score = if union > 0.0 {
                    intersection / union
                } else {
                    0.0
                };

                SkillMatch {
                    skill_name: se.skill_name.clone(),
                    score,
                    description: se.description.clone(),
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        RouteResult {
            matches: scored,
            elapsed_us: start.elapsed().as_micros() as u64,
            method: "jaccard_keyword".to_string(),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Simple whitespace-and-punctuation tokenizer, lowercased.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| {
            c.is_whitespace() || c == ',' || c == '.' || c == ':' || c == ';' || c == '-'
        })
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|t| !t.is_empty() && t.len() > 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::skills::store::InstalledSkill;

    struct NoopEmbedder;

    #[async_trait::async_trait]
    impl EmbeddingProvider for NoopEmbedder {
        fn name(&self) -> &'static str {
            "none"
        }
        fn model_id(&self) -> &str {
            ""
        }
        fn dimensions(&self) -> usize {
            0
        }
        fn signature(&self) -> u64 {
            0
        }
        async fn embed(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }
        async fn embed_one(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![])
        }
    }

    fn test_store(skills: &[(&str, &str)]) -> SkillsStore {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.toml");
        let mut store = SkillsStore::new(path).unwrap();
        for (name, version) in skills {
            store.upsert(InstalledSkill {
                name: name.to_string(),
                version: version.to_string(),
                commit_hash: "abc123".to_string(),
                enabled: true,
                gpg_fingerprint: None,
                installed_at: "2026-01-01T00:00:00Z".to_string(),
                last_audit_at: None,
                audit_result: None,
            });
        }
        store
    }

    #[test]
    fn route_keyword_returns_matches() {
        let store = test_store(&[
            ("git-helper", "1.0.0"),
            ("python-runner", "2.0.0"),
            ("web-scraper", "0.5.0"),
        ]);
        let embedder = Arc::new(NoopEmbedder);
        let router = SemanticRouter::new(embedder);
        router.build_index(&store).unwrap();

        let result = router.route_query("python", 3);
        assert_eq!(result.method, "jaccard_keyword");
        assert!(!result.matches.is_empty(), "should match python-runner");
        // python-runner should score highest since "python" appears in its name
        assert_eq!(result.matches[0].skill_name, "python-runner");
    }

    #[test]
    fn empty_index_returns_empty_matches() {
        let embedder = Arc::new(NoopEmbedder);
        let router = SemanticRouter::new(embedder);

        let result = router.route_query("anything", 3);
        assert_eq!(result.method, "empty_index");
        assert!(result.matches.is_empty());
    }

    #[test]
    fn tokenize_splits_and_lowercases() {
        let tokens = tokenize("Hello, World: Python-Runner");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"python".to_string()));
        assert!(tokens.contains(&"runner".to_string()));
    }

    #[test]
    fn tokenize_filters_short_tokens() {
        let tokens = tokenize("a b c ab bc cd");
        assert!(!tokens.contains(&"a".to_string()));
        assert!(!tokens.contains(&"b".to_string()));
        assert!(!tokens.contains(&"c".to_string()));
        assert!(tokens.contains(&"ab".to_string()));
        assert!(tokens.contains(&"cd".to_string()));
    }
}
