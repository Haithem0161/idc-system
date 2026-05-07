//! Local SQLite persistence.
//!
//! Owns the connection pool, migration runner, and transaction helpers.
//! See `.claude/rules/rust.md` and `.claude/rules/offline-first.md`.

pub mod migrations;
pub mod sqlite;
