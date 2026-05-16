//! `ShiftService`: orchestrates the 6 shift mutations declared in phase-04 §3.
//!
//! - `clock_in`: open a new shift; validates operator is active and not
//!   already on an open shift.
//! - `clock_out`: close the operator's open shift.
//! - `edit`: superadmin retroactive edit of `(check_in_at, check_out_at,
//!   note)` with overlap and future-time guards.
//! - `soft_delete`: superadmin tombstone for an orphan / overlap shift.
//! - `list_open`: tenant-scoped on-shift list with joined operator data.
//! - `history_today`: today's open + closed shifts.
//! - `list_overlaps`: tenant-scoped overlap pairs (banner driver).
//!
//! Every mutator goes through `AuditWriter::with_audit` so the audit row
//! lands BEFORE the business row -- audit-first ordering per phase-01 §7.7.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::Operator;
use crate::domains::catalog::domain::repositories::OperatorRepo;
use crate::domains::shifts::domain::entities::operator_shift::{
    OperatorShiftEditInput, OperatorShiftOpenInput,
};
use crate::domains::shifts::domain::entities::OperatorShift;
use crate::domains::shifts::domain::repositories::{OperatorShiftRepo, OverlapPair};
use crate::domains::shifts::service::push_payloads::OperatorShiftPushPayload;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

/// Hydrated view of a shift returned to the UI -- includes the joined
/// operator name so the receptionist sees the human-readable label without
/// a separate round-trip per row.
#[derive(Debug, Clone, Serialize)]
pub struct ShiftWithMeta {
    #[serde(flatten)]
    pub shift: OperatorShift,
    pub operator_name: String,
    pub operator_phone: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftEditInput {
    pub shift_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub note: Option<Option<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShiftListOverlapsArgs {
    pub operator_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct ShiftService {
    pool: sqlx::SqlitePool,
    shifts: Arc<dyn OperatorShiftRepo>,
    operators: Arc<dyn OperatorRepo>,
    writer: AuditWriter,
    device_id: String,
}

impl ShiftService {
    pub fn new(
        pool: sqlx::SqlitePool,
        shifts: Arc<dyn OperatorShiftRepo>,
        operators: Arc<dyn OperatorRepo>,
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: String,
    ) -> Self {
        Self {
            pool,
            shifts,
            operators,
            writer: AuditWriter::new(audit_repo, outbox_repo, device_id.clone()),
            device_id,
        }
    }

    fn require_role(role: UserRole, allowed: &[UserRole]) -> AppResult<()> {
        if allowed.contains(&role) {
            Ok(())
        } else {
            Err(AppError::Validation(format!(
                "this action requires one of: {:?}",
                allowed
            )))
        }
    }

    async fn load_operator(&self, operator_id: Uuid) -> AppResult<Operator> {
        self.operators
            .get_by_id(operator_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("operator {operator_id}")))
    }

    async fn load_shift(&self, shift_id: Uuid) -> AppResult<OperatorShift> {
        self.shifts
            .get_by_id(shift_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("operator_shift {shift_id}")))
    }

    /// PRD §8.3 step 1: clock-in is allowed for any active operator by any
    /// authenticated user.  Receptionist and superadmin both qualify.
    pub async fn clock_in(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        operator_id: Uuid,
        note: Option<String>,
    ) -> AppResult<OperatorShift> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        let operator = self.load_operator(operator_id).await?;
        if !operator.is_active || operator.deleted_at.is_some() {
            return Err(AppError::Validation(
                "operator is inactive or deleted".into(),
            ));
        }
        if operator.entity_id != entity_id {
            return Err(AppError::Validation(
                "operator belongs to a different tenant".into(),
            ));
        }
        if self.shifts.has_open_for_operator(operator_id, None).await? {
            return Err(AppError::Conflict(
                "operator already has an open shift".into(),
            ));
        }
        let shift = OperatorShift::open(OperatorShiftOpenInput {
            operator_id,
            by_user_id: actor_user_id,
            note,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = shift.id;
        let write = UpsertShiftWrite {
            before: None,
            after: shift,
            repo: self.shifts.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::ClockIn,
                "operator_shifts",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        self.load_shift(id).await
    }

    pub async fn clock_out(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        shift_id: Uuid,
    ) -> AppResult<OperatorShift> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        let current = self.load_shift(shift_id).await?;
        let entity_id = current.entity_id.clone();
        let updated = current.clone().close(actor_user_id, Utc::now())?;
        let write = UpsertShiftWrite {
            before: Some(current),
            after: updated,
            repo: self.shifts.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::ClockOut,
                "operator_shifts",
                &shift_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.load_shift(shift_id).await
    }

    pub async fn edit(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        input: ShiftEditInput,
    ) -> AppResult<OperatorShift> {
        Self::require_role(actor_role, &[UserRole::Superadmin])?;
        let current = self.load_shift(input.shift_id).await?;
        if current.deleted_at.is_some() {
            return Err(AppError::Validation("shift is deleted".into()));
        }
        let entity_id = current.entity_id.clone();
        let updated = current.clone().edit_times(OperatorShiftEditInput {
            check_in_at: input.check_in_at,
            check_out_at: input.check_out_at,
            note: input.note,
        })?;
        // Block reopening (out_at -> NULL) when another shift is open for
        // the same operator (§7.4, §7.8).
        if updated.check_out_at.is_none()
            && self
                .shifts
                .has_open_for_operator(updated.operator_id, Some(updated.id))
                .await?
        {
            return Err(AppError::Conflict(
                "another open shift exists for this operator".into(),
            ));
        }
        // Block overlap with non-deleted shifts of the same operator
        // (§7.8 step 4). Project the candidate window against live siblings.
        let now = Utc::now();
        let candidate_end = updated.check_out_at.unwrap_or(now);
        let siblings = self
            .shifts
            .list_for_operator(updated.operator_id, Some(updated.id))
            .await?;
        if let Some(conflict) = first_overlap(&siblings, updated.check_in_at, candidate_end, now) {
            return Err(AppError::Conflict(format!(
                "edit would overlap shift {}",
                conflict.id
            )));
        }
        let write = UpsertShiftWrite {
            before: Some(current),
            after: updated,
            repo: self.shifts.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "operator_shifts",
                &input.shift_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.load_shift(input.shift_id).await
    }

    pub async fn soft_delete(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        shift_id: Uuid,
        reason: String,
    ) -> AppResult<()> {
        Self::require_role(actor_role, &[UserRole::Superadmin])?;
        let current = self.load_shift(shift_id).await?;
        if current.deleted_at.is_some() {
            return Err(AppError::Validation("shift already deleted".into()));
        }
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_deleted();
        let write = UpsertShiftWrite {
            before: Some(current),
            after: updated,
            repo: self.shifts.clone(),
        };
        let delta_hint = serde_json::json!({ "reason": reason });
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::SoftDelete,
                "operator_shifts",
                &shift_id.to_string(),
                &entity_id,
                Some(format!("reason:{}", &delta_hint)),
                write,
            )
            .await
            .map(|_| ())
    }

    pub async fn list_open(&self, entity_id: &str) -> AppResult<Vec<ShiftWithMeta>> {
        let rows = self.shifts.list_open(entity_id).await?;
        self.hydrate(rows).await
    }

    pub async fn history_today(
        &self,
        entity_id: &str,
        today_start: DateTime<Utc>,
        today_end: DateTime<Utc>,
    ) -> AppResult<Vec<ShiftWithMeta>> {
        let rows = self
            .shifts
            .history_today(entity_id, today_start, today_end)
            .await?;
        self.hydrate(rows).await
    }

    pub async fn list_overlaps(
        &self,
        entity_id: &str,
        operator_id: Option<Uuid>,
    ) -> AppResult<Vec<OverlapPair>> {
        let now = Utc::now();
        match operator_id {
            Some(op) => self.shifts.list_overlaps_for_operator(op, now).await,
            None => self.shifts.list_overlaps(entity_id, now).await,
        }
    }

    async fn hydrate(&self, rows: Vec<OperatorShift>) -> AppResult<Vec<ShiftWithMeta>> {
        let mut out = Vec::with_capacity(rows.len());
        for shift in rows {
            let operator = self.operators.get_by_id(shift.operator_id).await?;
            let (operator_name, operator_phone) = match operator {
                Some(o) => (o.name, o.phone),
                None => ("(unknown operator)".to_string(), None),
            };
            out.push(ShiftWithMeta {
                shift,
                operator_name,
                operator_phone,
            });
        }
        Ok(out)
    }
}

fn first_overlap(
    others: &[OperatorShift],
    new_start: DateTime<Utc>,
    new_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<&OperatorShift> {
    others.iter().find(|s| {
        let s_end = s.check_out_at.unwrap_or(now);
        s.check_in_at < new_end && new_start < s_end
    })
}

/// Business-write closure shared by every mutator: insert / upsert the row
/// inside the audit-first transaction and enqueue exactly one outbox op.
struct UpsertShiftWrite {
    before: Option<OperatorShift>,
    after: OperatorShift,
    repo: Arc<dyn OperatorShiftRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertShiftWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(OperatorShiftPushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after_json = serde_json::to_value(OperatorShiftPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&OperatorShiftPushPayload::from(&self.after))?;
        let op = OutboxOp::new("operator_shifts", self.after.id.to_string(), payload);
        Ok((after_json, vec![op]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::shifts::domain::entities::operator_shift::OperatorShiftOpenInput;
    use uuid::Uuid;

    fn shift(start_min_ago: i64, dur_min: Option<i64>) -> OperatorShift {
        let base = Utc::now() - chrono::Duration::minutes(start_min_ago);
        let mut s = OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: None,
            entity_id: "tenant-x".into(),
            origin_device_id: Some("dev-1".into()),
        })
        .unwrap();
        s.check_in_at = base;
        s.check_out_at = dur_min.map(|d| base + chrono::Duration::minutes(d));
        s
    }

    #[test]
    fn require_role_accepts_listed_role() {
        assert!(ShiftService::require_role(UserRole::Superadmin, &[UserRole::Superadmin]).is_ok());
        assert!(ShiftService::require_role(
            UserRole::Receptionist,
            &[UserRole::Receptionist, UserRole::Superadmin]
        )
        .is_ok());
    }

    #[test]
    fn require_role_rejects_other_role() {
        let err = ShiftService::require_role(UserRole::Receptionist, &[UserRole::Superadmin])
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn first_overlap_detects_strict_intersection() {
        let s1 = shift(10, Some(10));
        let now = Utc::now();
        let candidate_start = now - chrono::Duration::minutes(5);
        let candidate_end = now + chrono::Duration::minutes(5);
        let arr = [s1];
        let conflict = first_overlap(&arr, candidate_start, candidate_end, now);
        assert!(conflict.is_some());
    }

    #[test]
    fn first_overlap_treats_touching_intervals_as_non_overlap() {
        let s1 = shift(10, Some(10));
        let now = Utc::now();
        let candidate_start = s1.check_out_at.unwrap();
        let candidate_end = candidate_start + chrono::Duration::minutes(5);
        let arr = [s1];
        let conflict = first_overlap(&arr, candidate_start, candidate_end, now);
        assert!(conflict.is_none());
    }

    #[test]
    fn first_overlap_treats_open_shift_as_open_ended_until_now() {
        let s_open = shift(30, None);
        let now = Utc::now();
        let arr = [s_open];
        let conflict = first_overlap(
            &arr,
            now - chrono::Duration::minutes(10),
            now - chrono::Duration::minutes(5),
            now,
        );
        assert!(conflict.is_some());
    }

    #[test]
    fn first_overlap_returns_none_for_disjoint_intervals() {
        let s1 = shift(120, Some(30));
        let now = Utc::now();
        let candidate_start = now - chrono::Duration::minutes(10);
        let candidate_end = now;
        let arr = [s1];
        let conflict = first_overlap(&arr, candidate_start, candidate_end, now);
        assert!(conflict.is_none());
    }

    #[test]
    fn first_overlap_walks_all_siblings_returning_first_hit() {
        let s1 = shift(120, Some(30));
        let s2 = shift(10, Some(10));
        let s2_id = s2.id;
        let now = Utc::now();
        let candidate_start = now - chrono::Duration::minutes(5);
        let candidate_end = now + chrono::Duration::minutes(5);
        let arr = [s1, s2];
        let conflict = first_overlap(&arr, candidate_start, candidate_end, now);
        assert_eq!(conflict.map(|s| s.id), Some(s2_id));
    }
}
