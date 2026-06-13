//! Push loop step: drain outbox, POST /sync/push, ack on success, park on
//! conflict, exponential backoff on 5xx.

use std::sync::Arc;
use std::time::Instant;

use tracing::{info, warn};
use uuid::Uuid;

use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{OutboxRepo, SyncStateRepo};
use crate::domains::sync::infrastructure::SyncHttpClient;
use crate::error::{AppError, AppResult};
use crate::sync::conflict::Conflict;
use crate::sync::metrics::{write as write_metric, MetricKind};

pub const BATCH_SIZE: usize = 50;

pub struct PushOutcome {
    pub pushed: usize,
    pub conflicts: Vec<Conflict>,
    pub session_expired: bool,
}

pub async fn run_step(
    pool: &sqlx::SqlitePool,
    outbox_repo: Arc<dyn OutboxRepo>,
    state_repo: Arc<dyn SyncStateRepo>,
    http: &SyncHttpClient,
    token: Option<&str>,
    entity_id_tenant: &str,
) -> AppResult<PushOutcome> {
    let batch = outbox_repo.next_batch(BATCH_SIZE).await?;
    if batch.is_empty() {
        return Ok(PushOutcome {
            pushed: 0,
            conflicts: vec![],
            session_expired: false,
        });
    }

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            // No token yet -- defer until auth lands. The engine sets status
            // to Offline / Error so the user can see it.
            return Ok(PushOutcome {
                pushed: 0,
                conflicts: vec![],
                session_expired: false,
            });
        }
    };

    let payload: Vec<_> = batch.iter().map(crate::sync::outbox::to_push_op).collect();
    let start = Instant::now();
    let result = match http.push(token, &payload).await {
        Ok(r) => r,
        Err(AppError::SessionExpired) => {
            warn!("push: session expired");
            return Ok(PushOutcome {
                pushed: 0,
                conflicts: vec![],
                session_expired: true,
            });
        }
        Err(e) => {
            write_metric(
                pool,
                entity_id_tenant,
                MetricKind::SyncPushFail,
                serde_json::json!({
                    "batch_size": batch.len(),
                    "error": e.to_string(),
                }),
            )
            .await;
            // The whole HTTP call failed -- a transport-level error, not an
            // op-specific one. Reschedule the batch WITHOUT burning attempts so
            // a device that is merely offline never strands its queue once
            // connectivity returns.
            reschedule_transient_all(&outbox_repo, &batch, &e.to_string()).await?;
            return Err(e);
        }
    };

    // Ack accepted ops
    let accepted_ids: Vec<Uuid> = result
        .accepted
        .iter()
        .filter_map(|r| Uuid::parse_str(&r.op_id).ok())
        .collect();

    // Mark the underlying business rows clean BEFORE deleting the outbox ops
    // (after delete we lose the entity/entity_id mapping). Without this the
    // pushed rows stay dirty=1 forever, so the dirty flag is meaningless and
    // the audit-retention vacuum can never purge own-device rows.
    let accepted_set: std::collections::HashSet<Uuid> = accepted_ids.iter().copied().collect();
    let synced_entities: Vec<(String, String)> = batch
        .iter()
        .filter(|op| accepted_set.contains(&op.op_id))
        .map(|op| (op.entity.clone(), op.entity_id.clone()))
        .collect();
    outbox_repo.mark_entities_synced(&synced_entities).await?;

    outbox_repo.delete_acked(&accepted_ids).await?;

    // Park conflicts
    for conflict in &result.conflicts {
        if let Ok(id) = Uuid::parse_str(&conflict.op_id) {
            let _ = outbox_repo.park(id).await;
            write_metric(
                pool,
                entity_id_tenant,
                MetricKind::SyncConflict,
                serde_json::json!({
                    "op_id": conflict.op_id,
                    "entity": conflict.entity,
                    "auto_resolved": false,
                }),
            )
            .await;
        }
    }

    // Park per-op rejections (validation / authorization). The server isolated
    // these instead of aborting the batch; parking them keeps one poison op
    // from blocking every later push, and surfaces them via `stuck_count` /
    // `list_stuck` for manual recovery rather than letting them vanish.
    for rejected in &result.rejected {
        if let Ok(id) = Uuid::parse_str(&rejected.op_id) {
            let reason = format!("{}: {}", rejected.code, rejected.message);
            let _ = outbox_repo.park_with_error(id, &reason).await;
            warn!(op_id = %rejected.op_id, code = %rejected.code, "push: op rejected, parked");
            write_metric(
                pool,
                entity_id_tenant,
                MetricKind::SyncPushFail,
                serde_json::json!({
                    "op_id": rejected.op_id,
                    "rejected": true,
                    "code": rejected.code,
                }),
            )
            .await;
        }
    }

    let _ = state_repo.mark_pushed().await;

    write_metric(
        pool,
        entity_id_tenant,
        MetricKind::SyncPushOk,
        serde_json::json!({
            "batch_size": batch.len(),
            "accepted": result.accepted.len(),
            "conflicts": result.conflicts.len(),
            "duration_ms": start.elapsed().as_millis(),
        }),
    )
    .await;

    let conflicts: Vec<Conflict> = result.conflicts.into_iter().map(Into::into).collect();
    info!(
        accepted = accepted_ids.len(),
        conflicts = conflicts.len(),
        "push complete"
    );

    Ok(PushOutcome {
        pushed: accepted_ids.len(),
        conflicts,
        session_expired: false,
    })
}

/// Reschedule a whole batch after a transport-level failure WITHOUT bumping
/// `attempts`. Backoff still grows with the existing attempt count so a server
/// that is down for a while is polled less aggressively, but the cap is never
/// consumed by connectivity problems.
async fn reschedule_transient_all(
    repo: &Arc<dyn OutboxRepo>,
    batch: &[OutboxOp],
    err: &str,
) -> AppResult<()> {
    for op in batch {
        let backoff = OutboxOp::next_backoff(op.attempts).as_secs();
        repo.reschedule_transient(op.op_id, err, backoff).await?;
    }
    Ok(())
}
