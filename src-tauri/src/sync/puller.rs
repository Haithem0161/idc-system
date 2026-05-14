//! Pull loop step: GET /sync/pull?since=<cursor>, apply changes in a single
//! tx, persist the cursor.

use std::collections::BTreeSet;
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
    // DEF-002 fix: cursor write must share the apply tx's connection,
    // otherwise a real-world file SQLite deadlocks (tx holds the writer,
    // a second connection blocks waiting for the lock). The single-tx
    // path also satisfies phase-01 §4 pull-step 3's atomicity claim
    // (apply + persist cursor in one SQLite tx).
    state_repo
        .put_pull_cursor_in_tx(&mut tx, &resp.next_cursor)
        .await?;
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
    // Phase-06 §7.9 pull-time recompute hook: after applying any
    // `inventory_items` or `inventory_adjustments` row, the local on-hand
    // total for the affected items is OVERWRITTEN by a fresh SUM of the
    // local non-deleted adjustments. Server `quantityOnHand` is treated as
    // informational only -- the client is canonical for this column per
    // PRD §6.1.12.
    let mut affected_inventory_items: BTreeSet<String> = BTreeSet::new();
    for change in changes {
        match change.entity.as_str() {
            "audit_log" => {
                apply_audit_change(tx, change).await?;
                applied += 1;
            }
            "inventory_items" => {
                if let Some(item_id) = apply_inventory_item_change(tx, change).await? {
                    affected_inventory_items.insert(item_id);
                    applied += 1;
                }
            }
            "inventory_adjustments" => {
                if let Some(item_id) = apply_inventory_adjustment_change(tx, change).await? {
                    affected_inventory_items.insert(item_id);
                    applied += 1;
                }
            }
            _ => {
                // Other entities are not yet pulled into local SQLite in this
                // phase; skip defensively.
                continue;
            }
        }
    }

    // Recompute on-hand for every affected item. We bypass the
    // InventoryAdjustmentRepo trait to keep the puller free of
    // domain-layer dependencies; the SQL is identical (phase-06 §7.2).
    for item_id in affected_inventory_items {
        recompute_item_on_hand(tx, &item_id).await?;
    }

    Ok(applied)
}

async fn recompute_item_on_hand(tx: &mut crate::db::Tx<'_>, item_id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE inventory_items \
         SET quantity_on_hand = ( \
             SELECT COALESCE(SUM(delta), 0) \
             FROM inventory_adjustments \
             WHERE item_id = inventory_items.id \
               AND deleted_at IS NULL \
         ), \
         updated_at = ?, \
         version = version + 1, \
         dirty = 1 \
         WHERE id = ?",
    )
    .bind(now)
    .bind(item_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_inventory_item_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<Option<String>> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(None);
    }
    // LWW: only apply if the incoming version is strictly greater. We do
    // NOT touch `quantity_on_hand` here -- it is locally canonical and
    // gets recomputed below.
    let row = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT version, updated_at FROM inventory_items WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    let incoming_version = change.version;
    if let Some((existing_version, _existing_updated_at)) = row.as_ref() {
        if incoming_version <= *existing_version {
            // Stale or equal; still mark as affected so we run a recompute
            // (defends against post-recovery drift after Pass-3 §7.32 pulled_at).
            return Ok(Some(id.into()));
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO inventory_items ( \
            id, name_ar, name_en, unit, quantity_on_hand, low_stock_threshold, \
            is_active, created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name_ar = excluded.name_ar, \
            name_en = excluded.name_en, \
            unit = excluded.unit, \
            low_stock_threshold = excluded.low_stock_threshold, \
            is_active = excluded.is_active, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE inventory_items.version < excluded.version",
    )
    .bind(id)
    .bind(p.get("name_ar").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("name_en").and_then(|v| v.as_str()))
    .bind(p.get("unit").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("quantity_on_hand")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    )
    .bind(
        p.get("low_stock_threshold")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    )
    .bind(p.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true) as i64)
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
    .bind(incoming_version)
    .bind(now)
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .execute(&mut **tx)
    .await?;

    Ok(Some(id.into()))
}

async fn apply_inventory_adjustment_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<Option<String>> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let item_id = p
        .get("item_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if id.is_empty() || item_id.is_empty() {
        return Ok(None);
    }
    let now = chrono::Utc::now().to_rfc3339();
    // Additive-only: INSERT OR IGNORE. The phase-05 §7.33 immutability
    // trigger blocks any subsequent business-column mutation.
    sqlx::query(
        "INSERT OR IGNORE INTO inventory_adjustments ( \
            id, item_id, delta, reason, visit_id, note, by_user_id, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?,?)",
    )
    .bind(id)
    .bind(item_id)
    .bind(p.get("delta").and_then(|v| v.as_i64()).unwrap_or(0))
    .bind(p.get("reason").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("visit_id").and_then(|v| v.as_str()))
    .bind(p.get("note").and_then(|v| v.as_str()))
    .bind(p.get("by_user_id").and_then(|v| v.as_str()).unwrap_or(""))
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
    .bind(now)
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .execute(&mut **tx)
    .await?;
    Ok(Some(item_id.into()))
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
