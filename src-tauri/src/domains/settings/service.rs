//! SettingsService: tenant-scoped read + write with role gate + audit-first
//! ordering via `AuditWriter::with_audit`.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::settings::domain::entities::Setting;
use crate::domains::settings::domain::repositories::SettingRepo;
use crate::domains::settings::domain::value_objects::{is_required_key, SettingValue};
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{
    compute_delta, encode_audit_payload, AuditWriter, BusinessWrite,
};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

/// Result of a `reconcile_scope` run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub repointed: usize,
    pub tombstoned: usize,
}

#[derive(Clone)]
pub struct SettingsService {
    pool: sqlx::SqlitePool,
    setting_repo: Arc<dyn SettingRepo>,
    audit_repo: Arc<dyn AuditRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    device_id: String,
    writer: AuditWriter,
}

impl SettingsService {
    pub fn new(
        pool: sqlx::SqlitePool,
        setting_repo: Arc<dyn SettingRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            setting_repo,
            audit_repo: audit_repo.clone(),
            outbox_repo: outbox_repo.clone(),
            device_id: device_id.clone(),
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id),
        }
    }

    pub async fn list(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        self.setting_repo.list(entity_id).await
    }

    /// Build one upsert outbox op per local setting row (ALL tenants, including
    /// tombstoned and already-synced rows), using the exact same
    /// `SettingPushPayload` wire format as the normal write path. Consumed by
    /// the sync resync sweep (`sync_resync_local`), which enqueues the returned
    /// ops in one transaction. Serialization lives here so the private
    /// `SettingPushPayload` shape stays encapsulated in the settings domain.
    pub async fn resync_ops(&self) -> AppResult<Vec<OutboxOp>> {
        let settings = self.setting_repo.list_all_for_resync().await?;
        let mut ops = Vec::with_capacity(settings.len());
        for setting in &settings {
            let payload = serde_json::to_vec(&SettingPushPayload::from(setting))?;
            ops.push(OutboxOp::new("settings", setting.id.to_string(), payload));
        }
        Ok(ops)
    }

    pub async fn get(&self, key: &str, entity_id: &str) -> AppResult<Option<Setting>> {
        self.setting_repo.get_by_key(key, entity_id).await
    }

    pub async fn update(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        key: &str,
        value: SettingValue,
    ) -> AppResult<Setting> {
        if actor_role != UserRole::Superadmin {
            return Err(AppError::Validation(
                "settings update is superadmin-only".into(),
            ));
        }
        validate_value_for_key(key, &value)?;

        let existing = self.setting_repo.get_by_key(key, entity_id).await?;
        let entity_id_owned = entity_id.to_string();
        let key_owned = key.to_string();
        let value_clone = value.clone();
        let setting_repo = self.setting_repo.clone();

        let write = UpdateSettingWrite {
            existing: existing.clone(),
            key: key_owned.clone(),
            value: value_clone,
            entity_id: entity_id_owned.clone(),
            setting_repo,
        };

        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "settings",
                &existing
                    .as_ref()
                    .map(|s| s.id.to_string())
                    .unwrap_or_default(),
                &entity_id_owned,
                None,
                write,
            )
            .await?;

        self.setting_repo
            .get_by_key(&key_owned, &entity_id_owned)
            .await?
            .ok_or_else(|| AppError::Internal("setting vanished post-write".into()))
    }

    /// DEF-007 G23: atomic multi-key save.
    ///
    /// Validates every (key, value) pair up front (failing fast WITHOUT
    /// any DB writes), then applies all writes inside a single SQLite
    /// transaction. If any per-key validation fails or any write errors,
    /// the entire batch rolls back -- the caller observes the pre-batch
    /// state for every key.
    ///
    /// Audit + outbox are emitted per-key inside the same tx, audit-first
    /// per `AuditWriter` canonical ordering (phase-01 §7.7). The returned
    /// `Vec<Setting>` lists the post-write rows in the SAME ORDER as
    /// `entries`, so the caller can replay them onto the in-memory
    /// `settings_cache`.
    pub async fn update_batch(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        entries: Vec<(String, SettingValue)>,
    ) -> AppResult<Vec<Setting>> {
        if actor_role != UserRole::Superadmin {
            return Err(AppError::Validation(
                "settings update is superadmin-only".into(),
            ));
        }
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        // Validate every pair before any DB I/O so the batch is rejected
        // intact when any single key is malformed.
        for (key, value) in &entries {
            validate_value_for_key(key, value)?;
        }

        let mut existing: Vec<Option<Setting>> = Vec::with_capacity(entries.len());
        for (key, _) in &entries {
            existing.push(self.setting_repo.get_by_key(key, entity_id).await?);
        }

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let mut after_rows: Vec<Setting> = Vec::with_capacity(entries.len());

        for ((key, value), prior) in entries.into_iter().zip(existing.into_iter()) {
            let before = match &prior {
                Some(s) => serde_json::json!({
                    "key": s.key,
                    "value": s.value.as_storage(),
                    "valueType": s.value.value_type(),
                    "version": s.version,
                }),
                None => Value::Null,
            };
            let setting = match prior.clone() {
                Some(s) => s.updated_with(value.clone()),
                None => Setting::new_local(&key, value.clone(), entity_id, None)?,
            };
            self.setting_repo.upsert(&mut tx, &setting).await?;

            let after = serde_json::json!({
                "key": setting.key,
                "value": setting.value.as_storage(),
                "valueType": setting.value.value_type(),
                "version": setting.version,
            });
            let delta = compute_delta(&before, &after);

            let audit = AuditEntry::create(AuditCreateInput {
                actor_user_id,
                action: AuditAction::Update,
                entity: "settings".into(),
                entity_id: prior.as_ref().map(|s| s.id.to_string()).unwrap_or_default(),
                delta,
                ip: None,
                device_id: self.device_id.clone(),
                entity_id_tenant: entity_id.to_string(),
            });
            self.audit_repo.append(&mut tx, &audit).await?;
            let audit_payload = encode_audit_payload(&audit)?;
            let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
            self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;

            let payload = serde_json::to_vec(&SettingPushPayload::from(&setting))?;
            let outbox = OutboxOp::new("settings", setting.id.to_string(), payload);
            self.outbox_repo.enqueue(&mut tx, &outbox).await?;

            after_rows.push(setting);
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(after_rows)
    }

    /// Fold every live `'unscoped'` settings row into `tenant_entity_id`:
    /// tombstone the unscoped row when the tenant already has that key live,
    /// otherwise re-point it to the tenant. Runs in one transaction and is
    /// idempotent -- a no-op once no live `'unscoped'` rows remain. A `tenant_id`
    /// of `"unscoped"` (no real tenant yet) is a no-op.
    ///
    /// Sync semantics: the RE-POINT branch enqueues a `settings` outbox op (the
    /// row now carries the tenant `entity_id`, so it pushes cleanly and other
    /// devices converge via LWW). The TOMBSTONE branch does NOT enqueue an op:
    /// the tombstoned row keeps `entity_id = 'unscoped'`, and the server's push
    /// path rejects any settings payload whose `entity_id` differs from the
    /// caller's JWT tenant (403). Those `'unscoped'` seed rows were never pushed
    /// server-side to begin with (seeds create no outbox op), so there is nothing
    /// to converge -- enqueuing the tombstone would only park a permanently-stuck
    /// op. The soft-delete is still applied LOCALLY so the money engine and the
    /// hardened reads stop seeing the stale unscoped row.
    pub async fn reconcile_scope(&self, tenant_entity_id: &str) -> AppResult<ReconcileOutcome> {
        if tenant_entity_id == "unscoped" {
            return Ok(ReconcileOutcome::default());
        }

        let unscoped = self.setting_repo.list_live_by_entity("unscoped").await?;
        if unscoped.is_empty() {
            return Ok(ReconcileOutcome::default());
        }

        // Classify BEFORE opening the tx (reads only). For each unscoped row,
        // decide tombstone-vs-repoint by whether the tenant already holds the key.
        let mut plan: Vec<(Setting, bool)> = Vec::with_capacity(unscoped.len());
        for row in unscoped {
            let tenant_has = self
                .setting_repo
                .has_live_key(&row.key, tenant_entity_id)
                .await?;
            plan.push((row, tenant_has));
        }

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let mut out = ReconcileOutcome::default();

        for (row, tenant_has) in plan {
            // `enqueue` gates the outbox op: re-points sync (tenant-scoped,
            // accepted by the server); tombstones do NOT (they keep
            // `entity_id = 'unscoped'`, which the server 403s, and the row was
            // never pushed server-side, so there is nothing to converge).
            let (changed, enqueue) = if tenant_has {
                out.tombstoned += 1;
                (row.tombstoned(), false)
            } else {
                out.repointed += 1;
                (row.repointed_to(tenant_entity_id), true)
            };
            // MUST be update_row_by_id, NOT upsert: a tombstone sets deleted_at
            // (row no longer matches the partial unique index) and a re-point
            // changes entity_id, so the ON CONFLICT(entity_id, key) path would
            // fall through to an id-PK collision. UPDATE ... WHERE id = ? targets
            // the exact existing row.
            self.setting_repo
                .update_row_by_id(&mut tx, &changed)
                .await?;
            if enqueue {
                let payload = serde_json::to_vec(&SettingPushPayload::from(&changed))?;
                let op = OutboxOp::new("settings", changed.id.to_string(), payload);
                self.outbox_repo.enqueue(&mut tx, &op).await?;
            }
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(out)
    }
}

fn validate_value_for_key(key: &str, value: &SettingValue) -> AppResult<()> {
    match key {
        "dye_cost_iqd" => match value {
            SettingValue::Int(n) if *n >= 0 => Ok(()),
            _ => Err(AppError::Validation(format!(
                "{key} must be a non-negative integer"
            ))),
        },
        "report_pct" => match value {
            SettingValue::Int(n) if (0..=100).contains(n) => Ok(()),
            _ => Err(AppError::Validation(
                "report_pct must be an integer 0..=100".into(),
            )),
        },
        "internal_doctor_pct" => match value {
            SettingValue::Int(n) if (0..=100).contains(n) => Ok(()),
            _ => Err(AppError::Validation(
                "internal_doctor_pct must be an integer 0..=100".into(),
            )),
        },
        "idle_lock_minutes" => match value {
            SettingValue::Int(n) if *n > 0 => Ok(()),
            _ => Err(AppError::Validation(
                "idle_lock_minutes must be a positive integer".into(),
            )),
        },
        "arabic_numerals" => match value {
            SettingValue::Bool(_) => Ok(()),
            _ => Err(AppError::Validation(
                "arabic_numerals must be a bool".into(),
            )),
        },
        "thermal_width" => match value {
            SettingValue::Int(n) if *n == 32 || *n == 48 => Ok(()),
            _ => Err(AppError::Validation(
                "thermal_width must be 32 or 48".into(),
            )),
        },
        "thermal_printer_name"
        | "clinic_display_name_ar"
        | "clinic_display_name_en"
        | "currency_symbol"
        | "reporting_doctor_name" => match value {
            SettingValue::Text(_) => Ok(()),
            _ => Err(AppError::Validation(format!("{key} must be text"))),
        },
        "locale" => match value {
            SettingValue::Text(s) if s == "en" || s == "ar" => Ok(()),
            _ => Err(AppError::Validation("locale must be one of: en, ar".into())),
        },
        _ => Ok(()),
    }
}

struct UpdateSettingWrite {
    existing: Option<Setting>,
    key: String,
    value: SettingValue,
    entity_id: String,
    setting_repo: Arc<dyn SettingRepo>,
}

#[async_trait]
impl BusinessWrite for UpdateSettingWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.existing {
            Some(s) => serde_json::json!({
                "key": s.key,
                "value": s.value.as_storage(),
                "valueType": s.value.value_type(),
                "version": s.version,
            }),
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        if is_required_key(&self.key) {
            // Required keys are protected against accidental deletion but
            // updates are permitted.
        }

        let setting = match self.existing.clone() {
            Some(s) => s.updated_with(self.value.clone()),
            None => Setting::new_local(&self.key, self.value.clone(), &self.entity_id, None)?,
        };

        self.setting_repo.upsert(tx, &setting).await?;

        let after = serde_json::json!({
            "key": setting.key,
            "value": setting.value.as_storage(),
            "valueType": setting.value.value_type(),
            "version": setting.version,
        });

        let payload = serde_json::to_vec(&SettingPushPayload::from(&setting))?;
        let outbox = OutboxOp::new("settings", setting.id.to_string(), payload);

        Ok((after, vec![outbox]))
    }
}

#[derive(Serialize)]
struct SettingPushPayload<'a> {
    id: String,
    key: &'a str,
    value: String,
    value_type: &'static str,
    entity_id: &'a str,
    version: i64,
    updated_at: String,
    deleted_at: Option<String>,
}

impl<'a> From<&'a Setting> for SettingPushPayload<'a> {
    fn from(s: &'a Setting) -> Self {
        Self {
            id: s.id.to_string(),
            key: &s.key,
            value: s.value.as_storage(),
            value_type: s.value.value_type(),
            entity_id: &s.entity_id,
            version: s.version,
            updated_at: s.updated_at.to_rfc3339(),
            deleted_at: s.deleted_at.map(|d| d.to_rfc3339()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dye_cost_accepts_non_negative_int() {
        assert!(validate_value_for_key("dye_cost_iqd", &SettingValue::Int(0)).is_ok());
        assert!(validate_value_for_key("dye_cost_iqd", &SettingValue::Int(10_000)).is_ok());
    }

    #[test]
    fn dye_cost_rejects_negative_and_wrong_type() {
        assert!(validate_value_for_key("dye_cost_iqd", &SettingValue::Int(-1)).is_err());
        assert!(validate_value_for_key("dye_cost_iqd", &SettingValue::Text("10".into())).is_err());
        assert!(validate_value_for_key("dye_cost_iqd", &SettingValue::Bool(true)).is_err());
    }

    #[test]
    fn report_pct_accepts_0_to_100_and_rejects_out_of_range() {
        assert!(validate_value_for_key("report_pct", &SettingValue::Int(0)).is_ok());
        assert!(validate_value_for_key("report_pct", &SettingValue::Int(20)).is_ok());
        assert!(validate_value_for_key("report_pct", &SettingValue::Int(100)).is_ok());
        assert!(validate_value_for_key("report_pct", &SettingValue::Int(-1)).is_err());
        assert!(validate_value_for_key("report_pct", &SettingValue::Int(101)).is_err());
        assert!(validate_value_for_key("report_pct", &SettingValue::Text("20".into())).is_err());
    }

    #[test]
    fn reporting_doctor_name_accepts_text_including_empty() {
        assert!(validate_value_for_key(
            "reporting_doctor_name",
            &SettingValue::Text(String::new())
        )
        .is_ok());
        assert!(validate_value_for_key(
            "reporting_doctor_name",
            &SettingValue::Text("Dr X".into())
        )
        .is_ok());
        assert!(validate_value_for_key("reporting_doctor_name", &SettingValue::Int(1)).is_err());
    }

    #[test]
    fn internal_doctor_pct_accepts_0_to_100_inclusive() {
        for n in [0, 1, 30, 50, 99, 100] {
            assert!(
                validate_value_for_key("internal_doctor_pct", &SettingValue::Int(n)).is_ok(),
                "{n} should be accepted"
            );
        }
    }

    #[test]
    fn internal_doctor_pct_rejects_negative_and_over_100() {
        assert!(validate_value_for_key("internal_doctor_pct", &SettingValue::Int(-1)).is_err());
        assert!(validate_value_for_key("internal_doctor_pct", &SettingValue::Int(101)).is_err());
        assert!(validate_value_for_key("internal_doctor_pct", &SettingValue::Int(150)).is_err());
    }

    #[test]
    fn idle_lock_minutes_must_be_positive() {
        assert!(validate_value_for_key("idle_lock_minutes", &SettingValue::Int(1)).is_ok());
        assert!(validate_value_for_key("idle_lock_minutes", &SettingValue::Int(10)).is_ok());
        assert!(validate_value_for_key("idle_lock_minutes", &SettingValue::Int(0)).is_err());
        assert!(validate_value_for_key("idle_lock_minutes", &SettingValue::Int(-5)).is_err());
    }

    #[test]
    fn arabic_numerals_must_be_bool() {
        assert!(validate_value_for_key("arabic_numerals", &SettingValue::Bool(true)).is_ok());
        assert!(validate_value_for_key("arabic_numerals", &SettingValue::Bool(false)).is_ok());
        assert!(validate_value_for_key("arabic_numerals", &SettingValue::Int(1)).is_err());
        assert!(
            validate_value_for_key("arabic_numerals", &SettingValue::Text("true".into())).is_err()
        );
    }

    #[test]
    fn thermal_width_accepts_only_32_or_48() {
        assert!(validate_value_for_key("thermal_width", &SettingValue::Int(32)).is_ok());
        assert!(validate_value_for_key("thermal_width", &SettingValue::Int(48)).is_ok());
        assert!(validate_value_for_key("thermal_width", &SettingValue::Int(64)).is_err());
        assert!(validate_value_for_key("thermal_width", &SettingValue::Int(0)).is_err());
    }

    #[test]
    fn text_keys_must_be_text_variant() {
        for key in [
            "thermal_printer_name",
            "clinic_display_name_ar",
            "clinic_display_name_en",
            "currency_symbol",
        ] {
            assert!(validate_value_for_key(key, &SettingValue::Text(String::new())).is_ok());
            assert!(validate_value_for_key(key, &SettingValue::Text("anything".into())).is_ok());
            assert!(validate_value_for_key(key, &SettingValue::Int(0)).is_err());
            assert!(validate_value_for_key(key, &SettingValue::Bool(false)).is_err());
        }
    }

    #[test]
    fn unknown_key_falls_through_to_ok() {
        // Unknown keys are permitted by the service layer; entity layer would
        // be the place to enforce closed-key sets at v1.
        assert!(validate_value_for_key("ghost_key", &SettingValue::Int(0)).is_ok());
    }
}
