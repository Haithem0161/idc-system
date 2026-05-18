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
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct SettingsService {
    pool: sqlx::SqlitePool,
    setting_repo: Arc<dyn SettingRepo>,
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
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id),
        }
    }

    pub async fn list(&self, entity_id: &str) -> AppResult<Vec<Setting>> {
        self.setting_repo.list(entity_id).await
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
}

fn validate_value_for_key(key: &str, value: &SettingValue) -> AppResult<()> {
    match key {
        "dye_cost_iqd" | "report_cost_iqd" => match value {
            SettingValue::Int(n) if *n >= 0 => Ok(()),
            _ => Err(AppError::Validation(format!(
                "{key} must be a non-negative integer"
            ))),
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
        | "currency_symbol" => match value {
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
    fn report_cost_uses_same_non_negative_int_rule_as_dye_cost() {
        assert!(validate_value_for_key("report_cost_iqd", &SettingValue::Int(0)).is_ok());
        assert!(validate_value_for_key("report_cost_iqd", &SettingValue::Int(-1)).is_err());
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
