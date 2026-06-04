//! Rollback domain — pre-write snapshot infrastructure (UND-01).
//!
//! Stores indexed pre-write file snapshots in SQLite and diff files under
//! `.dadou/history/`. Each write operation is recorded with an action_id
//! (UUID v4), SHA-256 checksum, ISO 8601 timestamp, and a unified diff.
//!
//! ## Module layout
//!
//! | File | Purpose |
//! |------|---------|
//! | `types.rs` | `RollbackEntry`, `RollbackError`, `FileSnapshot` |
//! | `store.rs` | `RollbackStore` — SQLite CRUD + diff I/O |
//! | `schemas.rs` | JSON-RPC controller schemas (plan 06 stubs) |
//! | `mod.rs` | Re-exports and module declarations |

pub mod ops;
pub mod schemas;
pub mod store;
pub mod types;

pub use store::RollbackStore;
pub use types::{FileSnapshot, RollbackEntry, RollbackError};

pub use schemas::{
    all_controller_schemas as all_rollback_controller_schemas,
    all_registered_controllers as all_rollback_registered_controllers,
};
