//! Dashboard domain — local real-time observability server.
//!
//! Serves an HTML dashboard on a dedicated port (default `7790`) showing:
//! - Guardian decisions timeline (N1 / N2 / N3 verdicts)
//! - Active skills list
//! - Memory statistics
//! - Tool execution history
//!
//! Everything is populated by an [`EventHandler`] subscriber that persists
//! incoming [`DomainEvent`]s to a SQLite event store.

pub mod bus;
pub mod schemas;
pub mod server;
pub mod store;
pub mod types;

pub use types::DashboardConfig;
