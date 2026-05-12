//! `Visit` aggregate (PRD §6.1.10). The hub of reception.
//!
//! State machine (legal transitions only):
//! - draft -> draft        (field edits)
//! - draft -> locked       (via lock())
//! - locked -> voided      (via void())
//! - draft -> (deleted)    (via discard(); soft-delete from draft only)
//!
//! Every other transition is rejected by `assert_transition` (§7.32). Each
//! mutator returns a fresh value; in-place writes are disallowed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisitStatus {
    Draft,
    Locked,
    Voided,
}

impl VisitStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Locked => "locked",
            Self::Voided => "voided",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "locked" => Some(Self::Locked),
            "voided" => Some(Self::Voided),
            _ => None,
        }
    }
}

/// Money + name snapshot block captured at lock time. Locked visits MUST
/// have a complete snapshot (§7.17). Internal-pct mirrors PRD §6.1.10 inv 6
/// (set iff doctor_id is None).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisitSnapshots {
    pub price_iqd: i64,
    pub dye_cost_iqd: i64,
    pub report_cost_iqd: i64,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    pub internal_pct: Option<i64>,
    pub total_amount_iqd: i64,
    pub patient_name: String,
    pub doctor_name: Option<String>,
    pub operator_name: String,
    pub check_type_name_ar: String,
    pub check_type_name_en: Option<String>,
    pub check_subtype_name_ar: Option<String>,
    pub check_subtype_name_en: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Visit {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub status: VisitStatus,
    pub receptionist_user_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub operator_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub locked_at: Option<DateTime<Utc>>,
    pub voided_at: Option<DateTime<Utc>>,
    pub voided_by_user_id: Option<Uuid>,
    pub void_reason: Option<String>,
    pub snapshots: Option<VisitSnapshots>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

#[derive(Debug, Clone)]
pub struct VisitCreateDraftInput {
    pub patient_id: Uuid,
    pub receptionist_user_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VisitDraftPatch {
    pub check_subtype_id: Option<Option<Uuid>>,
    pub doctor_id: Option<Option<Uuid>>,
    pub dye: Option<bool>,
    pub report: Option<bool>,
}

impl Visit {
    pub fn create_draft(input: VisitCreateDraftInput) -> AppResult<Self> {
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            patient_id: input.patient_id,
            status: VisitStatus::Draft,
            receptionist_user_id: input.receptionist_user_id,
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            doctor_id: input.doctor_id,
            operator_id: None,
            dye: input.dye,
            report: input.report,
            locked_at: None,
            voided_at: None,
            voided_by_user_id: None,
            void_reason: None,
            snapshots: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id: input.origin_device_id,
            entity_id: input.entity_id,
        })
    }

    /// §7.32 illegal-transition matrix. Every mutator routes through this.
    pub fn assert_transition(from: VisitStatus, to: VisitStatus) -> AppResult<()> {
        match (from, to) {
            (VisitStatus::Draft, VisitStatus::Draft)
            | (VisitStatus::Draft, VisitStatus::Locked)
            | (VisitStatus::Locked, VisitStatus::Voided) => Ok(()),
            _ => Err(AppError::Validation(format!(
                "illegal visit transition: {} -> {}",
                from.as_str(),
                to.as_str()
            ))),
        }
    }

    pub fn edit_draft(mut self, patch: VisitDraftPatch) -> AppResult<Self> {
        Self::assert_transition(self.status, VisitStatus::Draft)?;
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("visit is deleted".into()));
        }
        if let Some(sub) = patch.check_subtype_id {
            self.check_subtype_id = sub;
        }
        if let Some(doc) = patch.doctor_id {
            self.doctor_id = doc;
        }
        if let Some(dye) = patch.dye {
            self.dye = dye;
        }
        if let Some(report) = patch.report {
            self.report = report;
        }
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn lock(
        mut self,
        operator_id: Uuid,
        snapshots: VisitSnapshots,
        at: DateTime<Utc>,
    ) -> AppResult<Self> {
        Self::assert_transition(self.status, VisitStatus::Locked)?;
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("visit is deleted".into()));
        }
        // Invariant 6: internal_pct iff doctor_id is None.
        match (self.doctor_id, &snapshots.internal_pct) {
            (Some(_), Some(_)) => {
                return Err(AppError::Validation(
                    "internal_pct must be null when doctor_id is set".into(),
                ));
            }
            (None, None) => {
                return Err(AppError::Validation(
                    "internal_pct required when doctor_id is null (house mode)".into(),
                ));
            }
            _ => {}
        }
        // Total-equals-sum (§7.2).
        let expected = snapshots.price_iqd + snapshots.dye_cost_iqd + snapshots.report_cost_iqd;
        if snapshots.total_amount_iqd != expected {
            return Err(AppError::Validation(
                "total_amount_iqd_snapshot must equal price + dye + report".into(),
            ));
        }
        self.status = VisitStatus::Locked;
        self.operator_id = Some(operator_id);
        self.snapshots = Some(snapshots);
        self.locked_at = Some(at);
        self.updated_at = at;
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn void(mut self, reason: String, by_user_id: Uuid, at: DateTime<Utc>) -> AppResult<Self> {
        Self::assert_transition(self.status, VisitStatus::Voided)?;
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("visit is deleted".into()));
        }
        let trimmed = reason.trim();
        if trimmed.chars().count() < 5 {
            return Err(AppError::Validation(
                "void reason must be at least 5 characters".into(),
            ));
        }
        self.status = VisitStatus::Voided;
        self.voided_at = Some(at);
        self.voided_by_user_id = Some(by_user_id);
        self.void_reason = Some(trimmed.to_string());
        self.updated_at = at;
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn soft_delete(mut self) -> AppResult<Self> {
        // §7.31: discard only legal from draft.
        if self.status != VisitStatus::Draft {
            return Err(AppError::Validation(format!(
                "cannot discard a {} visit",
                self.status.as_str()
            )));
        }
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }
}
