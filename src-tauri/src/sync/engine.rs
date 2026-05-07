//! Sync engine orchestrator.
//!
//! Boots on app start, subscribes to network status, drives the push and pull
//! loops, emits Tauri events (`sync:status`, `sync:progress`, `sync:conflict`),
//! and shuts down cleanly via `CancellationToken`.
