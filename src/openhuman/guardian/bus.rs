//! Event bus subscribers for the Guardian domain.
//!
//! Provides [`GuardianBlockingSubscriber`] which logs every N1 block decision
//! so operators can monitor which tools are being blocked and why.

use crate::core::event_bus::events::DomainEvent;
use crate::core::event_bus::subscriber::EventHandler;

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
}
