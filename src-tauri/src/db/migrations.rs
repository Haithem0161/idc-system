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
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_foundation.sql",
        include_str!("../../migrations/001_foundation.sql"),
    ),
    (
        "002_users_settings.sql",
        include_str!("../../migrations/002_users_settings.sql"),
    ),
];

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
/// Skips `;` inside `--` line comments and inside single-quoted string
/// literals. Migrations here never use multi-line `/* ... */` comments or
/// dollar-quoted strings; extend this parser if that changes.
fn split_statements(sql: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if in_line_comment {
            current.push(c);
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            current.push(c);
            if c == '\'' {
                in_string = false;
            }
            continue;
        }
        match c {
            '-' if matches!(chars.peek(), Some('-')) => {
                current.push(c);
                in_line_comment = true;
            }
            '\'' => {
                current.push(c);
                in_string = true;
            }
            ';' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(format!("{trimmed};"));
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        out.push(format!("{trimmed};"));
    }
    out
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
            ('outbox', 'sync_state', 'audit_log', 'metrics_events', 'users', 'settings')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 6);

        // Seed populated 10 setting keys
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 10);
    }
}
