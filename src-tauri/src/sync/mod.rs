//! Sync engine.
//!
//! Background Tokio task that drains the outbox to the sync server (push),
//! pulls remote changes since the cursor (pull), and resolves conflicts per
//! entity policy. See `.claude/rules/offline-first.md` for the full contract.

pub mod conflict;
pub mod engine;
pub mod outbox;
pub mod puller;
pub mod pusher;
