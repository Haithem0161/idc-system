//! Sync engine -- presentation/runtime layer for the sync bounded context.
//!
//! Owns the background Tokio task lifecycle, holds the typed HTTP client,
//! emits Tauri events for UI status, and persists pulled changes into local
//! tables. Pure domain logic lives in `crate::domains::sync`.

pub mod conflict;
pub mod engine;
pub mod metrics;
pub mod outbox;
pub mod puller;
pub mod puller_entities;
pub mod pusher;

pub use engine::{SyncEngine, SyncEngineConfig, SyncEngineHandle};
