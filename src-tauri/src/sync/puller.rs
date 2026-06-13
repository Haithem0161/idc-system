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
    /// Distinct entity tables touched by this pull. The engine emits these in
    /// a `sync:applied` event so the frontend can invalidate exactly the
    /// affected React Query caches; without it, pulled peer-device data stays
    /// invisible on mounted screens until a manual refetch or remount.
    pub affected_entities: Vec<String>,
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
                affected_entities: Vec::new(),
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
                affected_entities: Vec::new(),
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
        // Empty pull -- nothing to apply and the cursor already matches, but a
        // successful round-trip still happened. Stamp `last_pulled_at` so the
        // "last pulled" diagnostic doesn't go stale on a quiet day while sync
        // is healthy (without this it froze at the last non-empty pull).
        if let Err(e) = state_repo.mark_pulled().await {
            tracing::warn!(error = %e, "empty pull: failed to stamp last_pulled_at");
        }
        return Ok(PullOutcome {
            applied: 0,
            session_expired: false,
            affected_entities: Vec::new(),
        });
    }

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    let (applied, affected_entities) = apply_changes(&mut tx, &resp.changes).await?;
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
        affected_entities,
    })
}

/// Apply order respecting foreign-key dependencies. Within a single pull
/// batch we process entity types in this sequence so a child row (e.g. a
/// `visits` row referencing a `patients`/`doctors`/`operators` row) is never
/// inserted before its parent, regardless of the order the server streamed
/// them. Entities not listed are applied last (after all parents).
const APPLY_ORDER: &[&str] = &[
    "users",
    "settings",
    "check_types",
    "check_subtypes",
    "doctors",
    "doctor_check_pricing",
    "operators",
    "operator_specialties",
    "inventory_items",
    "inventory_consumption_map",
    "operator_shifts",
    "patients",
    "visits",
    "inventory_adjustments",
    "audit_log",
];

fn apply_rank(entity: &str) -> usize {
    APPLY_ORDER
        .iter()
        .position(|e| *e == entity)
        .unwrap_or(APPLY_ORDER.len())
}

async fn apply_changes(
    tx: &mut crate::db::Tx<'_>,
    changes: &[PullChange],
) -> AppResult<(usize, Vec<String>)> {
    let mut applied = 0;
    // Phase-06 §7.9 pull-time recompute hook: after applying any
    // `inventory_items` or `inventory_adjustments` row, the local on-hand
    // total for the affected items is OVERWRITTEN by a fresh SUM of the
    // local non-deleted adjustments. Server `quantityOnHand` is treated as
    // informational only -- the client is canonical for this column per
    // PRD §6.1.12.
    let mut affected_inventory_items: BTreeSet<String> = BTreeSet::new();
    // Distinct entity tables actually touched, for the `sync:applied` event.
    let mut affected_entities: BTreeSet<String> = BTreeSet::new();

    // FK-safe ordering: parents before children. A stable sort by dependency
    // rank keeps same-entity ordering (and thus version progression) intact.
    let mut ordered: Vec<&PullChange> = changes.iter().collect();
    ordered.sort_by_key(|c| apply_rank(&c.entity));

    for change in ordered {
        let applied_before = applied;
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
            "users" => {
                // DEF-007 G35: users pull-apply MUST preserve the local
                // `password_hash` byte-for-byte. The server's pull payload
                // intentionally OMITS the hash (phase-02 §7.24); even if a
                // future server regression starts sending it, the SQL
                // below never touches the `password_hash` column on
                // update.
                if apply_users_change(tx, change).await? {
                    applied += 1;
                }
            }
            // C3/C5: the remaining syncable entities. Previously these were
            // silently skipped while the cursor advanced past them, so peer
            // changes to patients/visits/settings/catalog/shifts were dropped
            // permanently. Each handler is an LWW-gated upsert (see
            // `puller_entities`).
            "settings" => {
                crate::sync::puller_entities::apply_settings_change(tx, change).await?;
                applied += 1;
            }
            "check_types" => {
                crate::sync::puller_entities::apply_check_types_change(tx, change).await?;
                applied += 1;
            }
            "check_subtypes" => {
                crate::sync::puller_entities::apply_check_subtypes_change(tx, change).await?;
                applied += 1;
            }
            "doctors" => {
                crate::sync::puller_entities::apply_doctors_change(tx, change).await?;
                applied += 1;
            }
            "doctor_check_pricing" => {
                crate::sync::puller_entities::apply_doctor_check_pricing_change(tx, change).await?;
                applied += 1;
            }
            "operators" => {
                crate::sync::puller_entities::apply_operators_change(tx, change).await?;
                applied += 1;
            }
            "operator_specialties" => {
                crate::sync::puller_entities::apply_operator_specialties_change(tx, change).await?;
                applied += 1;
            }
            "inventory_consumption_map" => {
                crate::sync::puller_entities::apply_inventory_consumption_map_change(tx, change)
                    .await?;
                applied += 1;
            }
            "operator_shifts" => {
                crate::sync::puller_entities::apply_operator_shifts_change(tx, change).await?;
                applied += 1;
            }
            "patients" => {
                crate::sync::puller_entities::apply_patients_change(tx, change).await?;
                applied += 1;
            }
            "visits" => {
                crate::sync::puller_entities::apply_visits_change(tx, change).await?;
                applied += 1;
            }
            other => {
                // An unknown entity must NOT be silently dropped while the
                // cursor advances past it (the original C3/C5 defect). Fail
                // loudly so the gap is visible instead of causing permanent
                // data loss.
                return Err(crate::error::AppError::Validation(format!(
                    "pull: unhandled entity `{other}` (cursor not advanced)"
                )));
            }
        }
        // Record the entity only when a row was actually applied (a stale LWW
        // no-op must not trigger a needless cache invalidation).
        if applied > applied_before {
            affected_entities.insert(change.entity.clone());
        }
    }

    // Recompute on-hand for every affected item. We bypass the
    // InventoryAdjustmentRepo trait to keep the puller free of
    // domain-layer dependencies; the SQL is identical (phase-06 §7.2).
    for item_id in affected_inventory_items {
        recompute_item_on_hand(tx, &item_id).await?;
    }

    Ok((applied, affected_entities.into_iter().collect()))
}

async fn recompute_item_on_hand(tx: &mut crate::db::Tx<'_>, item_id: &str) -> AppResult<()> {
    // Pull-side derived-column refresh ONLY (phase-06 §7.9). `quantity_on_hand`
    // is locally canonical, so we recompute it -- but we must NOT bump
    // `version`/`dirty`/`updated_at`. Those are the row's sync identity; the
    // local-mutation path bumps them and enqueues an outbox op, but this pull
    // path enqueues nothing. Bumping version here inflated the local version
    // above the server's, so the LWW gate then silently dropped every future
    // server update to the item, and the never-pushed dirty=1 was permanent.
    sqlx::query(
        "UPDATE inventory_items \
         SET quantity_on_hand = ( \
             SELECT COALESCE(SUM(delta), 0) \
             FROM inventory_adjustments \
             WHERE item_id = inventory_items.id \
               AND deleted_at IS NULL \
         ) \
         WHERE id = ?",
    )
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
         WHERE inventory_items.version < excluded.version \
           AND inventory_items.dirty = 0",
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

/// DEF-007 G35: apply a pulled `users` row WITHOUT touching the local
/// `password_hash` column.
///
/// - INSERT path: when the user does not exist locally yet, we insert with
///   an empty `password_hash`. The user must log in once online to populate
///   it (which is the documented bootstrap path -- offline login requires
///   a prior online round-trip per `.claude/rules/auth.md`).
/// - UPDATE path: `ON CONFLICT DO UPDATE SET ...` enumerates EVERY column
///   except `password_hash`, so a regression that started sending the hash
///   from the server (against §7.24) STILL cannot clobber the local one.
///
/// The standard LWW gate applies: stale `version` short-circuits the
/// update.
async fn apply_users_change(tx: &mut crate::db::Tx<'_>, change: &PullChange) -> AppResult<bool> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(false);
    }

    let row = sqlx::query_as::<_, (i64,)>("SELECT version FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    let incoming_version = change.version;
    if let Some((existing,)) = row {
        if incoming_version <= existing {
            return Ok(false);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO users ( \
            id, email, name, password_hash, role, is_active, last_login_at, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,'',?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            email = excluded.email, \
            name = excluded.name, \
            role = excluded.role, \
            is_active = excluded.is_active, \
            last_login_at = excluded.last_login_at, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE users.version < excluded.version \
           AND users.dirty = 0",
    )
    .bind(id)
    .bind(p.get("email").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("receptionist"),
    )
    .bind(p.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true) as i64)
    .bind(p.get("last_login_at").and_then(|v| v.as_str()))
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
    Ok(true)
}
