//! Sync bounded context.
//!
//! Tracks outbound mutations (outbox), receives remote changes (pull cursor),
//! records the audit log, and owns the conflict envelope. The engine itself
//! lives in `crate::sync` (top-level) because it owns lifecycle and emits
//! Tauri events, which is presentation-layer concern.

pub mod commands;
pub mod domain;
pub mod infrastructure;

pub use domain::value_objects::{AuditAction, OutboxAction, SyncStatus};
