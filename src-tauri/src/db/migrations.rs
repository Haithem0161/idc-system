//! Migration runner.
//!
//! Applies SQL files embedded at compile time. Tracks state in a
//! `_migrations` table. Forward-only and idempotent.

use sqlx::SqlitePool;
use tracing::info;

use crate::error::{AppError, AppResult};

/// Embedded migrations.
///
/// Ordering is by filename; keep the `NNN_` prefix.
const MIGRATIONS: &[(&str, &str)] = &[(
    "001_foundation.sql",
    include_str!("../../migrations/001_foundation.sql"),
)];

/// Apply every embedded migration that has not already run.
pub async fn run(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (\
            name        TEXT PRIMARY KEY,\
            applied_at  TEXT NOT NULL\
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(format!("create _migrations: {e}")))?;

    for (name, sql) in MIGRATIONS {
        let applied: Option<(String,)> =
            sqlx::query_as("SELECT name FROM _migrations WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::Internal(format!("migration check: {e}")))?;

        if applied.is_some() {
            continue;
        }

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(format!("migration tx: {e}")))?;

        for statement in split_statements(sql) {
            sqlx::query(&statement)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(format!("migration {name}: {e}")))?;
        }

        sqlx::query("INSERT INTO _migrations (name, applied_at) VALUES (?, ?)")
            .bind(name)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(format!("migration record: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(format!("migration commit: {e}")))?;

        info!(migration = name, "applied");
    }

    Ok(())
}

/// Split a SQL file into individual statements on top-level semicolons.
///
/// SQLite drivers expect one statement per call; comments and blank lines are
/// passed through (the driver tolerates them). We deliberately keep the parser
/// trivial -- migrations here never use string literals that contain `;`.
fn split_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s};"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::init_pool_in_memory;

    #[tokio::test]
    async fn migrations_apply_idempotently() {
        let pool = init_pool_in_memory().await.unwrap();
        run(&pool).await.unwrap();
        run(&pool).await.unwrap();

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);

        // Tables exist
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
            ('outbox', 'sync_state', 'audit_log', 'metrics_events')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 4);
    }
}
