//! Pull loop step: GET /sync/pull?since=<cursor>, apply changes in a single
//! tx, persist the cursor.

use std::sync::Arc;
use std::time::Instant;

use sqlx::SqlitePool;
use tracing::info;

use crate::domains::sync::domain::repositories::SyncStateRepo;
use crate::domains::sync::infrastructure::{PullChange, SyncHttpClient};
use crate::error::{AppError, AppResult};
use crate::sync::metrics::{write as write_metric, MetricKind};

pub struct PullOutcome {
    pub applied: usize,
    pub session_expired: bool,
}

pub async fn run_step(
    pool: &SqlitePool,
    state_repo: Arc<dyn SyncStateRepo>,
    http: &SyncHttpClient,
    token: Option<&str>,
    entity_id_tenant: &str,
) -> AppResult<PullOutcome> {
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(PullOutcome {
                applied: 0,
                session_expired: false,
            });
        }
    };

    let state = state_repo.get().await?;
    let start = Instant::now();
    let resp = match http.pull(token, state.pull_cursor.as_deref()).await {
        Ok(r) => r,
        Err(AppError::SessionExpired) => {
            return Ok(PullOutcome {
                applied: 0,
                session_expired: true,
            });
        }
        Err(e) => {
            write_metric(
                pool,
                entity_id_tenant,
                MetricKind::SyncPullFail,
                serde_json::json!({
                    "since_cursor": state.pull_cursor,
                    "error": e.to_string(),
                }),
            )
            .await;
            return Err(e);
        }
    };

    if resp.changes.is_empty() {
        // Empty pull -- still advance the cursor server-side (the server is
        // authoritative) but it should already match.
        return Ok(PullOutcome {
            applied: 0,
            session_expired: false,
        });
    }

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    let applied = apply_changes(&mut tx, &resp.changes).await?;
    state_repo.put_pull_cursor(&resp.next_cursor).await?;
    tx.commit().await.map_err(AppError::from)?;

    write_metric(
        pool,
        entity_id_tenant,
        MetricKind::SyncPullOk,
        serde_json::json!({
            "batch_size": applied,
            "since_cursor": state.pull_cursor,
            "duration_ms": start.elapsed().as_millis(),
        }),
    )
    .await;

    info!(applied, "pull complete");

    Ok(PullOutcome {
        applied,
        session_expired: false,
    })
}

async fn apply_changes(tx: &mut crate::db::Tx<'_>, changes: &[PullChange]) -> AppResult<usize> {
    let mut applied = 0;
    for change in changes {
        // Only audit_log is syncable in Phase 1; other entities arrive only
        // after their owning phase ships. Skip unknown entities defensively.
        if change.entity != "audit_log" {
            continue;
        }
        apply_audit_change(tx, change).await?;
        applied += 1;
    }
    Ok(applied)
}

async fn apply_audit_change(tx: &mut crate::db::Tx<'_>, change: &PullChange) -> AppResult<()> {
    // The server emits the audit row with the canonical sync columns. We
    // INSERT OR IGNORE because additive-only entities never overwrite an
    // existing row.
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }

    sqlx::query(
        "INSERT OR IGNORE INTO audit_log (\
            id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, \
            created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
            origin_device_id, entity_id_tenant\
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?)",
    )
    .bind(id)
    .bind(
        p.get("actor_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("action").and_then(|v| v.as_str()).unwrap_or("create"))
    .bind(p.get("entity").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("delta")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".into()),
    )
    .bind(p.get("ip").and_then(|v| v.as_str()))
    .bind(p.get("device_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("at")
            .and_then(|v| v.as_str())
            .unwrap_or(&change.updated_at),
    )
    .bind(
        p.get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or(&change.updated_at),
    )
    .bind(
        p.get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or(&change.updated_at),
    )
    .bind(p.get("deleted_at").and_then(|v| v.as_str()))
    .bind(change.version)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(
        p.get("entity_id_tenant")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}
