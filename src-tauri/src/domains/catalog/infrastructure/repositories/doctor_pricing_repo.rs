//! SQLite implementation of `DoctorPricingRepo`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::catalog::domain::entities::DoctorCheckPricing;
use crate::domains::catalog::domain::repositories::DoctorPricingRepo;
use crate::domains::catalog::domain::value_objects::CutKind;
use crate::error::{AppError, AppResult};

use super::common::{dt_opt_to_str, dt_to_str, parse_dt, parse_dt_opt, parse_uuid, parse_uuid_opt};

#[derive(Clone)]
pub struct SqliteDoctorPricingRepo {
    pool: SqlitePool,
}

impl SqliteDoctorPricingRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DoctorPricingRepo for SqliteDoctorPricingRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, p: &DoctorCheckPricing) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO doctor_check_pricing (\
                id, doctor_id, check_type_id, check_subtype_id, price_override_iqd, \
                cut_kind, cut_value, created_at, updated_at, deleted_at, version, dirty, \
                last_synced_at, origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               doctor_id = excluded.doctor_id, \
               check_type_id = excluded.check_type_id, \
               check_subtype_id = excluded.check_subtype_id, \
               price_override_iqd = excluded.price_override_iqd, \
               cut_kind = excluded.cut_kind, cut_value = excluded.cut_value, \
               updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, \
               version = excluded.version, dirty = excluded.dirty, \
               last_synced_at = excluded.last_synced_at",
        )
        .bind(p.id.to_string())
        .bind(p.doctor_id.to_string())
        .bind(p.check_type_id.to_string())
        .bind(p.check_subtype_id.map(|id| id.to_string()))
        .bind(p.price_override_iqd)
        .bind(p.cut_kind.as_str())
        .bind(p.cut_value)
        .bind(dt_to_str(p.created_at))
        .bind(dt_to_str(p.updated_at))
        .bind(dt_opt_to_str(p.deleted_at))
        .bind(p.version)
        .bind(p.dirty as i64)
        .bind(dt_opt_to_str(p.last_synced_at))
        .bind(p.origin_device_id.as_deref())
        .bind(&p.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<DoctorCheckPricing>> {
        let row: Option<DoctorPricingRow> = sqlx::query_as::<_, DoctorPricingRow>(
            "SELECT * FROM doctor_check_pricing WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(DoctorPricingRow::into_domain).transpose()
    }

    async fn list_by_doctor(&self, doctor_id: Uuid) -> AppResult<Vec<DoctorCheckPricing>> {
        let rows: Vec<DoctorPricingRow> = sqlx::query_as::<_, DoctorPricingRow>(
            "SELECT * FROM doctor_check_pricing WHERE doctor_id = ? AND deleted_at IS NULL \
             ORDER BY created_at ASC",
        )
        .bind(doctor_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(DoctorPricingRow::into_domain)
            .collect()
    }

    async fn find_match(
        &self,
        doctor_id: Uuid,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
    ) -> AppResult<Option<DoctorCheckPricing>> {
        let row: Option<DoctorPricingRow> = match check_subtype_id {
            Some(sub) => {
                sqlx::query_as::<_, DoctorPricingRow>(
                    "SELECT * FROM doctor_check_pricing \
                     WHERE doctor_id = ? AND check_type_id = ? AND check_subtype_id = ? \
                     AND deleted_at IS NULL LIMIT 1",
                )
                .bind(doctor_id.to_string())
                .bind(check_type_id.to_string())
                .bind(sub.to_string())
                .fetch_optional(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, DoctorPricingRow>(
                    "SELECT * FROM doctor_check_pricing \
                     WHERE doctor_id = ? AND check_type_id = ? AND check_subtype_id IS NULL \
                     AND deleted_at IS NULL LIMIT 1",
                )
                .bind(doctor_id.to_string())
                .bind(check_type_id.to_string())
                .fetch_optional(&self.pool)
                .await?
            }
        };
        row.map(DoctorPricingRow::into_domain).transpose()
    }
}

#[derive(sqlx::FromRow)]
struct DoctorPricingRow {
    id: String,
    doctor_id: String,
    check_type_id: String,
    check_subtype_id: Option<String>,
    price_override_iqd: Option<i64>,
    cut_kind: String,
    cut_value: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl DoctorPricingRow {
    fn into_domain(self) -> AppResult<DoctorCheckPricing> {
        let cut_kind = CutKind::parse(&self.cut_kind)
            .ok_or_else(|| AppError::Validation(format!("invalid cut_kind: {}", self.cut_kind)))?;
        Ok(DoctorCheckPricing {
            id: parse_uuid(&self.id)?,
            doctor_id: parse_uuid(&self.doctor_id)?,
            check_type_id: parse_uuid(&self.check_type_id)?,
            check_subtype_id: parse_uuid_opt(self.check_subtype_id.as_deref())?,
            price_override_iqd: self.price_override_iqd,
            cut_kind,
            cut_value: self.cut_value,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
            deleted_at: parse_dt_opt(self.deleted_at.as_deref())?,
            version: self.version,
            dirty: self.dirty != 0,
            last_synced_at: parse_dt_opt(self.last_synced_at.as_deref())?,
            origin_device_id: self.origin_device_id,
            entity_id: self.entity_id,
        })
    }
}
