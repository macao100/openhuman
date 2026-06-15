//! Dashboard HTTP server.
//!
//! Starts a lightweight Axum HTTP server on a dedicated port (default `7790`)
//! serving the self-contained dashboard HTML frontend and API endpoints.

use std::net::SocketAddr;
use std::pin::Pin;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::core::event_bus::DomainEvent;
use crate::openhuman::config::Config;

use super::store;
use super::types::SkillSummary;

// ── Embedded frontend ─────────────────────────────────────────────────────

/// The dashboard HTML is compiled into the binary at build time.
const DASHBOARD_HTML: &str = include_str!("web/index.html");

// ── Router construction ───────────────────────────────────────────────────

/// Build the Axum router for the dashboard server.
pub fn build_dashboard_router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/recent", get(recent_handler))
        .route("/api/skills", get(skills_handler))
        .route("/api/memory", get(memory_handler))
        .route("/api/routing", get(routing_handler))
        .route("/api/routing/search", get(routing_search_handler))
        .route("/dashboard/events", get(sse_handler))
}

// ── Handlers ──────────────────────────────────────────────────────────────

/// Serve the self-contained dashboard HTML page.
async fn root_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        DASHBOARD_HTML,
    )
}

/// JSON: aggregate dashboard statistics.
async fn stats_handler() -> impl IntoResponse {
    let stats = if let Some(store) = store::global() {
        if let Ok(store) = store.lock() {
            match store.get_stats() {
                Ok(mut stats) => {
                    // Augment with active skill count.
                    if let Ok(skills_store) = crate::openhuman::skills::store::SkillsStore::load() {
                        stats.active_skill_count = skills_store
                            .installed()
                            .iter()
                            .filter(|s| s.enabled)
                            .count() as u64;
                    }
                    serde_json::to_value(&stats).unwrap_or_default()
                }
                Err(e) => {
                    log::warn!("[dashboard] stats error: {e}");
                    serde_json::json!({"error": "failed to read stats"})
                }
            }
        } else {
            serde_json::json!({"error": "store lock failed"})
        }
    } else {
        serde_json::json!({"error": "store not initialised"})
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        stats.to_string(),
    )
}

/// Query parameters for `/api/recent`.
#[derive(Deserialize)]
struct RecentQuery {
    limit: Option<u64>,
    kind: Option<String>,
}

/// JSON: recent dashboard events.
async fn recent_handler(Query(query): Query<RecentQuery>) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(50).min(500);
    let events = if let Some(store) = store::global() {
        if let Ok(store) = store.lock() {
            match store.list_recent(limit, query.kind.as_deref()) {
                Ok(events) => serde_json::to_value(&events).unwrap_or_default(),
                Err(e) => {
                    log::warn!("[dashboard] recent error: {e}");
                    serde_json::json!({"error": "failed to list events"})
                }
            }
        } else {
            serde_json::json!({"error": "store lock failed"})
        }
    } else {
        serde_json::json!({"error": "store not initialised"})
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        events.to_string(),
    )
}

/// JSON: installed skill summaries.
async fn skills_handler() -> impl IntoResponse {
    let skills = match crate::openhuman::skills::store::SkillsStore::load() {
        Ok(skills_store) => {
            let list: Vec<SkillSummary> = skills_store
                .installed()
                .iter()
                .map(|s| SkillSummary {
                    name: s.name.clone(),
                    version: s.version.clone(),
                    enabled: s.enabled,
                    gpg_verified: s.gpg_fingerprint.is_some(),
                    description: None,
                })
                .collect();
            serde_json::to_value(&list).unwrap_or_default()
        }
        Err(e) => {
            log::warn!("[dashboard] skills error: {e}");
            serde_json::json!({"error": "failed to load skills"})
        }
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        skills.to_string(),
    )
}

/// JSON: memory event statistics.
async fn memory_handler() -> impl IntoResponse {
    let result = if let Some(store) = store::global() {
        if let Ok(store) = store.lock() {
            match store.list_recent(10_000, None) {
                Ok(events) => {
                    let stored = events.iter().filter(|e| e.kind == "memory_stored").count();
                    let recalled = events
                        .iter()
                        .filter(|e| e.kind == "memory_recalled")
                        .count();
                    serde_json::json!({
                        "total_memory_events": stored + recalled,
                        "stored": stored,
                        "recalled": recalled,
                    })
                }
                Err(e) => serde_json::json!({"error": format!("{e}")}),
            }
        } else {
            serde_json::json!({"error": "store lock failed"})
        }
    } else {
        serde_json::json!({"error": "store not initialised"})
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        result.to_string(),
    )
}

/// JSON: semantic router index status.
async fn routing_handler() -> impl IntoResponse {
    let result = if let Some(router) = crate::openhuman::semantic_router::ops::global() {
        if let Ok(index) = router.index.read() {
            serde_json::json!({
                "skill_count": index.len(),
                "has_embedder": router.has_embedder,
            })
        } else {
            serde_json::json!({"error": "lock failed"})
        }
    } else {
        serde_json::json!({"error": "router not initialised"})
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        result.to_string(),
    )
}

/// Query parameters for `/api/routing/search`.
#[derive(Deserialize)]
struct RoutingSearchQuery {
    q: String,
    top_k: Option<usize>,
}

/// JSON: search skills matching a query.
async fn routing_search_handler(Query(query): Query<RoutingSearchQuery>) -> impl IntoResponse {
    let top_k = query.top_k.unwrap_or(3).min(10);
    let result = if let Some(router) = crate::openhuman::semantic_router::ops::global() {
        let route_result = router.route_query(&query.q, top_k);
        serde_json::to_value(&route_result).unwrap_or_default()
    } else {
        serde_json::json!({"error": "router not initialised", "matches": []})
    };

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        result.to_string(),
    )
}

/// SSE: stream dashboard events to connected clients.
async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Grab the raw broadcast receiver from the event bus.
    // The EventBus provides a method for external consumers.
    // We filter to dashboard-relevant domains.
    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        if let Some(bus) = crate::core::event_bus::global() {
            let rx = bus.raw_receiver();
            Box::pin(BroadcastStream::new(rx).filter_map(|result| {
                let event = match result {
                    Ok(e) => e,
                    Err(_) => return None,
                };

                // Only forward events from dashboard-relevant domains.
                let domain = event.domain();
                let relevant = matches!(
                    domain,
                    "guardian" | "tool" | "agent" | "skill" | "memory" | "channel" | "system"
                );
                if !relevant {
                    return None;
                }

                // Serialise to a compact JSON payload.
                let payload = serde_json::json!({
                    "kind": event_name(&event),
                    "domain": domain,
                    "payload": event_payload(event),
                    "recorded_at": chrono::Utc::now().to_rfc3339(),
                });

                let data = payload.to_string();
                Some(Ok(Event::default().event("dashboard_event").data(data)))
            }))
        } else {
            // No event bus — return an empty stream.
            let (_, rx) = broadcast::channel::<DomainEvent>(1);
            Box::pin(BroadcastStream::new(rx).filter_map(|_| None::<Result<Event, Infallible>>))
        };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

// ── SSE helpers ───────────────────────────────────────────────────────────

fn event_name(event: &DomainEvent) -> &'static str {
    match event {
        DomainEvent::GuardianBlocked { .. } => "guardian_blocked",
        DomainEvent::N2Blocked { .. } => "n2_blocked",
        DomainEvent::N2Escalated { .. } => "n2_escalated",
        DomainEvent::N3Result { .. } => "n3_result",
        DomainEvent::PlanValidated { .. } => "plan_validated",
        DomainEvent::InjectionBlocked { .. } => "injection_blocked",
        DomainEvent::ToolExecutionStarted { .. } => "tool_started",
        DomainEvent::ToolExecutionCompleted { .. } => "tool_completed",
        DomainEvent::AgentTurnStarted { .. } => "agent_turn",
        DomainEvent::AgentTurnCompleted { .. } => "agent_turn",
        DomainEvent::SkillExecuted { .. } => "skill_executed",
        DomainEvent::MemoryStored { .. } => "memory_stored",
        DomainEvent::MemoryRecalled { .. } => "memory_recalled",
        DomainEvent::ChannelConnected { .. } => "channel_connected",
        DomainEvent::ChannelDisconnected { .. } => "channel_disconnected",
        DomainEvent::SystemStartup { .. } => "system_startup",
        DomainEvent::SystemShutdown { .. } => "system_shutdown",
        _ => "unknown",
    }
}

fn event_payload(event: DomainEvent) -> serde_json::Value {
    match event {
        DomainEvent::GuardianBlocked {
            tool_name,
            reason,
            latency_us,
        } => serde_json::json!({
            "tool_name": tool_name,
            "reason": reason,
            "latency_us": latency_us,
        }),
        DomainEvent::N2Blocked {
            tool_name,
            reason,
            scores_json,
            latency_us,
        } => serde_json::json!({
            "tool_name": tool_name,
            "reason": reason,
            "scores_json": scores_json,
            "latency_us": latency_us,
        }),
        DomainEvent::N2Escalated {
            tool_name,
            scores_json,
            latency_us,
        } => serde_json::json!({
            "tool_name": tool_name,
            "scores_json": scores_json,
            "latency_us": latency_us,
        }),
        DomainEvent::N3Result {
            tool_name,
            verdict,
            reason,
            latency_us,
        } => serde_json::json!({
            "tool_name": tool_name,
            "verdict": verdict,
            "reason": reason,
            "latency_us": latency_us,
        }),
        DomainEvent::ToolExecutionStarted {
            tool_name,
            session_id,
        } => serde_json::json!({
            "tool_name": tool_name,
            "session_id": session_id,
        }),
        DomainEvent::ToolExecutionCompleted {
            tool_name,
            session_id,
            success,
            elapsed_ms,
        } => serde_json::json!({
            "tool_name": tool_name,
            "session_id": session_id,
            "success": success,
            "elapsed_ms": elapsed_ms,
        }),
        DomainEvent::AgentTurnStarted {
            session_id,
            channel,
        } => serde_json::json!({
            "session_id": session_id,
            "channel": channel,
        }),
        DomainEvent::AgentTurnCompleted {
            session_id,
            text_chars,
            iterations,
        } => serde_json::json!({
            "session_id": session_id,
            "text_chars": text_chars,
            "iterations": iterations,
        }),
        DomainEvent::SkillExecuted {
            skill_id,
            tool_name,
            arguments: _,
            result: _,
            success,
            elapsed_ms,
        } => serde_json::json!({
            "skill_id": skill_id,
            "tool_name": tool_name,
            "success": success,
            "elapsed_ms": elapsed_ms,
        }),
        DomainEvent::MemoryStored {
            key,
            category,
            namespace,
        } => serde_json::json!({
            "key": key,
            "category": category,
            "namespace": namespace,
        }),
        DomainEvent::MemoryRecalled { query, hit_count } => serde_json::json!({
            "query": query,
            "hit_count": hit_count,
        }),
        DomainEvent::ChannelConnected { channel } => serde_json::json!({
            "channel": channel,
        }),
        DomainEvent::ChannelDisconnected { channel, reason } => serde_json::json!({
            "channel": channel,
            "reason": reason,
        }),
        DomainEvent::SystemStartup { component } => serde_json::json!({
            "component": component,
        }),
        DomainEvent::SystemShutdown { component } => serde_json::json!({
            "component": component,
        }),
        _ => serde_json::json!({}),
    }
}

// ── Server lifecycle ──────────────────────────────────────────────────────

/// Start the dashboard server on the configured host and port.
///
/// Returns the bound address on success (after the first successful bind
/// attempt). If the preferred port is occupied, it tries the next 4 ports.
pub async fn start_dashboard_server(
    config: &Config,
    shutdown_token: tokio_util::sync::CancellationToken,
) -> Result<SocketAddr, anyhow::Error> {
    let host = &config.dashboard.host;
    let base_port = config.dashboard.port;

    let router = build_dashboard_router();
    let mut last_error = None;

    for offset in 0..5u16 {
        let port = base_port + offset;
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid dashboard address {host}:{port}: {e}"))?;

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                log::warn!("[dashboard] port {port} unavailable: {e} — trying next");
                last_error = Some(e);
                continue;
            }
        };

        let actual_addr = listener.local_addr()?;
        log::info!("[dashboard] server listening on http://{actual_addr}");

        // Spawn periodic pruning: every hour, remove events older than retention_days.
        let retention_days = config.dashboard.retention_days;
        let max_events = config.dashboard.max_events;
        let prune_shutdown = shutdown_token.clone();
        tokio::spawn(async move {
            loop {
                if prune_shutdown.is_cancelled() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                if prune_shutdown.is_cancelled() {
                    break;
                }
                if let Some(store) = store::global() {
                    if let Ok(store) = store.lock() {
                        let _ = store.prune_older_than(retention_days);
                        let _ = store.enforce_max_events(max_events);
                    }
                }
            }
        });

        let app = router.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_token.cancelled().await;
                log::info!("[dashboard] shutdown complete");
            })
            .await
            .map_err(|e| anyhow::anyhow!("dashboard server error: {e}"))?;

        return Ok(actual_addr);
    }

    Err(anyhow::anyhow!(
        "could not bind dashboard to any port {base_port}-{}: {:?}",
        base_port + 4,
        last_error
    ))
}
