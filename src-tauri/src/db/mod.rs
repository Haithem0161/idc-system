//! Local SQLite persistence.
//!
//! Owns the connection pool, migration runner, and transaction helpers.

pub mod migrations;
pub mod sqlite;

pub use sqlite::init_pool;

/// Convenience alias for an active SQLite transaction. Domain layers carry
/// this type only via the infrastructure boundary; pure-domain code stays
/// dependency-free.
pub type Tx<'a> = sqlx::Transaction<'a, sqlx::Sqlite>;
