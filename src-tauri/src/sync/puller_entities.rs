//! Pull-apply handlers for the remaining syncable entities (C3/C5).
//!
//! Two policy shapes live here (declared in [`crate::sync::conflict::policy_for`]):
//!
//! - **LastWriteWins** (catalog, patients, shifts, etc.): the incoming row is
//!   applied via `INSERT ... ON CONFLICT DO UPDATE ... WHERE <t>.version <
//!   excluded.version AND <t>.dirty = 0`. The `WHERE` clause is the *sole*
//!   authoritative gate -- there is intentionally no Rust-level `SELECT version`
//!   pre-read. Reading the version in Rust and then issuing the upsert opened a
//!   window where a concurrent local mutation (separate pool connection) could
//!   flip `dirty`/bump `version` between the read and the conflict evaluation,
//!   silently clobbering the local edit (phase-10 T2). Letting SQLite evaluate
//!   the gate atomically closes that race; a `rows_affected() == 0` result means
//!   "stale or locally dirty" and is the correct silent skip.
//!
//! - **Manual** (`settings`, `visits`): a server row must NEVER overwrite an
//!   unsynced local edit, because settings drive money math and visits are
//!   financial records (phase-10 T1). On pull we first check whether the local
//!   row is `dirty = 1` (has unpushed local edits). If so we skip applying the
//!   server row entirely and leave the local edit intact; the dirty row pushes
//!   on the next push cycle, where the server's `detectSettingConflict` /
//!   `detectVisitConflict` parks the divergence and surfaces it through the
//!   conflict resolver UI (the single source of truth for parked conflicts).
//!   When the local row is clean we fast-forward via the same version gate.

use crate::domains::sync::infrastructure::PullChange;
use crate::error::AppResult;

/// Whether the local row for `id` in `table` currently has unpushed local
/// edits (`dirty = 1`). Used by the Manual-policy handlers to refuse to
/// overwrite a divergent local edit on pull. `table` is a fixed string literal
/// supplied by the caller (never user input), so the `format!` is safe.
async fn local_row_is_dirty(tx: &mut crate::db::Tx<'_>, table: &str, id: &str) -> AppResult<bool> {
    let row = sqlx::query_as::<_, (i64,)>(&format!("SELECT dirty FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(matches!(row, Some((1,))))
}

pub(crate) async fn apply_settings_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // Manual policy (phase-10 T1): never overwrite an unsynced local edit. A
    // dirty local row diverges from the incoming server row; skip the apply and
    // let the next push surface the conflict server-side.
    if local_row_is_dirty(tx, "settings", id).await? {
        return Ok(());
    }
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO settings ( \
            id, key, value, value_type, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            key = excluded.key, \
            value = excluded.value, \
            value_type = excluded.value_type, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE settings.version < excluded.version \
           AND settings.dirty = 0",
    )
    .bind(id)
    .bind(p.get("key").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("value").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("value_type").and_then(|v| v.as_str()).unwrap_or(""))
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
    Ok(())
}

pub(crate) async fn apply_check_types_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: the SQL WHERE gate below is the sole authoritative check (phase-10
    // T2). No Rust-level version pre-read -- that opened a clobber race.
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO check_types ( \
            id, name_ar, name_en, has_subtypes, base_price_iqd, \
            dye_supported, report_supported, sort_order, is_active, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name_ar = excluded.name_ar, \
            name_en = excluded.name_en, \
            has_subtypes = excluded.has_subtypes, \
            base_price_iqd = excluded.base_price_iqd, \
            dye_supported = excluded.dye_supported, \
            report_supported = excluded.report_supported, \
            sort_order = excluded.sort_order, \
            is_active = excluded.is_active, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE check_types.version < excluded.version \
           AND check_types.dirty = 0",
    )
    .bind(id)
    .bind(p.get("name_ar").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("name_en").and_then(|v| v.as_str()))
    .bind(
        p.get("has_subtypes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64,
    )
    .bind(p.get("base_price_iqd").and_then(|v| v.as_i64()))
    .bind(
        p.get("dye_supported")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64,
    )
    .bind(
        p.get("report_supported")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64,
    )
    .bind(p.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0))
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
    Ok(())
}

pub(crate) async fn apply_check_subtypes_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO check_subtypes ( \
            id, check_type_id, name_ar, name_en, price_iqd, sort_order, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            check_type_id = excluded.check_type_id, \
            name_ar = excluded.name_ar, \
            name_en = excluded.name_en, \
            price_iqd = excluded.price_iqd, \
            sort_order = excluded.sort_order, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE check_subtypes.version < excluded.version \
           AND check_subtypes.dirty = 0",
    )
    .bind(id)
    .bind(
        p.get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("name_ar").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("name_en").and_then(|v| v.as_str()))
    .bind(p.get("price_iqd").and_then(|v| v.as_i64()).unwrap_or(0))
    .bind(p.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0))
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
    Ok(())
}

pub(crate) async fn apply_doctors_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO doctors ( \
            id, name, specialty, phone, is_active, notes, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name = excluded.name, \
            specialty = excluded.specialty, \
            phone = excluded.phone, \
            is_active = excluded.is_active, \
            notes = excluded.notes, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE doctors.version < excluded.version \
           AND doctors.dirty = 0",
    )
    .bind(id)
    .bind(p.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("specialty").and_then(|v| v.as_str()))
    .bind(p.get("phone").and_then(|v| v.as_str()))
    .bind(p.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true) as i64)
    .bind(p.get("notes").and_then(|v| v.as_str()))
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
    Ok(())
}

pub(crate) async fn apply_doctor_check_pricing_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO doctor_check_pricing ( \
            id, doctor_id, check_type_id, check_subtype_id, \
            price_override_iqd, cut_kind, cut_value, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            doctor_id = excluded.doctor_id, \
            check_type_id = excluded.check_type_id, \
            check_subtype_id = excluded.check_subtype_id, \
            price_override_iqd = excluded.price_override_iqd, \
            cut_kind = excluded.cut_kind, \
            cut_value = excluded.cut_value, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE doctor_check_pricing.version < excluded.version \
           AND doctor_check_pricing.dirty = 0",
    )
    .bind(id)
    .bind(p.get("doctor_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("check_subtype_id").and_then(|v| v.as_str()))
    .bind(p.get("price_override_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("cut_kind").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("cut_value").and_then(|v| v.as_i64()).unwrap_or(0))
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
    Ok(())
}

pub(crate) async fn apply_operators_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO operators ( \
            id, name, phone, base_cut_per_check_iqd, is_active, notes, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name = excluded.name, \
            phone = excluded.phone, \
            base_cut_per_check_iqd = excluded.base_cut_per_check_iqd, \
            is_active = excluded.is_active, \
            notes = excluded.notes, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE operators.version < excluded.version \
           AND operators.dirty = 0",
    )
    .bind(id)
    .bind(p.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("phone").and_then(|v| v.as_str()))
    .bind(
        p.get("base_cut_per_check_iqd")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    )
    .bind(p.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true) as i64)
    .bind(p.get("notes").and_then(|v| v.as_str()))
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
    Ok(())
}

pub(crate) async fn apply_operator_specialties_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO operator_specialties ( \
            id, operator_id, check_type_id, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            operator_id = excluded.operator_id, \
            check_type_id = excluded.check_type_id, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE operator_specialties.version < excluded.version \
           AND operator_specialties.dirty = 0",
    )
    .bind(id)
    .bind(p.get("operator_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
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
    .bind(incoming_version)
    .bind(now)
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn apply_inventory_consumption_map_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO inventory_consumption_map ( \
            id, check_type_id, check_subtype_id, item_id, \
            quantity_per_check, on_dye_only, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            check_type_id = excluded.check_type_id, \
            check_subtype_id = excluded.check_subtype_id, \
            item_id = excluded.item_id, \
            quantity_per_check = excluded.quantity_per_check, \
            on_dye_only = excluded.on_dye_only, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE inventory_consumption_map.version < excluded.version \
           AND inventory_consumption_map.dirty = 0",
    )
    .bind(id)
    .bind(
        p.get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("check_subtype_id").and_then(|v| v.as_str()))
    .bind(p.get("item_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("quantity_per_check")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    )
    .bind(
        p.get("on_dye_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64,
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
    .bind(incoming_version)
    .bind(now)
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn apply_operator_shifts_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO operator_shifts ( \
            id, operator_id, check_in_at, check_out_at, \
            check_in_by_user_id, check_out_by_user_id, note, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            operator_id = excluded.operator_id, \
            check_in_at = excluded.check_in_at, \
            check_out_at = excluded.check_out_at, \
            check_in_by_user_id = excluded.check_in_by_user_id, \
            check_out_by_user_id = excluded.check_out_by_user_id, \
            note = excluded.note, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE operator_shifts.version < excluded.version \
           AND operator_shifts.dirty = 0",
    )
    .bind(id)
    .bind(p.get("operator_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("check_in_at").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("check_out_at").and_then(|v| v.as_str()))
    .bind(
        p.get("check_in_by_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("check_out_by_user_id").and_then(|v| v.as_str()))
    .bind(p.get("note").and_then(|v| v.as_str()))
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
    Ok(())
}

pub(crate) async fn apply_patients_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (phase-10 T2).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO patients ( \
            id, name, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name = excluded.name, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE patients.version < excluded.version \
           AND patients.dirty = 0",
    )
    .bind(id)
    .bind(p.get("name").and_then(|v| v.as_str()).unwrap_or(""))
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
    Ok(())
}

pub(crate) async fn apply_visits_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // Manual policy (phase-10 T1): never overwrite an unsynced local edit. A
    // dirty local visit diverges from the incoming server row; skip the apply
    // and let the next push surface the conflict server-side
    // (detectVisitConflict parks it for the resolver UI).
    if local_row_is_dirty(tx, "visits", id).await? {
        return Ok(());
    }
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO visits ( \
            id, patient_id, status, receptionist_user_id, check_type_id, \
            check_subtype_id, doctor_id, operator_id, dye, report, \
            locked_at, voided_at, voided_by_user_id, void_reason, \
            price_snapshot_iqd, dye_cost_snapshot_iqd, report_cost_snapshot_iqd, \
            doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, \
            internal_pct_snapshot, total_amount_iqd_snapshot, \
            patient_name_snapshot, doctor_name_snapshot, operator_name_snapshot, \
            check_type_name_ar_snapshot, check_type_name_en_snapshot, \
            check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            patient_id = excluded.patient_id, \
            status = excluded.status, \
            receptionist_user_id = excluded.receptionist_user_id, \
            check_type_id = excluded.check_type_id, \
            check_subtype_id = excluded.check_subtype_id, \
            doctor_id = excluded.doctor_id, \
            operator_id = excluded.operator_id, \
            dye = excluded.dye, \
            report = excluded.report, \
            locked_at = excluded.locked_at, \
            voided_at = excluded.voided_at, \
            voided_by_user_id = excluded.voided_by_user_id, \
            void_reason = excluded.void_reason, \
            price_snapshot_iqd = excluded.price_snapshot_iqd, \
            dye_cost_snapshot_iqd = excluded.dye_cost_snapshot_iqd, \
            report_cost_snapshot_iqd = excluded.report_cost_snapshot_iqd, \
            doctor_cut_snapshot_iqd = excluded.doctor_cut_snapshot_iqd, \
            operator_cut_snapshot_iqd = excluded.operator_cut_snapshot_iqd, \
            internal_pct_snapshot = excluded.internal_pct_snapshot, \
            total_amount_iqd_snapshot = excluded.total_amount_iqd_snapshot, \
            patient_name_snapshot = excluded.patient_name_snapshot, \
            doctor_name_snapshot = excluded.doctor_name_snapshot, \
            operator_name_snapshot = excluded.operator_name_snapshot, \
            check_type_name_ar_snapshot = excluded.check_type_name_ar_snapshot, \
            check_type_name_en_snapshot = excluded.check_type_name_en_snapshot, \
            check_subtype_name_ar_snapshot = excluded.check_subtype_name_ar_snapshot, \
            check_subtype_name_en_snapshot = excluded.check_subtype_name_en_snapshot, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id, \
            entity_id = excluded.entity_id \
         WHERE visits.version < excluded.version \
           AND visits.dirty = 0",
    )
    .bind(id)
    .bind(p.get("patient_id").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("status").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(
        p.get("receptionist_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(
        p.get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.get("check_subtype_id").and_then(|v| v.as_str()))
    .bind(p.get("doctor_id").and_then(|v| v.as_str()))
    .bind(p.get("operator_id").and_then(|v| v.as_str()))
    .bind(p.get("dye").and_then(|v| v.as_bool()).unwrap_or(false) as i64)
    .bind(p.get("report").and_then(|v| v.as_bool()).unwrap_or(false) as i64)
    .bind(p.get("locked_at").and_then(|v| v.as_str()))
    .bind(p.get("voided_at").and_then(|v| v.as_str()))
    .bind(p.get("voided_by_user_id").and_then(|v| v.as_str()))
    .bind(p.get("void_reason").and_then(|v| v.as_str()))
    .bind(p.get("price_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("dye_cost_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("report_cost_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("doctor_cut_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("operator_cut_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("internal_pct_snapshot").and_then(|v| v.as_i64()))
    .bind(p.get("total_amount_iqd_snapshot").and_then(|v| v.as_i64()))
    .bind(p.get("patient_name_snapshot").and_then(|v| v.as_str()))
    .bind(p.get("doctor_name_snapshot").and_then(|v| v.as_str()))
    .bind(p.get("operator_name_snapshot").and_then(|v| v.as_str()))
    .bind(
        p.get("check_type_name_ar_snapshot")
            .and_then(|v| v.as_str()),
    )
    .bind(
        p.get("check_type_name_en_snapshot")
            .and_then(|v| v.as_str()),
    )
    .bind(
        p.get("check_subtype_name_ar_snapshot")
            .and_then(|v| v.as_str()),
    )
    .bind(
        p.get("check_subtype_name_en_snapshot")
            .and_then(|v| v.as_str()),
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
    .bind(incoming_version)
    .bind(now)
    .bind(p.get("origin_device_id").and_then(|v| v.as_str()))
    .bind(p.get("entity_id").and_then(|v| v.as_str()).unwrap_or(""))
    .execute(&mut **tx)
    .await?;
    Ok(())
}
