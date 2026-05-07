//! SQLite connection pool initialization.
//!
//! Configure with: `journal_mode = WAL`, `synchronous = NORMAL`,
//! `foreign_keys = ON`, `busy_timeout = 5000`. Single `Arc<SqlitePool>`
//! shared across the app.
