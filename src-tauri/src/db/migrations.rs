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
    (
        "003_catalog.sql",
        include_str!("../../migrations/003_catalog.sql"),
    ),
    (
        "004_operator_shifts.sql",
        include_str!("../../migrations/004_operator_shifts.sql"),
    ),
    (
        "005_patients_visits_adjustments.sql",
        include_str!("../../migrations/005_patients_visits_adjustments.sql"),
    ),
    (
        "006_inventory_ops.sql",
        include_str!("../../migrations/006_inventory_ops.sql"),
    ),
    (
        "007_reports.sql",
        include_str!("../../migrations/007_reports.sql"),
    ),
    (
        "008_polish.sql",
        include_str!("../../migrations/008_polish.sql"),
    ),
    (
        "009_pre_ship.sql",
        include_str!("../../migrations/009_pre_ship.sql"),
    ),
    (
        "010_sync_server_url.sql",
        include_str!("../../migrations/010_sync_server_url.sql"),
    ),
    (
        "011_purge_system_vacuum_outbox.sql",
        include_str!("../../migrations/011_purge_system_vacuum_outbox.sql"),
    ),
    (
        "012_patient_demographics.sql",
        include_str!("../../migrations/012_patient_demographics.sql"),
    ),
    (
        "013_patients_fts_restore_fix.sql",
        include_str!("../../migrations/013_patients_fts_restore_fix.sql"),
    ),
    (
        "014_doctor_default_cut.sql",
        include_str!("../../migrations/014_doctor_default_cut.sql"),
    ),
    (
        "015_daily_close.sql",
        include_str!("../../migrations/015_daily_close.sql"),
    ),
    (
        "016_visit_amount_paid_override.sql",
        include_str!("../../migrations/016_visit_amount_paid_override.sql"),
    ),
    (
        "017_daily_close_collected_discount.sql",
        include_str!("../../migrations/017_daily_close_collected_discount.sql"),
    ),
    (
        "018_report_percentage.sql",
        include_str!("../../migrations/018_report_percentage.sql"),
    ),
    (
        "019_daily_close_report_payable.sql",
        include_str!("../../migrations/019_daily_close_report_payable.sql"),
    ),
    (
        "020_mandoub_catalog.sql",
        include_str!("../../migrations/020_mandoub_catalog.sql"),
    ),
    (
        "021_visits_mandoub.sql",
        include_str!("../../migrations/021_visits_mandoub.sql"),
    ),
    (
        "022_visits_discount.sql",
        include_str!("../../migrations/022_visits_discount.sql"),
    ),
    (
        "023_visit_price_override.sql",
        include_str!("../../migrations/023_visit_price_override.sql"),
    ),
    (
        "024_settings_tenant_reconcile.sql",
        include_str!("../../migrations/024_settings_tenant_reconcile.sql"),
    ),
];

/// The local sync schema version: the count of embedded migrations. Sent to the
/// server as `X-Schema-Version` on every sync request so the server can reject
/// (426) a client whose local schema predates a migration that made a synced
/// column required, instead of silently accepting a payload missing that column
/// (phase-10 T3). Monotonic and forward-only because migrations are append-only.
pub const SYNC_SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

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
/// Skips `;` inside `--` line comments, inside single-quoted string literals,
/// and inside `BEGIN ... END;` blocks (CREATE TRIGGER bodies). Migrations
/// here never use multi-line `/* ... */` comments or dollar-quoted strings;
/// extend this parser if that changes.
fn split_statements(sql: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut block_depth: u32 = 0;
    let mut word = String::new();
    let mut chars = sql.chars().peekable();

    fn flush_word(word: &mut String, depth: &mut u32) {
        if word.eq_ignore_ascii_case("BEGIN") {
            *depth += 1;
        } else if word.eq_ignore_ascii_case("END") && *depth > 0 {
            *depth -= 1;
        }
        word.clear();
    }

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
                flush_word(&mut word, &mut block_depth);
                current.push(c);
                in_line_comment = true;
            }
            '\'' => {
                flush_word(&mut word, &mut block_depth);
                current.push(c);
                in_string = true;
            }
            ';' => {
                flush_word(&mut word, &mut block_depth);
                if block_depth == 0 {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        out.push(format!("{trimmed};"));
                    }
                    current.clear();
                } else {
                    current.push(c);
                }
            }
            _ => {
                if c.is_ascii_alphanumeric() || c == '_' {
                    word.push(c);
                } else {
                    flush_word(&mut word, &mut block_depth);
                }
                current.push(c);
            }
        }
    }
    flush_word(&mut word, &mut block_depth);
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

        // `sync_state.last_audit_vacuum_at` column was added by phase-08
        // §7.19. The column starts NULL until the first vacuum runs.
        let (vacuum_col,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pragma_table_info('sync_state') \
             WHERE name = 'last_audit_vacuum_at'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(vacuum_col, 1);

        // Tables exist
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
            ('outbox', 'sync_state', 'audit_log', 'metrics_events', 'users', 'settings', \
             'check_types', 'check_subtypes', 'doctors', 'doctor_check_pricing', \
             'operators', 'operator_specialties', 'inventory_items', \
             'inventory_consumption_map', 'operator_shifts')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 15);

        // FTS5 virtual table also created.
        let (vcount,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE name = 'doctors_fts'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(vcount >= 1);

        // Seed populated the v1 setting rows. Migration 002 seeds 10 keys;
        // migration 018 adds `report_pct` and `reporting_doctor_name` (+2) and
        // tombstones `report_cost_iqd` (the row is soft-deleted, not removed),
        // leaving 12 physical rows.
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 12);
        // The retired flat-cost key is tombstoned (deleted_at set), and the new
        // percentage key is live.
        let (live,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM settings WHERE deleted_at IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(live, 11);

        // Migration 012 added the 5 optional patient demographics columns
        // (and re-running run() above must not have errored on duplicate
        // ADD COLUMN -- the _migrations name-guard prevents re-application).
        let (demo_cols,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pragma_table_info('patients') \
             WHERE name IN ('phone','sex','birth_date','file_no','notes')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(demo_cols, 5);
    }

    /// Apply only the first `n` embedded migrations (each in its own tx, like
    /// `run`), recording them in `_migrations` so a later `run` resumes from
    /// there. Lets a test stand up a pre-018 schema and seed legacy rows.
    async fn apply_prefix(pool: &SqlitePool, n: usize) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _migrations (name TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
        )
        .execute(pool)
        .await
        .unwrap();
        for (name, sql) in MIGRATIONS.iter().take(n) {
            let mut tx = pool.begin().await.unwrap();
            for statement in split_statements(sql) {
                sqlx::query(&statement).execute(&mut *tx).await.unwrap();
            }
            sqlx::query("INSERT INTO _migrations (name, applied_at) VALUES (?, ?)")
                .bind(name)
                .bind("2026-01-01T00:00:00Z")
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    }

    /// Migration 018 must TRANSFORM already-locked visits frozen under the old
    /// flat-surcharge model (`total = price + dye + report_cost`) into the new
    /// carve-out model (`total = price + dye`, report as a net-side amount).
    /// Regression test for the bootstrap failure where the new total/coherence
    /// CHECK rejected a real DB that already had locked report visits.
    #[tokio::test]
    async fn migration_018_rebases_legacy_locked_report_visits() {
        let pool = init_pool_in_memory().await.unwrap();
        // Stand up the schema as it was BEFORE 018 (001..=017).
        apply_prefix(&pool, 17).await;

        // Minimal FK parents for a locked visit.
        let ts = "2026-01-02T00:00:00Z";
        sqlx::query(
            "INSERT INTO users(id,email,name,password_hash,role,is_active,created_at,updated_at,version,dirty,entity_id) \
             VALUES ('u1','r@x.io','R','x','receptionist',1,?,?,1,0,'t')",
        ).bind(ts).bind(ts).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO patients(id,name,created_at,updated_at,version,dirty,entity_id) VALUES ('p1','Pat',?,?,1,0,'t')")
            .bind(ts).bind(ts).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO check_types(id,name_ar,has_subtypes,base_price_iqd,dye_supported,report_supported,sort_order,is_active,created_at,updated_at,version,dirty,entity_id) \
             VALUES ('c1','x',0,50000,1,1,0,1,?,?,1,0,'t')",
        ).bind(ts).bind(ts).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO operators(id,name,base_cut_per_check_iqd,is_active,created_at,updated_at,version,dirty,entity_id) VALUES ('o1','Op',4000,1,?,?,1,0,'t')")
            .bind(ts).bind(ts).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO doctors(id,name,is_active,created_at,updated_at,version,dirty,entity_id) VALUES ('d1','Dr',1,?,?,1,0,'t')")
            .bind(ts).bind(ts).execute(&pool).await.unwrap();

        // A doctor-mode LOCKED visit with report ON, frozen the OLD way:
        // total = price(50000) + dye(2000) + report_cost(15000) = 67000.
        sqlx::query(
            "INSERT INTO visits(id,patient_id,status,receptionist_user_id,check_type_id,doctor_id,operator_id,dye,report,locked_at,\
               price_snapshot_iqd,dye_cost_snapshot_iqd,report_cost_snapshot_iqd,doctor_cut_snapshot_iqd,operator_cut_snapshot_iqd,\
               internal_pct_snapshot,total_amount_iqd_snapshot,patient_name_snapshot,doctor_name_snapshot,operator_name_snapshot,\
               check_type_name_ar_snapshot,created_at,updated_at,version,dirty,entity_id) \
             VALUES ('v1','p1','locked','u1','c1','d1','o1',1,1,?, 50000,2000,15000,12000,4000,NULL,67000,'Pat','Dr','Op','x',?,?,1,1,'t')",
        ).bind(ts).bind(ts).bind(ts).execute(&pool).await.unwrap();

        // Now run the rest (018 + 019). This MUST NOT fail on the CHECK.
        run(&pool).await.unwrap();

        // The legacy locked row was rebased: total = price + dye (report removed),
        // the historical report amount preserved, pct made non-null (0), dalal 0.
        let (total, amt, pct, dalal): (i64, i64, Option<i64>, i64) = sqlx::query_as(
            "SELECT total_amount_iqd_snapshot, report_amount_snapshot_iqd, report_pct_snapshot, dalal \
             FROM visits WHERE id = 'v1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 52000, "total rebased to price + dye");
        assert_eq!(amt, 15000, "legacy report amount preserved");
        assert_eq!(pct, Some(0), "report=1 requires non-null pct");
        assert_eq!(dalal, 0);
    }

    #[test]
    fn split_handles_trigger_blocks() {
        let sql = "CREATE TABLE x (id TEXT); \
                   CREATE TRIGGER t AFTER INSERT ON x BEGIN \
                       INSERT INTO x(id) VALUES ('a'); \
                       INSERT INTO x(id) VALUES ('b'); \
                   END; \
                   CREATE TABLE y (id TEXT);";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 3);
        assert!(stmts[1].to_uppercase().contains("CREATE TRIGGER"));
        assert!(stmts[1].to_uppercase().contains("END"));
    }
}
