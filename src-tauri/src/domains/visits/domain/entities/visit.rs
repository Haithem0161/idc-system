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
    /// Net-side carve-out paid to the internal reporting doctor when the
    /// visit's `report` flag is on: `report_pct * (price - doctor_cut) / 100`.
    /// 0 when report is off. NOT part of the patient total.
    pub report_amount_iqd: i64,
    /// The report percentage captured at lock time. `Some(pct)` when report is
    /// on, `None` when off.
    pub report_pct: Option<i64>,
    /// The internal reporting doctor's name captured at lock time. `Some(name)`
    /// when report is on and the setting is non-empty, `None` otherwise.
    pub reporting_doctor_name: Option<String>,
    pub doctor_cut_iqd: i64,
    pub operator_cut_iqd: i64,
    /// مندوب (representative) per-visit cut: 500 or 1000 IQD when a مندوب is
    /// referenced, 0 otherwise. PURE PASSTHROUGH -- chosen on the visit and
    /// snapshotted as-is; it is a net-side carve-out subtracted later in the
    /// reports read-model and never participates in the doctor/operator cut or
    /// the patient total.
    pub mandoub_cut_iqd: i64,
    /// مندوب name captured at lock time. `Some(name)` when a مندوب is
    /// referenced, `None` otherwise. Mirrors the doctor/operator name snapshots.
    pub mandoub_name: Option<String>,
    pub internal_pct: Option<i64>,
    pub total_amount_iqd: i64,
    /// Cash actually collected when the receptionist overrides the billed total
    /// (e.g. the patient cannot pay in full). `None` = paid the full
    /// `total_amount_iqd`. Decoupled from the billed money model: it is excluded
    /// from the `total = price + dye` invariant and never affects the doctor or
    /// operator cut. `Some(0)` is legal (waived).
    pub amount_paid_override_iqd: Option<i64>,
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
    /// مندوب (representative) reference. Valid ONLY when a real referring doctor
    /// is selected (`mandoub_id` Some => `doctor_id` Some). The chosen 500/1000
    /// cut is NOT stored here -- it travels through the lock path and lands in
    /// the snapshot.
    pub mandoub_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    /// دلال (dalal) money mode: a built-in doctor substitute with a flat cut.
    /// Mutually exclusive with a referring doctor (`dalal` true => `doctor_id`
    /// None).
    pub dalal: bool,
    /// Discount mode: valid ONLY with a real referring doctor (`discount` true =>
    /// `doctor_id` Some). When set, the money engine forces the referring
    /// doctor's cut for this visit to 0; nothing else moves (patient total,
    /// operator cut, report, and مندوب cut are unchanged).
    pub discount: bool,
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
    pub mandoub_id: Option<Uuid>,
    pub dye: bool,
    pub report: bool,
    pub dalal: bool,
    pub discount: bool,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VisitDraftPatch {
    /// `Some(id)` reassigns the draft's patient (legal only in Draft state).
    /// `None` leaves the patient unchanged.
    pub patient_id: Option<Uuid>,
    pub check_subtype_id: Option<Option<Uuid>>,
    pub doctor_id: Option<Option<Uuid>>,
    pub mandoub_id: Option<Option<Uuid>>,
    pub dye: Option<bool>,
    pub report: Option<bool>,
    pub dalal: Option<bool>,
    pub discount: Option<bool>,
}

impl Visit {
    pub fn create_draft(input: VisitCreateDraftInput) -> AppResult<Self> {
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id required".into()));
        }
        // دلال is a doctor substitute: it can never coexist with a referring
        // doctor.
        if input.dalal && input.doctor_id.is_some() {
            return Err(AppError::Validation(
                "dalal cannot coexist with a referring doctor".into(),
            ));
        }
        // مندوب requires a real referring doctor (the opposite polarity of
        // dalal): it can only be referenced when a doctor is selected.
        if input.mandoub_id.is_some() && input.doctor_id.is_none() {
            return Err(AppError::Validation(
                "mandoub requires a referring doctor".into(),
            ));
        }
        // Discount applies to the referring doctor's cut, so it requires a real
        // referring doctor (and is therefore incompatible with house/dalal).
        if input.discount && input.doctor_id.is_none() {
            return Err(AppError::Validation(
                "discount requires a referring doctor".into(),
            ));
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
            mandoub_id: input.mandoub_id,
            dye: input.dye,
            report: input.report,
            dalal: input.dalal,
            discount: input.discount,
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
        if let Some(patient_id) = patch.patient_id {
            self.patient_id = patient_id;
        }
        if let Some(sub) = patch.check_subtype_id {
            self.check_subtype_id = sub;
        }
        if let Some(doc) = patch.doctor_id {
            self.doctor_id = doc;
        }
        if let Some(mandoub) = patch.mandoub_id {
            self.mandoub_id = mandoub;
        }
        if let Some(dye) = patch.dye {
            self.dye = dye;
        }
        if let Some(report) = patch.report {
            self.report = report;
        }
        if let Some(dalal) = patch.dalal {
            self.dalal = dalal;
        }
        if let Some(discount) = patch.discount {
            self.discount = discount;
        }
        // Validate the resulting state: دلال is mutually exclusive with a
        // referring doctor. Checked after applying both patches so a single
        // edit that sets dalal AND clears the doctor is accepted.
        if self.dalal && self.doctor_id.is_some() {
            return Err(AppError::Validation(
                "dalal cannot coexist with a referring doctor".into(),
            ));
        }
        // مندوب + discount auto-clear (draft path): both are only valid with a
        // real referring doctor. If the doctor was cleared or switched to
        // house/dalal in this patch, drop them rather than erroring so a
        // combined "clear doctor (+ set dalal)" edit is accepted cleanly. This
        // is the draft-form contract: the new-visit flow can clear the doctor
        // without separately remembering to clear the مندوب or the discount.
        if self.doctor_id.is_none() {
            self.mandoub_id = None;
            self.discount = false;
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
        // Invariant 6 (re-modelled): internal_pct marks HOUSE mode ONLY. A real
        // referring doctor and دلال both leave it None; only house mode (no
        // doctor AND not dalal) sets it.
        let is_house = self.doctor_id.is_none() && !self.dalal;
        match (is_house, &snapshots.internal_pct) {
            (false, Some(_)) => {
                return Err(AppError::Validation(
                    "internal_pct must be null when a doctor is set or the visit is dalal".into(),
                ));
            }
            (true, None) => {
                return Err(AppError::Validation(
                    "internal_pct required in house mode (no doctor, not dalal)".into(),
                ));
            }
            _ => {}
        }
        // Report coherence: when report is off the amount is 0 and the
        // pct/name snapshots are absent; when on, a pct must be present.
        if self.report {
            if snapshots.report_pct.is_none() {
                return Err(AppError::Validation(
                    "report_pct_snapshot required when report is on".into(),
                ));
            }
        } else if snapshots.report_amount_iqd != 0
            || snapshots.report_pct.is_some()
            || snapshots.reporting_doctor_name.is_some()
        {
            return Err(AppError::Validation(
                "report snapshots must be absent when report is off".into(),
            ));
        }
        // مندوب coherence (mirror migration 021's locked CHECK): when set, a
        // real referring doctor must be present, the snapshot cut is 500 or
        // 1000, and the name snapshot is captured. When absent, both مندوب
        // snapshots must be null.
        if self.mandoub_id.is_some() {
            if self.doctor_id.is_none() {
                return Err(AppError::Validation(
                    "mandoub requires a referring doctor".into(),
                ));
            }
            if !matches!(snapshots.mandoub_cut_iqd, 500 | 1000) {
                return Err(AppError::Validation(
                    "mandoub_cut_snapshot must be 500 or 1000 when a mandoub is set".into(),
                ));
            }
            if snapshots.mandoub_name.is_none() {
                return Err(AppError::Validation(
                    "mandoub_name_snapshot required when a mandoub is set".into(),
                ));
            }
        } else if snapshots.mandoub_cut_iqd != 0 || snapshots.mandoub_name.is_some() {
            return Err(AppError::Validation(
                "mandoub snapshots must be absent when no mandoub is set".into(),
            ));
        }
        // Discount coherence: a discount is only valid with a real referring
        // doctor, and when on the money engine must have zeroed the doctor cut.
        if self.discount {
            if self.doctor_id.is_none() {
                return Err(AppError::Validation(
                    "discount requires a referring doctor".into(),
                ));
            }
            if snapshots.doctor_cut_iqd != 0 {
                return Err(AppError::Validation(
                    "doctor_cut_snapshot must be 0 when discount is on".into(),
                ));
            }
        }
        // Total-equals-sum (§7.2). The patient total no longer includes report;
        // report is a net-side carve-out. The override is deliberately NOT part
        // of this invariant: `total_amount_iqd` stays the BILLED total (price +
        // dye); the override only records what was collected against it.
        let expected = snapshots.price_iqd + snapshots.dye_cost_iqd;
        if snapshots.total_amount_iqd != expected {
            return Err(AppError::Validation(
                "total_amount_iqd_snapshot must equal price + dye".into(),
            ));
        }
        // A collected-amount override, when present, must be non-negative. Zero
        // is allowed (the patient could not pay / the amount was waived).
        if let Some(paid) = snapshots.amount_paid_override_iqd {
            if paid < 0 {
                return Err(AppError::Validation(
                    "amount_paid_override_iqd must be >= 0".into(),
                ));
            }
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

    /// Re-attribute this visit to a different patient. Used by the patient
    /// merge flow to fold a duplicate into a survivor; it is an identity
    /// correction, valid in any status (draft/locked/voided), and never
    /// touches the financial snapshot. Bumps the sync columns so the new
    /// `patient_id` propagates. No-op (returns self unchanged) if already
    /// pointing at `new_patient_id`.
    pub fn reattribute_patient(mut self, new_patient_id: Uuid) -> Self {
        if self.patient_id == new_patient_id {
            return self;
        }
        self.patient_id = new_patient_id;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap_house(price: i64) -> VisitSnapshots {
        VisitSnapshots {
            price_iqd: price,
            dye_cost_iqd: 0,
            report_amount_iqd: 0,
            report_pct: None,
            reporting_doctor_name: None,
            doctor_cut_iqd: price * 40 / 100,
            operator_cut_iqd: 5_000,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            internal_pct: Some(40),
            total_amount_iqd: price,
            amount_paid_override_iqd: None,
            patient_name: "Pat".into(),
            doctor_name: None,
            operator_name: "Op".into(),
            check_type_name_ar: "اختبار".into(),
            check_type_name_en: Some("Test".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
        }
    }

    fn snap_doctor(price: i64, doctor_name: &str) -> VisitSnapshots {
        VisitSnapshots {
            price_iqd: price,
            dye_cost_iqd: 0,
            report_amount_iqd: 0,
            report_pct: None,
            reporting_doctor_name: None,
            doctor_cut_iqd: 12_500,
            operator_cut_iqd: 5_000,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            internal_pct: None,
            total_amount_iqd: price,
            amount_paid_override_iqd: None,
            patient_name: "Pat".into(),
            doctor_name: Some(doctor_name.into()),
            operator_name: "Op".into(),
            check_type_name_ar: "اختبار".into(),
            check_type_name_en: Some("Test".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
        }
    }

    /// A دلال snapshot: no doctor, no internal_pct, flat 10 cut.
    fn snap_dalal(price: i64) -> VisitSnapshots {
        VisitSnapshots {
            price_iqd: price,
            dye_cost_iqd: 0,
            report_amount_iqd: 0,
            report_pct: None,
            reporting_doctor_name: None,
            doctor_cut_iqd: 10,
            operator_cut_iqd: 5_000,
            mandoub_cut_iqd: 0,
            mandoub_name: None,
            internal_pct: None,
            total_amount_iqd: price,
            amount_paid_override_iqd: None,
            patient_name: "Pat".into(),
            doctor_name: None,
            operator_name: "Op".into(),
            check_type_name_ar: "اختبار".into(),
            check_type_name_en: Some("Test".into()),
            check_subtype_name_ar: None,
            check_subtype_name_en: None,
        }
    }

    fn draft_input() -> VisitCreateDraftInput {
        VisitCreateDraftInput {
            patient_id: Uuid::now_v7(),
            receptionist_user_id: Uuid::now_v7(),
            check_type_id: Uuid::now_v7(),
            check_subtype_id: None,
            doctor_id: None,
            mandoub_id: None,
            dye: false,
            report: false,
            dalal: false,
            discount: false,
            entity_id: "t".into(),
            origin_device_id: Some("dev".into()),
        }
    }

    #[test]
    fn produces_draft_with_uuid_v7_and_version_1_dirty_true() {
        let v = Visit::create_draft(draft_input()).unwrap();
        assert_eq!(v.status, VisitStatus::Draft);
        assert_eq!(v.version, 1);
        assert!(v.dirty);
        // UUID v7 carries the version nibble 7 in the second group.
        let bytes = v.id.as_bytes();
        assert_eq!((bytes[6] & 0xF0) >> 4, 7);
        assert!(v.locked_at.is_none());
        assert!(v.voided_at.is_none());
        assert!(v.snapshots.is_none());
    }

    #[test]
    fn rejects_create_with_empty_entity_id() {
        let mut input = draft_input();
        input.entity_id = "  ".into();
        let err = Visit::create_draft(input);
        assert!(err.is_err());
    }

    #[test]
    fn assert_transition_legal_set_matches_phase_05_section_7_32() {
        use VisitStatus::*;
        // Legal: Draft -> Draft, Draft -> Locked, Locked -> Voided.
        assert!(Visit::assert_transition(Draft, Draft).is_ok());
        assert!(Visit::assert_transition(Draft, Locked).is_ok());
        assert!(Visit::assert_transition(Locked, Voided).is_ok());

        // Illegal: every other combination.
        for from in [Draft, Locked, Voided] {
            for to in [Draft, Locked, Voided] {
                let legal = matches!(
                    (from, to),
                    (Draft, Draft) | (Draft, Locked) | (Locked, Voided)
                );
                if !legal {
                    let res = Visit::assert_transition(from, to);
                    assert!(
                        res.is_err(),
                        "expected illegal transition {} -> {}",
                        from.as_str(),
                        to.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn edit_draft_bumps_version_and_records_dye_flag() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let edited = v
            .clone()
            .edit_draft(VisitDraftPatch {
                dye: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert!(edited.dye);
        assert_eq!(edited.version, v.version + 1);
        assert!(edited.updated_at >= v.updated_at);
    }

    #[test]
    fn edit_draft_reassigns_patient_and_leaves_it_unchanged_when_absent() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let original_patient = v.patient_id;
        let new_patient = Uuid::now_v7();
        assert_ne!(original_patient, new_patient);

        // Reassign the patient.
        let moved = v
            .clone()
            .edit_draft(VisitDraftPatch {
                patient_id: Some(new_patient),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(moved.patient_id, new_patient);

        // An unrelated edit (no patient_id) must NOT touch the patient.
        let kept = moved
            .clone()
            .edit_draft(VisitDraftPatch {
                dye: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kept.patient_id, new_patient);
    }

    #[test]
    fn edit_draft_rejected_on_locked_visit() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_house(50_000), Utc::now())
            .unwrap();
        let err = locked.edit_draft(VisitDraftPatch::default());
        assert!(err.is_err());
    }

    #[test]
    fn lock_house_visit_requires_internal_pct_set() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut bad = snap_house(50_000);
        bad.internal_pct = None;
        let err = v.lock(Uuid::now_v7(), bad, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_doctor_visit_requires_internal_pct_null() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let mut bad = snap_doctor(50_000, "Dr");
        bad.internal_pct = Some(40);
        let err = v.lock(Uuid::now_v7(), bad, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_rejects_total_not_equal_sum() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut bad = snap_house(50_000);
        bad.total_amount_iqd = 999_999;
        let err = v.lock(Uuid::now_v7(), bad, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_accepts_amount_paid_override_including_zero() {
        // A receptionist override (incl. 0 = waived) is decoupled from the
        // billed total invariant: total still equals price + dye.
        for paid in [Some(0_i64), Some(25_000), None] {
            let v = Visit::create_draft(draft_input()).unwrap();
            let mut snap = snap_house(50_000);
            snap.amount_paid_override_iqd = paid;
            let locked = v.lock(Uuid::now_v7(), snap, Utc::now()).unwrap();
            let s = locked.snapshots.unwrap();
            // The override never perturbs the billed total or the doctor cut.
            assert_eq!(s.total_amount_iqd, 50_000);
            assert_eq!(s.doctor_cut_iqd, 50_000 * 40 / 100);
            assert_eq!(s.amount_paid_override_iqd, paid);
        }
    }

    #[test]
    fn lock_rejects_negative_amount_paid_override() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut snap = snap_house(50_000);
        snap.amount_paid_override_iqd = Some(-1);
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_produces_locked_status_and_populates_snapshots() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let at = Utc::now();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_doctor(50_000, "Dr"), at)
            .unwrap();
        assert_eq!(locked.status, VisitStatus::Locked);
        assert!(locked.locked_at.is_some());
        assert!(locked.snapshots.is_some());
        assert_eq!(locked.version, v.version + 1);
    }

    #[test]
    fn lock_preserves_created_at() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let created_at = v.created_at;
        let later = created_at + chrono::Duration::seconds(5);
        let locked = v.lock(Uuid::now_v7(), snap_house(50_000), later).unwrap();
        assert_eq!(locked.created_at, created_at);
    }

    #[test]
    fn lock_rejected_when_already_locked() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let at = Utc::now();
        let locked = v.lock(Uuid::now_v7(), snap_house(50_000), at).unwrap();
        let err = locked.lock(Uuid::now_v7(), snap_house(50_000), at);
        assert!(err.is_err());
    }

    #[test]
    fn lock_rejected_when_deleted() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut deleted = v.clone();
        deleted.deleted_at = Some(Utc::now());
        let err = deleted.lock(Uuid::now_v7(), snap_house(50_000), Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn void_rejects_when_not_locked() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let err = v.void("patient walked out".into(), Uuid::now_v7(), Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn void_rejects_short_reason_under_5_graphemes() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_house(50_000), Utc::now())
            .unwrap();
        // 4-char trimmed reason
        let err = locked.void("oops".into(), Uuid::now_v7(), Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn void_accepts_reason_with_leading_trailing_whitespace_trimmed() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_house(50_000), Utc::now())
            .unwrap();
        let voided = locked
            .void("    valid reason    ".into(), Uuid::now_v7(), Utc::now())
            .unwrap();
        assert_eq!(voided.status, VisitStatus::Voided);
        assert_eq!(voided.void_reason.as_deref(), Some("valid reason"));
    }

    #[test]
    fn void_preserves_locked_at_and_created_at() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let lock_at = Utc::now();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_house(50_000), lock_at)
            .unwrap();
        let void_at = lock_at + chrono::Duration::seconds(10);
        let voided = locked
            .clone()
            .void("patient walked".into(), Uuid::now_v7(), void_at)
            .unwrap();
        assert_eq!(voided.locked_at, locked.locked_at);
        assert_eq!(voided.created_at, locked.created_at);
        assert_eq!(voided.voided_at, Some(void_at));
    }

    #[test]
    fn void_rejects_when_already_voided() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let locked = v
            .clone()
            .lock(Uuid::now_v7(), snap_house(50_000), Utc::now())
            .unwrap();
        let voided = locked
            .void("first void".into(), Uuid::now_v7(), Utc::now())
            .unwrap();
        let err = voided.void("second void".into(), Uuid::now_v7(), Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn soft_delete_legal_only_from_draft() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let dropped = v.clone().soft_delete().unwrap();
        assert!(dropped.deleted_at.is_some());

        let locked = v
            .lock(Uuid::now_v7(), snap_house(50_000), Utc::now())
            .unwrap();
        let err = locked.soft_delete();
        assert!(err.is_err());
    }

    #[test]
    fn visit_status_parse_round_trips() {
        for s in ["draft", "locked", "voided"] {
            let parsed = VisitStatus::parse(s).unwrap();
            assert_eq!(parsed.as_str(), s);
        }
        assert!(VisitStatus::parse("other").is_none());
    }

    #[test]
    fn visit_status_serializes_as_lowercase() {
        let json = serde_json::to_string(&VisitStatus::Locked).unwrap();
        assert_eq!(json, "\"locked\"");
    }

    #[test]
    fn lock_rejects_when_total_under_or_over_sum() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut snap = snap_house(50_000);
        snap.dye_cost_iqd = 2_000;
        // total wrong (does not include dye)
        snap.total_amount_iqd = snap.price_iqd;
        assert!(v
            .clone()
            .lock(Uuid::now_v7(), snap.clone(), Utc::now())
            .is_err());
        // total wrong (over by 1; report is NOT part of the patient total).
        snap.total_amount_iqd = snap.price_iqd + snap.dye_cost_iqd + 1;
        assert!(v.lock(Uuid::now_v7(), snap, Utc::now()).is_err());
    }

    #[test]
    fn lock_dalal_visit_accepts_internal_pct_null() {
        // دلال leaves internal_pct None even though doctor_id is None.
        let mut input = draft_input();
        input.dalal = true;
        let v = Visit::create_draft(input).unwrap();
        let locked = v
            .lock(Uuid::now_v7(), snap_dalal(50_000), Utc::now())
            .unwrap();
        assert_eq!(locked.status, VisitStatus::Locked);
        let s = locked.snapshots.unwrap();
        assert_eq!(s.doctor_cut_iqd, 10);
        assert!(s.internal_pct.is_none());
    }

    #[test]
    fn lock_dalal_visit_rejects_internal_pct_set() {
        let mut input = draft_input();
        input.dalal = true;
        let v = Visit::create_draft(input).unwrap();
        let mut bad = snap_dalal(50_000);
        bad.internal_pct = Some(40);
        let err = v.lock(Uuid::now_v7(), bad, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn create_draft_rejects_dalal_with_doctor() {
        let mut input = draft_input();
        input.dalal = true;
        input.doctor_id = Some(Uuid::now_v7());
        assert!(Visit::create_draft(input).is_err());
    }

    #[test]
    fn edit_draft_rejects_setting_dalal_while_doctor_present() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let err = v.edit_draft(VisitDraftPatch {
            dalal: Some(true),
            ..Default::default()
        });
        assert!(err.is_err());
    }

    #[test]
    fn edit_draft_allows_dalal_when_doctor_cleared_in_same_patch() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let edited = v
            .edit_draft(VisitDraftPatch {
                doctor_id: Some(None),
                dalal: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert!(edited.dalal);
        assert!(edited.doctor_id.is_none());
    }

    #[test]
    fn lock_rejects_report_on_without_report_pct_snapshot() {
        let mut input = draft_input();
        input.report = true;
        let v = Visit::create_draft(input).unwrap();
        // report on but no pct snapshot -> incoherent.
        let snap = snap_house(50_000);
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_rejects_report_off_with_report_amount_present() {
        // report flag off but a non-zero amount/pct present -> incoherent.
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut snap = snap_house(50_000);
        snap.report_amount_iqd = 6_000;
        snap.report_pct = Some(20);
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }

    /// A doctor snapshot carrying a مندوب cut + name, for the locked-coherence
    /// tests.
    fn snap_doctor_with_mandoub(price: i64, cut: i64, mandoub: &str) -> VisitSnapshots {
        let mut s = snap_doctor(price, "Dr");
        s.mandoub_cut_iqd = cut;
        s.mandoub_name = Some(mandoub.into());
        s
    }

    #[test]
    fn create_draft_rejects_mandoub_without_doctor() {
        let mut input = draft_input();
        input.mandoub_id = Some(Uuid::now_v7());
        assert!(Visit::create_draft(input).is_err());
    }

    #[test]
    fn create_draft_accepts_mandoub_with_doctor() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        assert!(v.mandoub_id.is_some());
    }

    #[test]
    fn edit_draft_auto_clears_mandoub_when_doctor_cleared() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        // Clearing the doctor drops the مندوب rather than erroring.
        let edited = v
            .edit_draft(VisitDraftPatch {
                doctor_id: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert!(edited.doctor_id.is_none());
        assert!(edited.mandoub_id.is_none());
    }

    #[test]
    fn lock_mandoub_visit_snapshots_cut_and_name() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let locked = v
            .lock(
                Uuid::now_v7(),
                snap_doctor_with_mandoub(50_000, 1_000, "Rep"),
                Utc::now(),
            )
            .unwrap();
        let s = locked.snapshots.unwrap();
        assert_eq!(s.mandoub_cut_iqd, 1_000);
        assert_eq!(s.mandoub_name.as_deref(), Some("Rep"));
    }

    #[test]
    fn lock_mandoub_visit_rejects_cut_not_500_or_1000() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let err = v.lock(
            Uuid::now_v7(),
            snap_doctor_with_mandoub(50_000, 750, "Rep"),
            Utc::now(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn lock_mandoub_visit_rejects_missing_name_snapshot() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.mandoub_id = Some(Uuid::now_v7());
        let v = Visit::create_draft(input).unwrap();
        let mut snap = snap_doctor(50_000, "Dr");
        snap.mandoub_cut_iqd = 500;
        snap.mandoub_name = None;
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn create_draft_rejects_discount_without_doctor() {
        let mut input = draft_input();
        input.discount = true;
        assert!(Visit::create_draft(input).is_err());
    }

    #[test]
    fn create_draft_accepts_discount_with_doctor() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.discount = true;
        let v = Visit::create_draft(input).unwrap();
        assert!(v.discount);
    }

    #[test]
    fn edit_draft_auto_clears_discount_when_doctor_cleared() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.discount = true;
        let v = Visit::create_draft(input).unwrap();
        let edited = v
            .edit_draft(VisitDraftPatch {
                doctor_id: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert!(edited.doctor_id.is_none());
        assert!(!edited.discount);
    }

    #[test]
    fn lock_discount_visit_requires_zero_doctor_cut() {
        // A discount doctor visit whose snapshot still carries a non-zero doctor
        // cut is incoherent: the money engine must have zeroed it.
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.discount = true;
        let v = Visit::create_draft(input).unwrap();
        // snap_doctor carries doctor_cut_iqd = 12_500 -> must be rejected.
        let err = v.lock(Uuid::now_v7(), snap_doctor(50_000, "Dr"), Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_discount_visit_accepts_zero_doctor_cut() {
        let mut input = draft_input();
        input.doctor_id = Some(Uuid::now_v7());
        input.discount = true;
        let v = Visit::create_draft(input).unwrap();
        let mut snap = snap_doctor(50_000, "Dr");
        snap.doctor_cut_iqd = 0;
        let locked = v.lock(Uuid::now_v7(), snap, Utc::now()).unwrap();
        let s = locked.snapshots.unwrap();
        assert_eq!(s.doctor_cut_iqd, 0);
    }

    #[test]
    fn lock_non_mandoub_visit_rejects_stray_mandoub_snapshots() {
        let v = Visit::create_draft(draft_input()).unwrap();
        let mut snap = snap_house(50_000);
        snap.mandoub_cut_iqd = 500;
        snap.mandoub_name = Some("Rep".into());
        let err = v.lock(Uuid::now_v7(), snap, Utc::now());
        assert!(err.is_err());
    }

    #[test]
    fn lock_accepts_report_on_with_coherent_snapshots() {
        let mut input = draft_input();
        input.report = true;
        let v = Visit::create_draft(input).unwrap();
        let mut snap = snap_house(50_000);
        snap.report_amount_iqd = 6_000;
        snap.report_pct = Some(20);
        snap.reporting_doctor_name = Some("Dr Report".into());
        let locked = v.lock(Uuid::now_v7(), snap, Utc::now()).unwrap();
        let s = locked.snapshots.unwrap();
        // Report is NOT part of the patient total.
        assert_eq!(s.total_amount_iqd, 50_000);
        assert_eq!(s.report_amount_iqd, 6_000);
    }

    /// Pins the serialized JSON keys of `VisitSnapshots`. The frontend
    /// `VisitSnapshotRecord` interface mirrors these names exactly; a rename
    /// here (e.g. `mandoub_cut_iqd` -> `mandoub_cut_snapshot_iqd`) silently
    /// turns the matching TS field into `undefined`, which surfaced as a `NaN`
    /// clinic-net and a missing مندوب row in the visit detail page. This test
    /// fails loudly on any wire-shape drift so the TS side can be kept in sync.
    #[test]
    fn visit_snapshots_json_keys_stable() {
        let snap = snap_doctor_with_mandoub(50_000, 1_000, "Rep Zed");
        let json = serde_json::to_value(&snap).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        keys.sort_unstable();
        let mut expected = [
            "price_iqd",
            "dye_cost_iqd",
            "report_amount_iqd",
            "report_pct",
            "reporting_doctor_name",
            "doctor_cut_iqd",
            "operator_cut_iqd",
            "mandoub_cut_iqd",
            "mandoub_name",
            "internal_pct",
            "total_amount_iqd",
            "amount_paid_override_iqd",
            "patient_name",
            "doctor_name",
            "operator_name",
            "check_type_name_ar",
            "check_type_name_en",
            "check_subtype_name_ar",
            "check_subtype_name_en",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }
}
