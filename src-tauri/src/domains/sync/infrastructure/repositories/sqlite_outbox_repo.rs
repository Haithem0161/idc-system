//! sqlx-backed implementation of `OutboxRepo`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::OutboxRepo;
use crate::domains::sync::domain::value_objects::OutboxAction;
use crate::error::AppResult;

#[derive(Clone)]
pub struct SqliteOutboxRepo {
    pool: SqlitePool,
}

impl SqliteOutboxRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OutboxRepo for SqliteOutboxRepo {
    async fn enqueue(&self, tx: &mut Tx<'_>, op: &OutboxOp) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO outbox \
            (op_id, entity, entity_id, op, payload, created_at, attempts, next_attempt_at, last_error, parked) \
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(op.op_id.to_string())
        .bind(&op.entity)
        .bind(&op.entity_id)
        .bind(op.op.as_str())
        .bind(&op.payload)
        .bind(op.created_at.to_rfc3339())
        .bind(op.attempts)
        .bind(op.next_attempt_at.to_rfc3339())
        .bind(op.last_error.as_deref())
        .bind(op.parked as i64)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn next_batch(&self, limit: usize) -> AppResult<Vec<OutboxOp>> {
        let now = Utc::now().to_rfc3339();
        let rows: Vec<OutboxRow> = sqlx::query_as::<_, OutboxRow>(
            "SELECT op_id, entity, entity_id, op, payload, created_at, attempts, \
                    next_attempt_at, last_error, parked \
             FROM outbox \
             WHERE attempts < 10 AND parked = 0 AND next_attempt_at <= ? \
             ORDER BY next_attempt_at ASC \
             LIMIT ?",
        )
        .bind(now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(OutboxRow::try_into_domain).collect()
    }

    async fn pending_count(&self) -> AppResult<u32> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE attempts < 10 AND parked = 0")
                .fetch_one(&self.pool)
                .await?;
        Ok(n.max(0) as u32)
    }

    async fn mark_failure(&self, op_id: Uuid, error: &str, backoff_secs: u64) -> AppResult<()> {
        let next = (Utc::now() + chrono::Duration::seconds(backoff_secs as i64)).to_rfc3339();
        sqlx::query(
            "UPDATE outbox \
             SET attempts = attempts + 1, last_error = ?, next_attempt_at = ? \
             WHERE op_id = ?",
        )
        .bind(error)
        .bind(next)
        .bind(op_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn park(&self, op_id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE outbox SET parked = 1 WHERE op_id = ?")
            .bind(op_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_acked(&self, op_ids: &[Uuid]) -> AppResult<()> {
        if op_ids.is_empty() {
            return Ok(());
        }
        let placeholders = std::iter::repeat("?")
            .take(op_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM outbox WHERE op_id IN ({placeholders})");
        let mut query = sqlx::query(&sql);
        for id in op_ids {
            query = query.bind(id.to_string());
        }
        query.execute(&self.pool).await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct OutboxRow {
    op_id: String,
    entity: String,
    entity_id: String,
    op: String,
    payload: Vec<u8>,
    created_at: String,
    attempts: i64,
    next_attempt_at: String,
    last_error: Option<String>,
    parked: i64,
}

impl OutboxRow {
    fn try_into_domain(self) -> AppResult<OutboxOp> {
        let op = match self.op.as_str() {
            "upsert" => OutboxAction::Upsert,
            other => {
                return Err(crate::error::AppError::Validation(format!(
                    "unsupported outbox op kind: {other}"
                )))
            }
        };
        let op_id = Uuid::parse_str(&self.op_id)?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&self.created_at)
            .map_err(|e| crate::error::AppError::Validation(format!("created_at: {e}")))?
            .with_timezone(&Utc);
        let next_attempt_at = chrono::DateTime::parse_from_rfc3339(&self.next_attempt_at)
            .map_err(|e| crate::error::AppError::Validation(format!("next_attempt_at: {e}")))?
            .with_timezone(&Utc);

        Ok(OutboxOp::reconstitute(
            op_id,
            self.entity,
            self.entity_id,
            op,
            self.payload,
            created_at,
            self.attempts as i32,
            next_attempt_at,
            self.last_error,
            self.parked != 0,
        ))
    }
}
