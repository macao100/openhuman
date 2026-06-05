//! Event bus subscribers for the Guardian domain.
//!
//! Provides [`GuardianBlockingSubscriber`] which logs every N1 block decision
//! so operators can monitor which tools are being blocked and why.

use crate::core::event_bus::{DomainEvent, EventHandler};

/// Logs Guardian N1 blocking events.
///
/// Subscribes to the "guardian" domain and emits a structured `log::warn!`
/// for every blocked tool call, including the tool name, latency, and reason.
pub struct GuardianBlockingSubscriber;

#[async_trait::async_trait]
impl EventHandler for GuardianBlockingSubscriber {
    fn name(&self) -> &'static str {
        "guardian::blocking_logger"
    }

    fn domains(&self) -> Option<&'static [&'static str]> {
        Some(&["guardian"])
    }

    async fn handle(&self, event: &DomainEvent) {
        if let DomainEvent::GuardianBlocked {
            tool_name,
            reason,
            latency_us,
        } = event
        {
            log::warn!(
                "[guardian] N1 blocked tool={} latency={}μs reason={}",
                tool_name,
                latency_us,
                reason,
            );
        }
    }
}

/// Logs Guardian N2 blocking events.
///
/// Subscribes to the "guardian" domain and logs every action blocked
/// by the N2 heuristic classifiers, including the scores that triggered
/// the block.
pub struct N2BlockingSubscriber;

#[async_trait::async_trait]
impl EventHandler for N2BlockingSubscriber {
    fn name(&self) -> &'static str {
        "guardian::n2_blocking_logger"
    }

    fn domains(&self) -> Option<&'static [&'static str]> {
        Some(&["guardian"])
    }

    async fn handle(&self, event: &DomainEvent) {
        if let DomainEvent::N2Blocked {
            tool_name,
            reason,
            scores_json,
            latency_us,
        } = event
        {
            log::warn!(
                "[guardian] N2 blocked tool={} latency={}μs scores={} reason={}",
                tool_name,
                latency_us,
                scores_json,
                reason,
            );
        }
    }
}

/// Logs Guardian N3 validation results.
///
/// Subscribes to the "guardian" domain and logs every N3 LLM validation
/// result, including the verdict, tool name, latency, and reason.
pub struct N3ResultSubscriber;

#[async_trait::async_trait]
impl EventHandler for N3ResultSubscriber {
    fn name(&self) -> &'static str {
        "guardian::n3_result_logger"
    }

    fn domains(&self) -> Option<&'static [&'static str]> {
        Some(&["guardian"])
    }

    async fn handle(&self, event: &DomainEvent) {
        if let DomainEvent::N3Result {
            tool_name,
            verdict,
            reason,
            latency_us,
        } = event
        {
            log::info!(
                "[guardian] N3 verdict={} tool={} latency={}μs reason={}",
                verdict,
                tool_name,
                latency_us,
                reason,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event_bus::bus::{publish_global, subscribe_global, init_global};
    use std::sync::Arc;

    #[tokio::test]
    async fn subscriber_receives_guardian_blocked_event() {
        init_global(16);

        let received = Arc::new(tokio::sync::Notify::new());
        let received_clone = received.clone();

        struct TestHandler {
            notify: Arc<tokio::sync::Notify>,
        }

        #[async_trait::async_trait]
        impl EventHandler for TestHandler {
            fn name(&self) -> &'static str {
                "guardian::test_handler"
            }

            fn domains(&self) -> Option<&'static [&'static str]> {
                Some(&["guardian"])
            }

            async fn handle(&self, event: &DomainEvent) {
                if matches!(event, DomainEvent::GuardianBlocked { .. }) {
                    self.notify.notify_one();
                }
            }
        }

        let _handle = subscribe_global(Arc::new(TestHandler {
            notify: received_clone.clone(),
        }));

        publish_global(DomainEvent::GuardianBlocked {
            tool_name: "shell".to_string(),
            reason: "[policy-blocked] test block".to_string(),
            latency_us: 42,
        });

        tokio::time::timeout(std::time::Duration::from_secs(5), received.notified())
            .await
            .expect("should receive GuardianBlocked event within timeout");
    }

    #[tokio::test]
    async fn guardian_blocked_event_fields_are_correct() {
        let event = DomainEvent::GuardianBlocked {
            tool_name: "file_write".to_string(),
            reason: "[policy-blocked] path not allowed".to_string(),
            latency_us: 123,
        };

        match &event {
            DomainEvent::GuardianBlocked {
                tool_name,
                reason,
                latency_us,
            } => {
                assert_eq!(tool_name, "file_write");
                assert!(reason.contains("[policy-blocked]"));
                assert_eq!(*latency_us, 123);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[tokio::test]
    async fn guardian_blocked_event_domain_is_guardian() {
        let event = DomainEvent::GuardianBlocked {
            tool_name: "shell".to_string(),
            reason: "blocked".to_string(),
            latency_us: 0,
        };
        assert_eq!(event.domain(), "guardian");
    }

    // ── N2 blocking subscriber tests ────────────────────────────────

    #[tokio::test]
    async fn n2_blocking_subscriber_receives_n2_blocked_event() {
        init_global(16);

        let received = Arc::new(tokio::sync::Notify::new());
        let received_clone = received.clone();

        struct N2TestHandler {
            notify: Arc<tokio::sync::Notify>,
        }

        #[async_trait::async_trait]
        impl EventHandler for N2TestHandler {
            fn name(&self) -> &'static str {
                "guardian::n2_test_handler"
            }

            fn domains(&self) -> Option<&'static [&'static str]> {
                Some(&["guardian"])
            }

            async fn handle(&self, event: &DomainEvent) {
                if matches!(event, DomainEvent::N2Blocked { .. }) {
                    self.notify.notify_one();
                }
            }
        }

        let _handle = subscribe_global(Arc::new(N2TestHandler {
            notify: received_clone.clone(),
        }));

        publish_global(DomainEvent::N2Blocked {
            tool_name: "shell".to_string(),
            reason: "N2 blocked test".to_string(),
            scores_json: "[]".to_string(),
            latency_us: 42,
        });

        tokio::time::timeout(std::time::Duration::from_secs(5), received.notified())
            .await
            .expect("should receive N2Blocked event within timeout");
    }

    #[tokio::test]
    async fn n2_blocked_event_fields_are_correct() {
        let event = DomainEvent::N2Blocked {
            tool_name: "file_write".to_string(),
            reason: "high entropy detected".to_string(),
            scores_json: r#"[{"score":0.7,"reason":"base64","triggered_by":"entropy"}]"#.to_string(),
            latency_us: 123,
        };

        match &event {
            DomainEvent::N2Blocked {
                tool_name,
                reason,
                scores_json,
                latency_us,
            } => {
                assert_eq!(tool_name, "file_write");
                assert_eq!(reason, "high entropy detected");
                assert!(scores_json.contains("0.7"));
                assert_eq!(*latency_us, 123);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[tokio::test]
    async fn n2_blocked_event_domain_is_guardian() {
        let event = DomainEvent::N2Blocked {
            tool_name: "shell".to_string(),
            reason: "blocked".to_string(),
            scores_json: "[]".to_string(),
            latency_us: 0,
        };
        assert_eq!(event.domain(), "guardian");
    }

    #[tokio::test]
    async fn n2_escalated_event_domain_is_guardian() {
        let event = DomainEvent::N2Escalated {
            tool_name: "shell".to_string(),
            scores_json: "[]".to_string(),
            latency_us: 0,
        };
        assert_eq!(event.domain(), "guardian");
    }

    // ── N3 result subscriber tests ──────────────────────────────────

    #[tokio::test]
    async fn n3_result_subscriber_receives_n3_result_event() {
        init_global(16);

        let received = Arc::new(tokio::sync::Notify::new());
        let received_clone = received.clone();

        struct N3TestHandler {
            notify: Arc<tokio::sync::Notify>,
        }

        #[async_trait::async_trait]
        impl EventHandler for N3TestHandler {
            fn name(&self) -> &'static str {
                "guardian::n3_test_handler"
            }

            fn domains(&self) -> Option<&'static [&'static str]> {
                Some(&["guardian"])
            }

            async fn handle(&self, event: &DomainEvent) {
                if matches!(event, DomainEvent::N3Result { .. }) {
                    self.notify.notify_one();
                }
            }
        }

        let _handle = subscribe_global(Arc::new(N3TestHandler {
            notify: received_clone.clone(),
        }));

        publish_global(DomainEvent::N3Result {
            tool_name: "shell".to_string(),
            verdict: "block".to_string(),
            reason: "malicious pattern detected".to_string(),
            latency_us: 456,
        });

        tokio::time::timeout(std::time::Duration::from_secs(5), received.notified())
            .await
            .expect("should receive N3Result event within timeout");
    }

    #[tokio::test]
    async fn n3_result_event_fields_are_correct() {
        let event = DomainEvent::N3Result {
            tool_name: "shell".to_string(),
            verdict: "block".to_string(),
            reason: "suspicious".to_string(),
            latency_us: 789,
        };

        match &event {
            DomainEvent::N3Result {
                tool_name,
                verdict,
                reason,
                latency_us,
            } => {
                assert_eq!(tool_name, "shell");
                assert_eq!(verdict, "block");
                assert_eq!(reason, "suspicious");
                assert_eq!(*latency_us, 789);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[tokio::test]
    async fn n3_result_event_domain_is_guardian() {
        let event = DomainEvent::N3Result {
            tool_name: "shell".to_string(),
            verdict: "allow".to_string(),
            reason: "safe".to_string(),
            latency_us: 0,
        };
        assert_eq!(event.domain(), "guardian");
    }
}
