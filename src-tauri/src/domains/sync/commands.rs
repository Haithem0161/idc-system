//! Tauri commands exposed by the sync bounded context.

use serde::Serialize;
use tauri::State;
use tracing::instrument;

use crate::domains::sync::domain::value_objects::SyncStatus;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatusSnapshot {
    pub status: SyncStatus,
    pub pending_ops: u32,
    /// Ops that can no longer make progress on their own (parked after a
    /// server rejection or attempts-capped). Surfaced so stranded work is
    /// visible instead of silently lost; recover via `sync_requeue_op`.
    pub stuck_ops: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StuckOp {
    pub op_id: String,
    pub entity: String,
    pub entity_id: String,
    pub attempts: i32,
    pub parked: bool,
    pub last_error: Option<String>,
    pub created_at: String,
}

// --- testable `_impl` helpers -------------------------------------------
//
// Each `#[tauri::command]` delegates to a plain async fn taking `&AppState`,
// so phase-01 §2.2 tests can drive every command without standing up a
// Tauri runtime / `State` wrapper.

pub async fn sync_status_impl(state: &AppState) -> AppResult<SyncStatusSnapshot> {
    let engine = state.sync_engine();
    let status = engine.status().await;
    let outbox = engine.outbox_repo();
    let pending_ops = outbox.pending_count().await?;
    let stuck_ops = outbox.stuck_count().await?;
    Ok(SyncStatusSnapshot {
        status,
        pending_ops,
        stuck_ops,
    })
}

pub async fn sync_outbox_count_impl(state: &AppState) -> AppResult<u32> {
    state.sync_engine().outbox_repo().pending_count().await
}

pub async fn sync_list_stuck_impl(state: &AppState) -> AppResult<Vec<StuckOp>> {
    let ops = state.sync_engine().outbox_repo().list_stuck().await?;
    Ok(ops
        .into_iter()
        .map(|op| StuckOp {
            op_id: op.op_id.to_string(),
            entity: op.entity,
            entity_id: op.entity_id,
            attempts: op.attempts,
            parked: op.parked,
            last_error: op.last_error,
            created_at: op.created_at.to_rfc3339(),
        })
        .collect())
}

pub async fn sync_requeue_op_impl(state: &AppState, op_id: String) -> AppResult<()> {
    let parsed = uuid::Uuid::parse_str(&op_id)?;
    let affected = state
        .sync_engine()
        .outbox_repo()
        .requeue_stuck(parsed)
        .await?;
    if affected == 0 {
        return Err(crate::error::AppError::NotFound(format!(
            "no stuck op with id {op_id}"
        )));
    }
    // Kick a push so the requeued op is attempted promptly.
    state.sync_engine().trigger_push().await;
    Ok(())
}

pub async fn sync_trigger_push_impl(state: &AppState) -> AppResult<()> {
    state.sync_engine().trigger_push().await;
    Ok(())
}

pub async fn sync_trigger_pull_impl(state: &AppState) -> AppResult<()> {
    state.sync_engine().trigger_pull().await;
    Ok(())
}

pub async fn sync_list_conflicts_impl(
    state: &AppState,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<serde_json::Value>> {
    let conflicts = state.sync_engine().list_conflicts().await?;
    // The engine returns the full open-conflict set; honor the IPC pagination
    // by slicing here (skip(offset).take(limit)). Negative values clamp to 0;
    // a missing/zero limit means "no cap" so existing callers that pass
    // limit: 100 keep working and an unspecified call returns everything.
    let offset = offset.unwrap_or(0).max(0) as usize;
    let limit = limit.filter(|l| *l > 0).map(|l| l as usize);
    Ok(conflicts
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "opId": c.op_id,
                "entity": c.entity,
                "entityId": c.entity_id,
                "serverPayload": c.server_payload,
                "localPayload": c.local_payload,
                "reason": c.reason,
            })
        })
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect())
}

pub async fn sync_resolve_conflict_impl(
    state: &AppState,
    args: ResolveConflictArgs,
) -> AppResult<()> {
    if args.choice != "local" && args.choice != "server" && args.choice != "merged" {
        return Err(crate::error::AppError::Validation(format!(
            "invalid choice: {}",
            args.choice
        )));
    }
    state
        .sync_engine()
        .resolve_conflict(args.op_id, args.choice, args.merged)
        .await
}

pub async fn device_info_impl(state: &AppState) -> AppResult<DeviceInfo> {
    Ok(DeviceInfo {
        device_id: state.device_id().to_string(),
        app_version: state.app_version().to_string(),
    })
}

/// Read-only snapshot of the sync-engine's persisted timing state, surfaced to
/// the sync dashboard so the user can see when this device last shipped and
/// received changes without digging through logs. All timestamps are RFC3339
/// UTC; `None` means the corresponding direction has not run yet on this
/// device (fresh install, or push/pull never succeeded).
#[derive(Debug, Clone, Serialize)]
pub struct SyncTiming {
    pub last_pushed_at: Option<String>,
    pub last_pulled_at: Option<String>,
    pub pull_cursor: Option<String>,
    pub device_id: String,
    pub server_url: Option<String>,
    pub app_version: String,
}

pub async fn sync_last_synced_impl(state: &AppState) -> AppResult<SyncTiming> {
    let sync_state = state.sync_engine().state_repo().get().await?;
    Ok(SyncTiming {
        last_pushed_at: sync_state.last_pushed_at.map(|t| t.to_rfc3339()),
        last_pulled_at: sync_state.last_pulled_at.map(|t| t.to_rfc3339()),
        pull_cursor: sync_state.pull_cursor,
        device_id: sync_state.device_id,
        server_url: state.sync_server_url().await,
        app_version: state.app_version().to_string(),
    })
}

/// Validate and normalize a sync server URL. Accepts only absolute http(s)
/// URLs with a host; trims a trailing slash so persisted values are stable.
/// Pure (no I/O) so it can be unit-tested and reused by both the bootstrap and
/// the superadmin update paths.
pub fn validate_sync_server_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(crate::error::AppError::Validation(
            "sync server url required".into(),
        ));
    }
    // Whitespace anywhere in a URL is always invalid and usually a paste error.
    // Checked first so the host extraction below can ignore spacing.
    if trimmed.chars().any(char::is_whitespace) {
        return Err(crate::error::AppError::Validation(
            "sync server url must not contain spaces".into(),
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(|| {
            crate::error::AppError::Validation(
                "sync server url must start with http:// or https://".into(),
            )
        })?;
    // Reject schemes with no host (e.g. "https://", "http:///path"). The host
    // is everything before the first '/', '?', or '#'.
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return Err(crate::error::AppError::Validation(
            "sync server url is missing a host".into(),
        ));
    }
    // Normalize: drop a single trailing slash so the stored value is canonical.
    Ok(trimmed.trim_end_matches('/').to_string())
}

pub async fn config_set_sync_server_url_impl(state: &AppState, url: String) -> AppResult<()> {
    let url = validate_sync_server_url(&url)?;
    // Persist FIRST so a crash between writes can never leave the engine
    // pointing at a URL the DB has forgotten. The setter is also called by
    // the first-launch modal and the superadmin first-run wizard; without
    // persistence the modal reopened on every restart.
    state
        .sync_engine()
        .state_repo()
        .put_server_url(&url)
        .await?;
    state.set_sync_server_url(url.clone()).await;
    state.sync_engine().set_server_url(url).await;
    Ok(())
}

pub async fn config_get_sync_server_url_impl(state: &AppState) -> AppResult<Option<String>> {
    Ok(state.sync_server_url().await)
}

/// Summary of a `sync_resync_local` sweep: how many outbox ops were enqueued
/// per entity table, and the grand total.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResyncSummary {
    /// `(entity_table, ops_enqueued)` in FK-dependency (APPLY_ORDER) order.
    pub per_entity: Vec<(String, u64)>,
    pub total: u64,
}

/// Re-enqueue EVERY syncable local row into the outbox for a full re-push.
///
/// # Why this exists
/// The normal write path only enqueues an outbox op at the moment a row is
/// mutated. Once a row is pushed and marked `dirty = 0` there is no mechanism
/// that ever re-derives an outbox op for it. If the server subsequently loses
/// those already-synced rows, the client has no way to replay them -- the
/// outbox is empty and nothing sweeps clean rows back into it. This command is
/// that sweep: it walks every syncable table (including `dirty = 0` and
/// tombstoned rows, across ALL tenants) and enqueues one fresh upsert op per
/// row, using the SAME push-payload serializers the write path uses, so the
/// wire format is identical.
///
/// # Idempotency
/// Each call mints fresh `op_id`s (UUIDv7) for every row. Re-running is safe:
/// the server dedupes by `op_id` AND upserts by row `id`, so a duplicate local
/// op for the same row just re-applies the same upsert. We deliberately do NOT
/// dedupe against existing outbox ops -- a full resync is the whole point.
///
/// # Ordering
/// Ops are enqueued strictly in `APPLY_ORDER` (parents before children) so the
/// push loop -- which drains the outbox in `created_at`/`op_id` (creation)
/// order -- lands a parent row (e.g. a `patients` row) on the server before any
/// child (e.g. a `visits` row) that references it.
///
/// # Coverage
/// Covers all 17 syncable entities: users, settings, check_types,
/// check_subtypes, doctors, doctor_check_pricing, operators,
/// operator_specialties, mandoubs, inventory_items, inventory_consumption_map,
/// operator_shifts, patients, visits, inventory_adjustments, daily_close,
/// audit_log. `audit_log` uses its MessagePack `encode_audit_payload`; every
/// other entity uses `serde_json::to_vec(&XPushPayload::from(&row))`, matching
/// the write path byte-for-byte.
pub async fn sync_resync_local_impl(state: &AppState) -> AppResult<ResyncSummary> {
    use crate::domains::sync::domain::entities::OutboxOp;

    let pool = state
        .db_pool()
        .ok_or_else(|| crate::error::AppError::Internal("db pool not initialised".into()))?;
    let outbox = state.sync_engine().outbox_repo();

    // Gather all outbox ops FIRST (each list_all_for_resync is a read-only
    // query), grouped per entity in APPLY_ORDER, so the write transaction below
    // only does inserts and stays short. Holding a long tx across all these
    // reads would needlessly lengthen the WAL write lock.
    let mut per_entity: Vec<(String, Vec<OutboxOp>)> = Vec::new();

    // --- users -----------------------------------------------------------
    if let Some(user_repo) = state.user_repo() {
        let mut ops = Vec::new();
        for u in user_repo.list_all_for_resync().await? {
            // include_hash = true: the create path pushes the hash so the
            // server can serve auth; a resync must restore it too.
            let payload = serde_json::to_vec(
                &crate::domains::auth::user_service::to_push_payload(&u, true),
            )?;
            ops.push(OutboxOp::new("users", u.id.to_string(), payload));
        }
        per_entity.push(("users".into(), ops));
    }

    // --- settings --------------------------------------------------------
    if let Some(settings) = state.settings_service() {
        per_entity.push(("settings".into(), settings.resync_ops().await?));
    }

    // --- catalog (check_types .. inventory_consumption_map) --------------
    if let Some(catalog) = state.catalog_services() {
        use crate::domains::catalog::service::push_payloads::{
            CheckSubtypePushPayload, CheckTypePushPayload, ConsumptionPushPayload,
            DoctorPricingPushPayload, DoctorPushPayload, InventoryItemPushPayload,
            MandoubPushPayload, OperatorPushPayload, OperatorSpecialtyPushPayload,
        };

        let mut ct = Vec::new();
        for row in catalog.check_type_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&CheckTypePushPayload::from(&row))?;
            ct.push(OutboxOp::new("check_types", row.id.to_string(), payload));
        }
        per_entity.push(("check_types".into(), ct));

        let mut cs = Vec::new();
        for row in catalog.check_subtype_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&CheckSubtypePushPayload::from(&row))?;
            cs.push(OutboxOp::new("check_subtypes", row.id.to_string(), payload));
        }
        per_entity.push(("check_subtypes".into(), cs));

        let mut d = Vec::new();
        for row in catalog.doctor_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&DoctorPushPayload::from(&row))?;
            d.push(OutboxOp::new("doctors", row.id.to_string(), payload));
        }
        per_entity.push(("doctors".into(), d));

        let mut dp = Vec::new();
        for row in catalog.doctor_pricing_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&DoctorPricingPushPayload::from(&row))?;
            dp.push(OutboxOp::new(
                "doctor_check_pricing",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("doctor_check_pricing".into(), dp));

        let mut op = Vec::new();
        for row in catalog.operator_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&OperatorPushPayload::from(&row))?;
            op.push(OutboxOp::new("operators", row.id.to_string(), payload));
        }
        per_entity.push(("operators".into(), op));

        let mut os = Vec::new();
        for row in catalog
            .operator_specialty_repo
            .list_all_for_resync()
            .await?
        {
            let payload = serde_json::to_vec(&OperatorSpecialtyPushPayload::from(&row))?;
            os.push(OutboxOp::new(
                "operator_specialties",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("operator_specialties".into(), os));

        let mut m = Vec::new();
        for row in catalog.mandoub_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&MandoubPushPayload::from(&row))?;
            m.push(OutboxOp::new("mandoubs", row.id.to_string(), payload));
        }
        per_entity.push(("mandoubs".into(), m));

        let mut it = Vec::new();
        for row in catalog.inventory_item_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&InventoryItemPushPayload::from(&row))?;
            it.push(OutboxOp::new(
                "inventory_items",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("inventory_items".into(), it));

        let mut cm = Vec::new();
        for row in catalog.consumption_repo.list_all_for_resync().await? {
            let payload = serde_json::to_vec(&ConsumptionPushPayload::from(&row))?;
            cm.push(OutboxOp::new(
                "inventory_consumption_map",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("inventory_consumption_map".into(), cm));
    }

    // --- operator_shifts (reachable via the visit service) ---------------
    if let Some(visits) = state.visit_service() {
        use crate::domains::shifts::service::OperatorShiftPushPayload;
        let mut sh = Vec::new();
        for row in visits.shifts_repo().list_all_for_resync().await? {
            let payload = serde_json::to_vec(&OperatorShiftPushPayload::from(&row))?;
            sh.push(OutboxOp::new(
                "operator_shifts",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("operator_shifts".into(), sh));
    }

    // --- patients --------------------------------------------------------
    if let Some(patients) = state.patient_service() {
        use crate::domains::patients::service::push_payloads::PatientPushPayload;
        let mut p = Vec::new();
        for row in patients.repo().list_all_for_resync().await? {
            let payload = serde_json::to_vec(&PatientPushPayload::from(&row))?;
            p.push(OutboxOp::new("patients", row.id.to_string(), payload));
        }
        per_entity.push(("patients".into(), p));
    }

    // --- visits + inventory_adjustments ----------------------------------
    if let Some(visits) = state.visit_service() {
        use crate::domains::visits::service::push_payloads::{
            InventoryAdjustmentPushPayload, VisitPushPayload,
        };
        let mut v = Vec::new();
        for row in visits.visits_repo().list_all_for_resync().await? {
            let payload = serde_json::to_vec(&VisitPushPayload::from(&row))?;
            v.push(OutboxOp::new("visits", row.id.to_string(), payload));
        }
        per_entity.push(("visits".into(), v));

        let mut a = Vec::new();
        for row in visits.adjustments_repo().list_all_for_resync().await? {
            let payload = serde_json::to_vec(&InventoryAdjustmentPushPayload::from(&row))?;
            a.push(OutboxOp::new(
                "inventory_adjustments",
                row.id.to_string(),
                payload,
            ));
        }
        per_entity.push(("inventory_adjustments".into(), a));
    }

    // --- daily_close (frozen close) --------------------------------------
    if let Some(reports) = state.reports_service() {
        use crate::domains::reports::service::FrozenClosePushPayload;
        let mut dc = Vec::new();
        for row in reports.frozen_close_repo().list_all_for_resync().await? {
            let payload = serde_json::to_vec(&FrozenClosePushPayload::from(&row))?;
            dc.push(OutboxOp::new("daily_close", row.id.to_string(), payload));
        }
        per_entity.push(("daily_close".into(), dc));
    }

    // --- audit_log (additive-only; MessagePack payload) ------------------
    if let Some(audit) = state.audit_query_service() {
        use crate::domains::sync::domain::services::encode_audit_payload;
        let mut al = Vec::new();
        for row in audit.audit_repo().list_all_for_resync().await? {
            let payload = encode_audit_payload(&row)?;
            al.push(OutboxOp::new("audit_log", row.id.to_string(), payload));
        }
        per_entity.push(("audit_log".into(), al));
    }

    // Enqueue everything in APPLY_ORDER, in a single transaction, so the whole
    // sweep is atomic: either every op lands or none do (a partial outbox from
    // a crashed sweep could push children before parents).
    per_entity.sort_by_key(|(entity, _)| resync_apply_rank(entity));

    let mut tx = pool.begin().await.map_err(crate::error::AppError::from)?;
    let mut summary = Vec::with_capacity(per_entity.len());
    let mut total: u64 = 0;
    for (entity, ops) in &per_entity {
        for op in ops {
            outbox.enqueue(&mut tx, op).await?;
        }
        let count = ops.len() as u64;
        total += count;
        summary.push((entity.clone(), count));
    }
    tx.commit().await.map_err(crate::error::AppError::from)?;

    tracing::info!(
        total,
        "sync_resync_local: re-enqueued local rows for full re-push"
    );

    // Kick a push so the re-enqueued ops drain promptly.
    state.sync_engine().trigger_push().await;

    Ok(ResyncSummary {
        per_entity: summary,
        total,
    })
}

/// FK-dependency rank for resync enqueue ordering. Mirrors the puller's
/// `APPLY_ORDER` so parents are enqueued (and therefore pushed) before their
/// children. An unlisted entity sorts last.
fn resync_apply_rank(entity: &str) -> usize {
    const APPLY_ORDER: &[&str] = &[
        "users",
        "settings",
        "check_types",
        "check_subtypes",
        "doctors",
        "doctor_check_pricing",
        "operators",
        "operator_specialties",
        "mandoubs",
        "inventory_items",
        "inventory_consumption_map",
        "operator_shifts",
        "patients",
        "visits",
        "inventory_adjustments",
        "daily_close",
        "audit_log",
    ];
    APPLY_ORDER
        .iter()
        .position(|e| *e == entity)
        .unwrap_or(APPLY_ORDER.len())
}

// --- #[tauri::command] wrappers (boundary layer) ------------------------

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_status(state: State<'_, AppState>) -> AppResult<SyncStatusSnapshot> {
    sync_status_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_outbox_count(state: State<'_, AppState>) -> AppResult<u32> {
    sync_outbox_count_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_push(state: State<'_, AppState>) -> AppResult<()> {
    sync_trigger_push_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_list_stuck(state: State<'_, AppState>) -> AppResult<Vec<StuckOp>> {
    sync_list_stuck_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_requeue_op(state: State<'_, AppState>, op_id: String) -> AppResult<()> {
    sync_requeue_op_impl(&state, op_id).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_trigger_pull(state: State<'_, AppState>) -> AppResult<()> {
    sync_trigger_pull_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_list_conflicts(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<serde_json::Value>> {
    sync_list_conflicts_impl(&state, limit, offset).await
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveConflictArgs {
    // The frontend sends this inner struct under the `args` key as camelCase
    // (`opId`). Tauri v2 only camelCase-converts TOP-LEVEL command params, not
    // inner struct fields, so without rename_all serde fails with
    // "missing field `op_id`" and every conflict resolution is rejected.
    pub op_id: String,
    pub choice: String,
    #[serde(default)]
    pub merged: Option<serde_json::Value>,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn sync_resolve_conflict(
    state: State<'_, AppState>,
    args: ResolveConflictArgs,
) -> AppResult<()> {
    sync_resolve_conflict_impl(&state, args).await
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub app_version: String,
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn device_info(state: State<'_, AppState>) -> AppResult<DeviceInfo> {
    device_info_impl(&state).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_set_sync_server_url(state: State<'_, AppState>, url: String) -> AppResult<()> {
    config_set_sync_server_url_impl(&state, url).await
}

/// Superadmin-gated update of the sync server URL from the Settings screen.
///
/// `config_set_sync_server_url` (above) is the PRE-LOGIN bootstrap path used by
/// the first-launch modal and first-run wizard, when no user exists yet, so it
/// cannot be gated. Once a clinic is set up, repointing the sync server decides
/// which server the app trusts for auth and where all PHI is pushed -- a
/// security-relevant change that must be restricted to a superadmin (matching
/// the settings invariant). This wrapper requires an authenticated superadmin
/// and then reuses the same validated setter.
pub async fn config_update_sync_server_url_impl(state: &AppState, url: String) -> AppResult<()> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(crate::error::AppError::NotAuthenticated)?;
    let role = crate::domains::auth::domain::value_objects::UserRole::parse(&ctx.role)
        .ok_or_else(|| crate::error::AppError::Validation("invalid actor role".into()))?;
    if role != crate::domains::auth::domain::value_objects::UserRole::Superadmin {
        return Err(crate::error::AppError::Validation(
            "changing the sync server url is superadmin-only".into(),
        ));
    }
    config_set_sync_server_url_impl(state, url).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_update_sync_server_url(
    state: State<'_, AppState>,
    url: String,
) -> AppResult<()> {
    config_update_sync_server_url_impl(&state, url).await
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn config_get_sync_server_url(state: State<'_, AppState>) -> AppResult<Option<String>> {
    config_get_sync_server_url_impl(&state).await
}

/// Re-enqueue every syncable local row for a full re-push (recovery from a
/// server that lost already-synced rows). Not role-gated: like the other
/// `sync_*` commands it only re-pushes local data this device already owns.
#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_resync_local(state: State<'_, AppState>) -> AppResult<ResyncSummary> {
    sync_resync_local_impl(&state).await
}

/// Read-only timing snapshot for the sync dashboard (last push/pull, cursor,
/// device id, server url, app version). Not role-gated: it exposes only this
/// device's own sync bookkeeping, no other tenant's data.
#[tauri::command]
#[instrument(skip(state))]
pub async fn sync_last_synced(state: State<'_, AppState>) -> AppResult<SyncTiming> {
    sync_last_synced_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::validate_sync_server_url;

    #[test]
    fn accepts_https_and_http_and_normalizes_trailing_slash() {
        assert_eq!(
            validate_sync_server_url("https://idc-sync.example.com").unwrap(),
            "https://idc-sync.example.com"
        );
        assert_eq!(
            validate_sync_server_url("  https://idc-sync.example.com/  ").unwrap(),
            "https://idc-sync.example.com"
        );
        assert_eq!(
            validate_sync_server_url("http://192.168.1.10:3161").unwrap(),
            "http://192.168.1.10:3161"
        );
        // A path is preserved (only a single trailing slash is trimmed).
        assert_eq!(
            validate_sync_server_url("https://h.example.com/api/").unwrap(),
            "https://h.example.com/api"
        );
    }

    #[test]
    fn rejects_empty_missing_scheme_missing_host_and_whitespace() {
        assert!(validate_sync_server_url("").is_err());
        assert!(validate_sync_server_url("   ").is_err());
        assert!(validate_sync_server_url("idc-sync.example.com").is_err()); // no scheme
        assert!(validate_sync_server_url("ftp://h.example.com").is_err()); // wrong scheme
        assert!(validate_sync_server_url("https://").is_err()); // no host
        assert!(validate_sync_server_url("http:///path").is_err()); // no host
        assert!(validate_sync_server_url("https://has space.example.com").is_err());
    }
}
