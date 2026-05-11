//! SQLite connection pool initialization.
//!
//! Single `Arc<SqlitePool>` shared across the app. Configured with
//! `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`,
//! `busy_timeout = 5000`.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

/// Open (or create) the local SQLite database and return a shared pool.
///
/// Caller passes the absolute path; the file is created if it does not exist.
pub async fn init_pool(db_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(AppError::from)?;
        }
    }

    let url = format!("sqlite://{}", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)
        .map_err(|e| AppError::Internal(format!("sqlite options: {e}")))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(5_000));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await
        .map_err(|e| AppError::Internal(format!("sqlite connect: {e}")))?;

    Ok(pool)
}

/// Open an in-memory pool for tests.
#[cfg(test)]
pub async fn init_pool_in_memory() -> AppResult<SqlitePool> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(|e| AppError::Internal(format!("sqlite options: {e}")))?
        .journal_mode(SqliteJournalMode::Memory)
        .synchronous(SqliteSynchronous::Off)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| AppError::Internal(format!("sqlite connect: {e}")))?;

    Ok(pool)
}
