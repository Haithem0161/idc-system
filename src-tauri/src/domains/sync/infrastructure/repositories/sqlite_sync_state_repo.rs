//! sqlx-backed implementation of `SyncStateRepo`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;

use crate::domains::sync::domain::entities::SyncState;
use crate::domains::sync::domain::repositories::SyncStateRepo;
use crate::error::AppResult;

#[derive(Clone)]
pub struct SqliteSyncStateRepo {
    pool: SqlitePool,
}

impl SqliteSyncStateRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SyncStateRepo for SqliteSyncStateRepo {
    async fn get(&self) -> AppResult<SyncState> {
        let row: Option<SyncStateRow> = sqlx::query_as::<_, SyncStateRow>(
            "SELECT pull_cursor, last_pulled_at, last_pushed_at, device_id, last_audit_vacuum_at \
             FROM sync_state WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => r.into_domain(),
            None => Err(crate::error::AppError::NotFound("sync_state".into())),
        }
    }

    async fn put_pull_cursor(&self, cursor: &str) -> AppResult<()> {
        sqlx::query("UPDATE sync_state SET pull_cursor = ?, last_pulled_at = ? WHERE id = 1")
            .bind(cursor)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_pushed(&self) -> AppResult<()> {
        sqlx::query("UPDATE sync_state SET last_pushed_at = ? WHERE id = 1")
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_audit_vacuumed(&self, at: chrono::DateTime<chrono::Utc>) -> AppResult<()> {
        sqlx::query("UPDATE sync_state SET last_audit_vacuum_at = ? WHERE id = 1")
            .bind(at.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn ensure_device_id(&self, device_id: &str) -> AppResult<String> {
        // INSERT OR IGNORE then read back; cheaper than SELECT-then-INSERT.
        sqlx::query("INSERT OR IGNORE INTO sync_state (id, device_id) VALUES (1, ?)")
            .bind(device_id)
            .execute(&self.pool)
            .await?;

        let (existing,): (String,) =
            sqlx::query_as("SELECT device_id FROM sync_state WHERE id = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(existing)
    }
}

#[derive(sqlx::FromRow)]
struct SyncStateRow {
    pull_cursor: Option<String>,
    last_pulled_at: Option<String>,
    last_pushed_at: Option<String>,
    device_id: String,
    last_audit_vacuum_at: Option<String>,
}

impl SyncStateRow {
    fn into_domain(self) -> AppResult<SyncState> {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::AppError::Validation(format!("datetime: {e}")))
        };
        Ok(SyncState {
            pull_cursor: self.pull_cursor,
            last_pulled_at: self.last_pulled_at.as_deref().map(parse_dt).transpose()?,
            last_pushed_at: self.last_pushed_at.as_deref().map(parse_dt).transpose()?,
            device_id: self.device_id,
            last_audit_vacuum_at: self
                .last_audit_vacuum_at
                .as_deref()
                .map(parse_dt)
                .transpose()?,
        })
    }
}
