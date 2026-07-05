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

/// Tombstone any *other* live row that would collide with an incoming live row
/// on a table's partial-unique secondary index, so the subsequent
/// `ON CONFLICT(id)` upsert cannot abort the whole pull transaction.
///
/// Background: every syncable table is keyed by `id` (UUID v7), but several also
/// carry a `UNIQUE(<business tuple>) WHERE deleted_at IS NULL` index. The server
/// upserts by `id` only and has no such constraint, so it can legitimately hold
/// two live rows with the SAME business tuple but DIFFERENT ids (delete+recreate
/// on one device, or independent creation on two). When both are pulled, the
/// second `INSERT` violates the local partial-unique index and aborts the pull
/// for ALL entities -- the cursor never advances and sync wedges permanently.
///
/// Fix (LWW tables): before inserting an incoming *live* row, soft-delete the
/// loser -- any *other* row that (a) shares the business tuple, (b) is itself
/// live (`deleted_at IS NULL`, so it actually occupies the partial index), and
/// (c) is NOT locally dirty (never clobber an unpushed local edit). The incoming
/// server row then inserts collision-free and becomes the single live row;
/// both ids survive (one tombstoned) so the convergence re-syncs cleanly.
///
/// `table` and `tuple_match` are fixed string literals supplied by the caller
/// (never user input). `tuple_binds` are the business-key values, bound in the
/// order their `?` placeholders appear in `tuple_match`.
pub(crate) async fn tombstone_unique_collision(
    tx: &mut crate::db::Tx<'_>,
    table: &str,
    keep_id: &str,
    tuple_match: &str,
    tuple_binds: &[&str],
    now: &str,
) -> AppResult<()> {
    // Binds in `?` order: deleted_at(now), updated_at(now), id(keep_id), then
    // the business-tuple values referenced by `tuple_match`.
    let sql = format!(
        "UPDATE {table} SET \
            deleted_at = ?, \
            updated_at = ?, \
            version = version + 1, \
            dirty = 1 \
         WHERE id != ? \
           AND deleted_at IS NULL \
           AND dirty = 0 \
           AND {tuple_match}"
    );
    let mut q = sqlx::query(&sql).bind(now).bind(now).bind(keep_id);
    for b in tuple_binds {
        q = q.bind(*b);
    }
    q.execute(&mut **tx).await?;
    Ok(())
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
    // Clear any OTHER clean live setting that shares the (entity_id, key) tuple
    // before insert (partial unique `settings_key`) so the pull can't abort. The
    // helper only touches `dirty = 0` rows, so an unsynced local edit is never
    // clobbered -- consistent with the Manual policy above.
    if p.get("deleted_at").and_then(|v| v.as_str()).is_none() {
        let entity_id = p.get("entity_id").and_then(|v| v.as_str()).unwrap_or("");
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        tombstone_unique_collision(
            tx,
            "settings",
            id,
            "entity_id = ? AND key = ?",
            &[entity_id, key],
            &now,
        )
        .await?;
    }
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
            dye_price_iqd, sort_order, is_active, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name_ar = excluded.name_ar, \
            name_en = excluded.name_en, \
            has_subtypes = excluded.has_subtypes, \
            base_price_iqd = excluded.base_price_iqd, \
            dye_price_iqd = excluded.dye_price_iqd, \
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
    .bind(p.get("dye_price_iqd").and_then(|v| v.as_i64()))
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
            id, check_type_id, name_ar, name_en, price_iqd, dye_price_iqd, sort_order, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            check_type_id = excluded.check_type_id, \
            name_ar = excluded.name_ar, \
            name_en = excluded.name_en, \
            price_iqd = excluded.price_iqd, \
            dye_price_iqd = excluded.dye_price_iqd, \
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
    .bind(p.get("dye_price_iqd").and_then(|v| v.as_i64()))
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
    // Clear any other live row sharing the (doctor, check_type, subtype) tuple
    // before insert (partial unique `doctor_check_pricing_unique`). The subtype
    // is nullable; the index keys on IFNULL(check_subtype_id,'') so we match the
    // same way.
    let incoming_deleted = p.get("deleted_at").and_then(|v| v.as_str());
    if incoming_deleted.is_none() {
        let doctor_id = p.get("doctor_id").and_then(|v| v.as_str()).unwrap_or("");
        let check_type_id = p
            .get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let subtype = p
            .get("check_subtype_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        tombstone_unique_collision(
            tx,
            "doctor_check_pricing",
            id,
            "doctor_id = ? AND check_type_id = ? AND IFNULL(check_subtype_id, '') = ?",
            &[doctor_id, check_type_id, subtype],
            &now,
        )
        .await?;
    }
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

pub(crate) async fn apply_mandoubs_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    // LWW: SQL WHERE gate is the sole authoritative check (mirrors operators).
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO mandoubs ( \
            id, name, phone, is_active, notes, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            name = excluded.name, \
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
         WHERE mandoubs.version < excluded.version \
           AND mandoubs.dirty = 0",
    )
    .bind(id)
    .bind(p.get("name").and_then(|v| v.as_str()).unwrap_or(""))
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
    // If the incoming row is live, clear any other live row sharing the
    // (operator_id, check_type_id) tuple so the insert can't trip the partial
    // unique index `operator_specialties_unique` and abort the pull.
    let incoming_deleted = p.get("deleted_at").and_then(|v| v.as_str());
    if incoming_deleted.is_none() {
        let operator_id = p.get("operator_id").and_then(|v| v.as_str()).unwrap_or("");
        let check_type_id = p
            .get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        tombstone_unique_collision(
            tx,
            "operator_specialties",
            id,
            "operator_id = ? AND check_type_id = ?",
            &[operator_id, check_type_id],
            &now,
        )
        .await?;
    }
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
    // Clear any other live row sharing the consumption-rule tuple before insert
    // (partial unique `inventory_consumption_unique`). `on_dye_only` is stored as
    // 0/1, so compare against the integer literal form of the incoming bool.
    let incoming_deleted = p.get("deleted_at").and_then(|v| v.as_str());
    if incoming_deleted.is_none() {
        let check_type_id = p
            .get("check_type_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let item_id = p.get("item_id").and_then(|v| v.as_str()).unwrap_or("");
        let subtype = p
            .get("check_subtype_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let on_dye_only = if p
            .get("on_dye_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            "1"
        } else {
            "0"
        };
        tombstone_unique_collision(
            tx,
            "inventory_consumption_map",
            id,
            "check_type_id = ? AND IFNULL(check_subtype_id, '') = ? AND item_id = ? AND on_dye_only = ?",
            &[check_type_id, subtype, item_id, on_dye_only],
            &now,
        )
        .await?;
    }
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
    // The partial unique index `operator_shifts_open` allows only ONE open shift
    // (check_out_at IS NULL, not deleted) per operator. If the incoming row is an
    // open, live shift, close any OTHER open, non-dirty shift for that operator
    // first so the insert cannot abort the pull. This is an additive ledger, so
    // we CLOSE the older shift (set check_out_at) rather than delete it -- the
    // historical row survives, it just leaves the partial index.
    let incoming_open = p.get("check_out_at").and_then(|v| v.as_str()).is_none();
    let incoming_live = p.get("deleted_at").and_then(|v| v.as_str()).is_none();
    if incoming_open && incoming_live {
        let operator_id = p.get("operator_id").and_then(|v| v.as_str()).unwrap_or("");
        sqlx::query(
            "UPDATE operator_shifts SET \
                check_out_at = ?, \
                updated_at = ?, \
                version = version + 1, \
                dirty = 1 \
             WHERE id != ? \
               AND operator_id = ? \
               AND check_out_at IS NULL \
               AND deleted_at IS NULL \
               AND dirty = 0",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .bind(operator_id)
        .execute(&mut **tx)
        .await?;
    }
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
            check_subtype_id, doctor_id, operator_id, mandoub_id, dye, report, dalal, \
            discount, \
            locked_at, voided_at, voided_by_user_id, void_reason, \
            price_snapshot_iqd, dye_cost_snapshot_iqd, report_amount_snapshot_iqd, \
            report_pct_snapshot, reporting_doctor_name_snapshot, \
            doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, \
            mandoub_cut_snapshot_iqd, mandoub_name_snapshot, \
            internal_pct_snapshot, total_amount_iqd_snapshot, amount_paid_override_iqd, \
            patient_name_snapshot, doctor_name_snapshot, operator_name_snapshot, \
            check_type_name_ar_snapshot, check_type_name_en_snapshot, \
            check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            patient_id = excluded.patient_id, \
            status = excluded.status, \
            receptionist_user_id = excluded.receptionist_user_id, \
            check_type_id = excluded.check_type_id, \
            check_subtype_id = excluded.check_subtype_id, \
            doctor_id = excluded.doctor_id, \
            operator_id = excluded.operator_id, \
            mandoub_id = excluded.mandoub_id, \
            dye = excluded.dye, \
            report = excluded.report, \
            dalal = excluded.dalal, \
            discount = excluded.discount, \
            locked_at = excluded.locked_at, \
            voided_at = excluded.voided_at, \
            voided_by_user_id = excluded.voided_by_user_id, \
            void_reason = excluded.void_reason, \
            price_snapshot_iqd = excluded.price_snapshot_iqd, \
            dye_cost_snapshot_iqd = excluded.dye_cost_snapshot_iqd, \
            report_amount_snapshot_iqd = excluded.report_amount_snapshot_iqd, \
            report_pct_snapshot = excluded.report_pct_snapshot, \
            reporting_doctor_name_snapshot = excluded.reporting_doctor_name_snapshot, \
            doctor_cut_snapshot_iqd = excluded.doctor_cut_snapshot_iqd, \
            operator_cut_snapshot_iqd = excluded.operator_cut_snapshot_iqd, \
            mandoub_cut_snapshot_iqd = excluded.mandoub_cut_snapshot_iqd, \
            mandoub_name_snapshot = excluded.mandoub_name_snapshot, \
            internal_pct_snapshot = excluded.internal_pct_snapshot, \
            total_amount_iqd_snapshot = excluded.total_amount_iqd_snapshot, \
            amount_paid_override_iqd = excluded.amount_paid_override_iqd, \
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
    .bind(p.get("mandoub_id").and_then(|v| v.as_str()))
    .bind(p.get("dye").and_then(|v| v.as_bool()).unwrap_or(false) as i64)
    .bind(p.get("report").and_then(|v| v.as_bool()).unwrap_or(false) as i64)
    .bind(
        // `dalal` may arrive as a JSON bool (desktop wire shape) or as an int
        // 0/1 (server column); accept both and default to 0.
        p.get("dalal")
            .map(|v| {
                v.as_bool()
                    .map(|b| b as i64)
                    .or_else(|| v.as_i64())
                    .unwrap_or(0)
            })
            .unwrap_or(0),
    )
    .bind(
        // `discount` shares the dalal wire-shape tolerance: bool (desktop) or
        // int 0/1 (server column). Defaults to 0 (no discount) when absent so a
        // pre-feature server payload converges cleanly.
        p.get("discount")
            .map(|v| {
                v.as_bool()
                    .map(|b| b as i64)
                    .or_else(|| v.as_i64())
                    .unwrap_or(0)
            })
            .unwrap_or(0),
    )
    .bind(p.get("locked_at").and_then(|v| v.as_str()))
    .bind(p.get("voided_at").and_then(|v| v.as_str()))
    .bind(p.get("voided_by_user_id").and_then(|v| v.as_str()))
    .bind(p.get("void_reason").and_then(|v| v.as_str()))
    .bind(p.get("price_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("dye_cost_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("report_amount_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("report_pct_snapshot").and_then(|v| v.as_i64()))
    .bind(p.get("reporting_doctor_name_snapshot").and_then(|v| v.as_str()))
    .bind(p.get("doctor_cut_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("operator_cut_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("mandoub_cut_snapshot_iqd").and_then(|v| v.as_i64()))
    .bind(p.get("mandoub_name_snapshot").and_then(|v| v.as_str()))
    .bind(p.get("internal_pct_snapshot").and_then(|v| v.as_i64()))
    .bind(p.get("total_amount_iqd_snapshot").and_then(|v| v.as_i64()))
    .bind(p.get("amount_paid_override_iqd").and_then(|v| v.as_i64()))
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

/// Apply a pulled `daily_close` (signed/frozen close). LWW via the atomic
/// version gate: the freeze is version 1, a superadmin reopen is version 2 of
/// the same id, so a peer device's reopen overwrites the local freeze cleanly.
/// The NOT NULL snapshot columns are always present in a well-formed payload;
/// missing optional reopen columns bind NULL.
pub(crate) async fn apply_daily_close_change(
    tx: &mut crate::db::Tx<'_>,
    change: &PullChange,
) -> AppResult<()> {
    let p = &change.payload;
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    let incoming_version = change.version;
    let now = chrono::Utc::now().to_rfc3339();
    let i64_field = |key: &str| p.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
    // The partial unique index `daily_close_active_per_day` allows only ONE
    // in-force close (reopened_at IS NULL, not deleted) per (entity_id,
    // target_date). If the incoming row is in force, retire any OTHER in-force,
    // non-dirty close for the same day before insert so the pull can't abort.
    // A duplicate freeze is being superseded, NOT reopened, so we set deleted_at
    // (it leaves the index but the historical freeze row survives) rather than
    // fabricate a `reopened_at` superadmin action.
    let incoming_in_force = p.get("reopened_at").and_then(|v| v.as_str()).is_none()
        && p.get("deleted_at").and_then(|v| v.as_str()).is_none();
    if incoming_in_force {
        let entity_id = p.get("entity_id").and_then(|v| v.as_str()).unwrap_or("");
        let target_date = p.get("target_date").and_then(|v| v.as_str()).unwrap_or("");
        sqlx::query(
            "UPDATE daily_close SET \
                deleted_at = ?, \
                updated_at = ?, \
                version = version + 1, \
                dirty = 1 \
             WHERE id != ? \
               AND entity_id = ? \
               AND target_date = ? \
               AND reopened_at IS NULL \
               AND deleted_at IS NULL \
               AND dirty = 0",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .bind(entity_id)
        .bind(target_date)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO daily_close ( \
            id, target_date, tz_offset, input_hash, \
            total_revenue_iqd, total_collected_iqd, total_discount_iqd, \
            total_doctor_cuts_iqd, total_operator_cuts_iqd, total_report_iqd, \
            total_inventory_consumption_value_iqd, net_iqd, locked_count, \
            voided_count, voided_value_iqd, \
            signed_by_user_id, signed_by_name, signed_at, \
            reopened_at, reopened_by_user_id, reopen_reason, \
            created_at, updated_at, deleted_at, version, dirty, \
            last_synced_at, origin_device_id, entity_id \
         ) VALUES (?,?,?,?, ?,?,?,?,?,?, ?,?,?, ?,?, ?,?,?, ?,?,?, ?,?,?,?,0, ?,?,?) \
         ON CONFLICT(id) DO UPDATE SET \
            reopened_at = excluded.reopened_at, \
            reopened_by_user_id = excluded.reopened_by_user_id, \
            reopen_reason = excluded.reopen_reason, \
            updated_at = excluded.updated_at, \
            deleted_at = excluded.deleted_at, \
            version = excluded.version, \
            dirty = 0, \
            last_synced_at = excluded.last_synced_at, \
            origin_device_id = excluded.origin_device_id \
         WHERE daily_close.version < excluded.version \
           AND daily_close.dirty = 0",
    )
    .bind(id)
    .bind(p.get("target_date").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("tz_offset").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(p.get("input_hash").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(i64_field("total_revenue_iqd"))
    .bind(i64_field("total_collected_iqd"))
    .bind(i64_field("total_discount_iqd"))
    .bind(i64_field("total_doctor_cuts_iqd"))
    .bind(i64_field("total_operator_cuts_iqd"))
    .bind(i64_field("total_report_iqd"))
    .bind(i64_field("total_inventory_consumption_value_iqd"))
    .bind(i64_field("net_iqd"))
    .bind(i64_field("locked_count"))
    .bind(i64_field("voided_count"))
    .bind(i64_field("voided_value_iqd"))
    .bind(
        p.get("signed_by_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(
        p.get("signed_by_name")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(
        p.get("signed_at")
            .and_then(|v| v.as_str())
            .unwrap_or(&change.updated_at),
    )
    .bind(p.get("reopened_at").and_then(|v| v.as_str()))
    .bind(p.get("reopened_by_user_id").and_then(|v| v.as_str()))
    .bind(p.get("reopen_reason").and_then(|v| v.as_str()))
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

#[cfg(test)]
mod secondary_unique_collision_tests {
    //! Regression tests for the pull-abort bug: the server can hold two LIVE
    //! rows that share a business-key tuple but have different ids (the server
    //! has no `WHERE deleted_at IS NULL` partial unique). When both are pulled,
    //! the second INSERT used to trip the local partial-unique index and abort
    //! the ENTIRE pull transaction. The handlers now clear the colliding live
    //! row first, so the pull applies cleanly and converges.

    use super::*;
    use crate::db::sqlite::init_pool_in_memory;
    use crate::domains::sync::infrastructure::PullChange;
    use sqlx::Row;

    const E: &str = "tenant-1";

    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = init_pool_in_memory().await.unwrap();
        crate::db::migrations::run(&pool).await.unwrap();
        pool
    }

    fn change(entity: &str, payload: serde_json::Value, version: i64) -> PullChange {
        PullChange {
            entity: entity.into(),
            entity_id: payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            payload,
            updated_at: "2026-06-26T10:00:00Z".into(),
            version,
        }
    }

    async fn seed_operator(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO operators (id, name, base_cut_per_check_iqd, is_active, \
             created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, 'Op', 0, 1, '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_check_type(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO check_types (id, name_ar, has_subtypes, base_price_iqd, \
             dye_price_iqd, sort_order, is_active, \
             created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, 'فحص', 0, 1000, NULL, 0, 1, \
             '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_user(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, is_active, \
             created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, 'U', '', 'receptionist', 1, \
             '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(format!("{id}@e.test"))
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    /// A pulled live operator_specialty that shares (operator, check_type) with
    /// an existing live row (different id) must NOT abort the pull; the incoming
    /// row wins and the old row is tombstoned -- exactly one live row remains.
    #[tokio::test]
    async fn operator_specialties_collision_tombstones_loser_instead_of_aborting() {
        let pool = migrated_pool().await;
        let op = "0190a000-0000-7000-8000-000000000001";
        let ct = "0190a000-0000-7000-8000-000000000002";
        seed_operator(&pool, op).await;
        seed_check_type(&pool, ct).await;

        let id1 = "0190b000-0000-7000-8000-00000000aaaa";
        sqlx::query(
            "INSERT INTO operator_specialties (id, operator_id, check_type_id, \
             created_at, updated_at, deleted_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', NULL, 1, 0, ?)",
        )
        .bind(id1)
        .bind(op)
        .bind(ct)
        .bind(E)
        .execute(&pool)
        .await
        .unwrap();

        let id2 = "0190b000-0000-7000-8000-00000000bbbb";
        let ch = change(
            "operator_specialties",
            serde_json::json!({
                "id": id2, "operator_id": op, "check_type_id": ct,
                "created_at": "2026-06-26T10:00:00Z", "updated_at": "2026-06-26T10:00:00Z",
                "deleted_at": null, "origin_device_id": "dev-b", "entity_id": E,
            }),
            1,
        );

        let mut tx = pool.begin().await.unwrap();
        // Must NOT error (previously: UNIQUE constraint failed -> pull abort).
        apply_operator_specialties_change(&mut tx, &ch)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let live: Vec<String> =
            sqlx::query("SELECT id FROM operator_specialties WHERE deleted_at IS NULL ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.get::<String, _>("id"))
                .collect();
        assert_eq!(
            live,
            vec![id2.to_string()],
            "incoming row should be the sole live row"
        );

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM operator_specialties WHERE id = ?")
                .bind(id1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total, 1, "loser is tombstoned, not deleted");
    }

    /// A locally-dirty colliding row must NEVER be tombstoned by the guard
    /// (never clobber an unpushed local edit).
    #[tokio::test]
    async fn operator_specialties_collision_preserves_dirty_local_row() {
        let pool = migrated_pool().await;
        let op = "0190a000-0000-7000-8000-000000000011";
        let ct = "0190a000-0000-7000-8000-000000000012";
        seed_operator(&pool, op).await;
        seed_check_type(&pool, ct).await;

        let id1 = "0190b000-0000-7000-8000-00000000cccc";
        sqlx::query(
            "INSERT INTO operator_specialties (id, operator_id, check_type_id, \
             created_at, updated_at, deleted_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', NULL, 3, 1, ?)",
        )
        .bind(id1)
        .bind(op)
        .bind(ct)
        .bind(E)
        .execute(&pool)
        .await
        .unwrap();

        let id2 = "0190b000-0000-7000-8000-00000000dddd";
        let ch = change(
            "operator_specialties",
            serde_json::json!({
                "id": id2, "operator_id": op, "check_type_id": ct,
                "created_at": "2026-06-26T10:00:00Z", "updated_at": "2026-06-26T10:00:00Z",
                "deleted_at": null, "origin_device_id": "dev-b", "entity_id": E,
            }),
            1,
        );
        let mut tx = pool.begin().await.unwrap();
        // The dirty row is guarded; the insert itself may error on the unique
        // index, which is fine -- the point is the dirty local edit is untouched.
        let _ = apply_operator_specialties_change(&mut tx, &ch).await;
        drop(tx);

        let row = sqlx::query("SELECT deleted_at, dirty FROM operator_specialties WHERE id = ?")
            .bind(id1)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(row.get::<Option<String>, _>("deleted_at").is_none());
        assert_eq!(row.get::<i64, _>("dirty"), 1);
    }

    /// A pulled OPEN shift for an operator who already has a local open shift
    /// (different id) must close the older one rather than abort the pull.
    #[tokio::test]
    async fn operator_shifts_open_collision_closes_older_shift() {
        let pool = migrated_pool().await;
        let op = "0190a000-0000-7000-8000-000000000021";
        let usr = "0190a000-0000-7000-8000-000000000031";
        seed_operator(&pool, op).await;
        seed_user(&pool, usr).await;

        let id1 = "0190c000-0000-7000-8000-00000000aaaa";
        sqlx::query(
            "INSERT INTO operator_shifts (id, operator_id, check_in_at, check_out_at, \
             check_in_by_user_id, check_out_by_user_id, note, \
             created_at, updated_at, deleted_at, version, dirty, entity_id) \
             VALUES (?, ?, '2026-06-26T08:00:00Z', NULL, ?, NULL, NULL, \
             '2026-06-26T08:00:00Z', '2026-06-26T08:00:00Z', NULL, 1, 0, ?)",
        )
        .bind(id1)
        .bind(op)
        .bind(usr)
        .bind(E)
        .execute(&pool)
        .await
        .unwrap();

        let id2 = "0190c000-0000-7000-8000-00000000bbbb";
        let ch = change(
            "operator_shifts",
            serde_json::json!({
                "id": id2, "operator_id": op, "check_in_at": "2026-06-26T10:00:00Z",
                "check_out_at": null, "check_in_by_user_id": usr, "check_out_by_user_id": null,
                "note": null, "created_at": "2026-06-26T10:00:00Z",
                "updated_at": "2026-06-26T10:00:00Z", "deleted_at": null,
                "origin_device_id": "dev-b", "entity_id": E,
            }),
            1,
        );
        let mut tx = pool.begin().await.unwrap();
        apply_operator_shifts_change(&mut tx, &ch).await.unwrap();
        tx.commit().await.unwrap();

        let open: Vec<String> = sqlx::query(
            "SELECT id FROM operator_shifts \
             WHERE check_out_at IS NULL AND deleted_at IS NULL ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>("id"))
        .collect();
        assert_eq!(open, vec![id2.to_string()]);
        let old_closed: Option<String> =
            sqlx::query_scalar("SELECT check_out_at FROM operator_shifts WHERE id = ?")
                .bind(id1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(old_closed.is_some(), "older open shift should be closed");
    }

    async fn seed_patient(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO patients (id, name, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, 'Pat', '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_doctor(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO doctors (id, name, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, 'Doc', 1, '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_mandoub(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO mandoubs (id, name, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, 'Rep', 1, '2026-06-26T09:00:00Z', '2026-06-26T09:00:00Z', 1, 0, ?)",
        )
        .bind(id)
        .bind(E)
        .execute(pool)
        .await
        .unwrap();
    }

    /// A locked visit pull payload carrying a مندوب, parameterized by the cut
    /// and name so two successive versions can diverge on exactly those fields.
    #[allow(clippy::too_many_arguments)]
    fn mandoub_visit_payload(
        id: &str,
        patient: &str,
        user: &str,
        check_type: &str,
        doctor: &str,
        operator: &str,
        mandoub: &str,
        cut: i64,
        mandoub_name: &str,
        updated_at: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id, "patient_id": patient, "status": "locked",
            "receptionist_user_id": user, "check_type_id": check_type,
            "check_subtype_id": null, "doctor_id": doctor, "operator_id": operator,
            "mandoub_id": mandoub, "dye": false, "report": false, "dalal": false,
            "locked_at": updated_at, "voided_at": null, "voided_by_user_id": null,
            "void_reason": null,
            "price_snapshot_iqd": 50000, "dye_cost_snapshot_iqd": 0,
            "report_amount_snapshot_iqd": 0, "report_pct_snapshot": null,
            "reporting_doctor_name_snapshot": null,
            "doctor_cut_snapshot_iqd": 15000, "operator_cut_snapshot_iqd": 4000,
            "mandoub_cut_snapshot_iqd": cut, "mandoub_name_snapshot": mandoub_name,
            "internal_pct_snapshot": null, "total_amount_iqd_snapshot": 50000,
            "amount_paid_override_iqd": null,
            "patient_name_snapshot": "Pat", "doctor_name_snapshot": "Doc",
            "operator_name_snapshot": "Op", "check_type_name_ar_snapshot": "فحص",
            "check_type_name_en_snapshot": null,
            "check_subtype_name_ar_snapshot": null, "check_subtype_name_en_snapshot": null,
            "created_at": "2026-06-26T09:00:00Z", "updated_at": updated_at,
            "deleted_at": null, "origin_device_id": "dev-b", "entity_id": E,
        })
    }

    /// Regression: a newer pulled version of an existing locked visit must
    /// converge the مندوب snapshot columns. The `ON CONFLICT DO UPDATE SET`
    /// clause once updated `mandoub_id` but omitted `mandoub_cut_snapshot_iqd`
    /// and `mandoub_name_snapshot`, so a re-locked/corrected visit syncing from
    /// another device left the local cut + name stale (the field-drift bug
    /// class). This pulls v1 (cut 500, "Old Rep") then v2 (cut 1000, "New Rep")
    /// and asserts both مندوب snapshots fast-forward.
    #[tokio::test]
    async fn visit_pull_update_converges_mandoub_snapshots() {
        let pool = migrated_pool().await;
        let patient = "0190d000-0000-7000-8000-000000000001";
        let user = "0190d000-0000-7000-8000-000000000002";
        let ct = "0190d000-0000-7000-8000-000000000003";
        let doctor = "0190d000-0000-7000-8000-000000000004";
        let operator = "0190d000-0000-7000-8000-000000000005";
        let mandoub = "0190d000-0000-7000-8000-000000000006";
        seed_patient(&pool, patient).await;
        seed_user(&pool, user).await;
        seed_check_type(&pool, ct).await;
        seed_doctor(&pool, doctor).await;
        seed_operator(&pool, operator).await;
        seed_mandoub(&pool, mandoub).await;

        let visit = "0190e000-0000-7000-8000-0000000000aa";
        let v1 = change(
            "visits",
            mandoub_visit_payload(
                visit,
                patient,
                user,
                ct,
                doctor,
                operator,
                mandoub,
                500,
                "Old Rep",
                "2026-06-26T10:00:00Z",
            ),
            1,
        );
        let mut tx = pool.begin().await.unwrap();
        apply_visits_change(&mut tx, &v1).await.unwrap();
        tx.commit().await.unwrap();

        let v2 = change(
            "visits",
            mandoub_visit_payload(
                visit,
                patient,
                user,
                ct,
                doctor,
                operator,
                mandoub,
                1000,
                "New Rep",
                "2026-06-26T11:00:00Z",
            ),
            2,
        );
        let mut tx = pool.begin().await.unwrap();
        apply_visits_change(&mut tx, &v2).await.unwrap();
        tx.commit().await.unwrap();

        let row = sqlx::query(
            "SELECT mandoub_cut_snapshot_iqd, mandoub_name_snapshot, version \
             FROM visits WHERE id = ?",
        )
        .bind(visit)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("version"), 2, "version fast-forwarded");
        assert_eq!(
            row.get::<i64, _>("mandoub_cut_snapshot_iqd"),
            1000,
            "مندوب cut snapshot must converge to the newer pull (was stale at 500 before the fix)"
        );
        assert_eq!(
            row.get::<String, _>("mandoub_name_snapshot"),
            "New Rep",
            "مندوب name snapshot must converge to the newer pull"
        );
    }

    /// A pulled locked visit carrying `discount: true` must land with the
    /// discount column set and a zeroed doctor cut. Also proves the bool wire
    /// shape (JSON `true`) is accepted by the discount bind.
    #[tokio::test]
    async fn visit_pull_applies_discount_flag_and_zero_doctor_cut() {
        let pool = migrated_pool().await;
        let patient = "0190d100-0000-7000-8000-000000000001";
        let user = "0190d100-0000-7000-8000-000000000002";
        let ct = "0190d100-0000-7000-8000-000000000003";
        let doctor = "0190d100-0000-7000-8000-000000000004";
        let operator = "0190d100-0000-7000-8000-000000000005";
        seed_patient(&pool, patient).await;
        seed_user(&pool, user).await;
        seed_check_type(&pool, ct).await;
        seed_doctor(&pool, doctor).await;
        seed_operator(&pool, operator).await;

        let visit = "0190e100-0000-7000-8000-0000000000aa";
        let payload = serde_json::json!({
            "id": visit, "patient_id": patient, "status": "locked",
            "receptionist_user_id": user, "check_type_id": ct,
            "check_subtype_id": null, "doctor_id": doctor, "operator_id": operator,
            "mandoub_id": null, "dye": false, "report": false, "dalal": false,
            "discount": true,
            "locked_at": "2026-06-26T10:00:00Z", "voided_at": null,
            "voided_by_user_id": null, "void_reason": null,
            "price_snapshot_iqd": 50000, "dye_cost_snapshot_iqd": 0,
            "report_amount_snapshot_iqd": 0, "report_pct_snapshot": null,
            "reporting_doctor_name_snapshot": null,
            "doctor_cut_snapshot_iqd": 0, "operator_cut_snapshot_iqd": 4000,
            "mandoub_cut_snapshot_iqd": null, "mandoub_name_snapshot": null,
            "internal_pct_snapshot": null, "total_amount_iqd_snapshot": 50000,
            "amount_paid_override_iqd": null,
            "patient_name_snapshot": "Pat", "doctor_name_snapshot": "Doc",
            "operator_name_snapshot": "Op", "check_type_name_ar_snapshot": "فحص",
            "check_type_name_en_snapshot": null,
            "check_subtype_name_ar_snapshot": null,
            "check_subtype_name_en_snapshot": null,
            "created_at": "2026-06-26T09:00:00Z",
            "updated_at": "2026-06-26T10:00:00Z",
            "deleted_at": null, "origin_device_id": "dev-b", "entity_id": E,
        });
        let v = change("visits", payload, 1);
        let mut tx = pool.begin().await.unwrap();
        apply_visits_change(&mut tx, &v).await.unwrap();
        tx.commit().await.unwrap();

        let row = sqlx::query("SELECT discount, doctor_cut_snapshot_iqd FROM visits WHERE id = ?")
            .bind(visit)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("discount"), 1, "discount flag applied");
        assert_eq!(
            row.get::<i64, _>("doctor_cut_snapshot_iqd"),
            0,
            "discounted visit has a zero doctor cut"
        );
    }
}
