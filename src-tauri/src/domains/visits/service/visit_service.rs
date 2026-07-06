//! `VisitService`: the reception orchestration layer.
//!
//! Mutators (all routed through `AuditWriter::with_audit` so the audit
//! row precedes the business write per PRD §4.3):
//! - `create_draft` -- builds a draft visit on a check type.
//! - `update_draft` -- edits subtype / doctor / dye / report.
//! - `discard` -- soft-deletes a draft only (§7.31).
//! - `lock` -- the heavy workflow: money math, eligibility, consumption,
//!   audit, receipt rendering. Single SQLite tx for DB writes; receipts
//!   render to memory first and write to disk inside the tx via atomic
//!   temp+rename so a render failure rolls back cleanly (§7.10, §7.16).
//! - `void` -- offsets consumption rows and writes the void row
//!   (§7.11, §7.15).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, Mandoub, Operator,
};
use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo, InventoryConsumptionRepo,
    InventoryItemRepo, MandoubRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use crate::domains::patients::domain::entities::Patient;
use crate::domains::patients::domain::repositories::PatientRepo;
use crate::domains::receipts::{render as render_receipts, ReceiptArtifacts, ReceiptRenderOptions};
use crate::domains::reports::domain::repositories::FrozenCloseRepo;
use crate::domains::reports::domain::services::{baghdad_offset_seconds, utc_to_local_date};
use crate::domains::shifts::domain::repositories::OperatorShiftRepo;
use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::services::{AuditWriter, BusinessWrite};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::domains::visits::domain::entities::{
    AdjustmentNewInput, AdjustmentReason, InventoryAdjustment, Visit, VisitCreateDraftInput,
    VisitDraftPatch, VisitSnapshots, VisitStatus,
};
use crate::domains::visits::domain::repositories::{
    InventoryAdjustmentRepo, VisitRepo, WorkspaceFilters,
};
use crate::domains::visits::domain::services::money_math::{self, MoneyMathInputs, MoneySettings};
use crate::domains::visits::service::push_payloads::{
    InventoryAdjustmentPushPayload, VisitPushPayload,
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDraftInput {
    pub patient_id: Uuid,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    /// مندوب (representative) reference. Valid only with a referring doctor. The
    /// 500/1000 cut is NOT chosen here -- it is passed at LOCK time.
    pub mandoub_id: Option<Uuid>,
    #[serde(default)]
    pub dye: bool,
    #[serde(default)]
    pub report: bool,
    #[serde(default)]
    pub dalal: bool,
    /// Discount: zero the referring doctor's cut for this visit. Valid only with
    /// a real referring doctor.
    #[serde(default)]
    pub discount: bool,
    /// Editable per-visit price. `None` keeps the catalog/doctor-pricing price;
    /// `Some(n)` overrides it (e.g. a negotiated or house-call price).
    #[serde(default)]
    pub price_override_iqd: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateDraftInput {
    pub visit_id: Uuid,
    /// `Some(id)` reassigns the draft to a different patient (e.g. the
    /// receptionist corrected the name after the first autosave). `None`
    /// leaves the patient unchanged.
    pub patient_id: Option<Uuid>,
    pub check_subtype_id: Option<Option<Uuid>>,
    pub doctor_id: Option<Option<Uuid>>,
    /// `Some(Some(id))` sets the مندوب, `Some(None)` clears it, `None` leaves it
    /// unchanged. A مندوب is auto-cleared by the entity when the doctor ends up
    /// None after the patch.
    pub mandoub_id: Option<Option<Uuid>>,
    pub dye: Option<bool>,
    pub report: Option<bool>,
    pub dalal: Option<bool>,
    /// `Some(true)`/`Some(false)` sets the discount flag; `None` leaves it
    /// unchanged. Auto-cleared by the entity when the doctor ends up None.
    pub discount: Option<bool>,
    /// `Some(Some(n))` sets the editable price override, `Some(None)` clears it
    /// back to the catalog/doctor-pricing price, `None` leaves it unchanged.
    pub price_override_iqd: Option<Option<i64>>,
}

/// Resolved snapshot bundle returned to UI from `pricing::resolve` and used
/// internally by `lock` to thread the names through to the receipt.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedSnapshots {
    pub snapshots: VisitSnapshots,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LockResult {
    pub visit: Visit,
    pub artifacts: ReceiptArtifacts,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChecksGridCard {
    pub check_type_id: Uuid,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub has_subtypes: bool,
    pub dye_available: bool,
    pub todays_visits: i64,
}

/// All upstream catalog references resolved by repository lookups so the
/// lock workflow can hand them to `money_math::compute`. Most fields are
/// retained for inspection in callers that also receive the resolved
/// snapshots (e.g. PricingResolver consumers).
#[allow(dead_code)]
struct LockBundle {
    visit: Visit,
    patient: Patient,
    check_type: CheckType,
    check_subtype: Option<CheckSubtype>,
    doctor: Option<Doctor>,
    doctor_pricing: Option<DoctorCheckPricing>,
    operator: Operator,
    mandoub: Option<Mandoub>,
}

#[derive(Clone)]
pub struct VisitServiceConfig {
    pub pool: sqlx::SqlitePool,
    pub visits: Arc<dyn VisitRepo>,
    pub adjustments: Arc<dyn InventoryAdjustmentRepo>,
    pub patients: Arc<dyn PatientRepo>,
    pub check_types: Arc<dyn CheckTypeRepo>,
    pub check_subtypes: Arc<dyn CheckSubtypeRepo>,
    pub doctors: Arc<dyn DoctorRepo>,
    pub doctor_pricing: Arc<dyn DoctorPricingRepo>,
    pub operators: Arc<dyn OperatorRepo>,
    pub operator_specialties: Arc<dyn OperatorSpecialtyRepo>,
    pub mandoubs: Arc<dyn MandoubRepo>,
    pub consumption: Arc<dyn InventoryConsumptionRepo>,
    pub inventory_items: Arc<dyn InventoryItemRepo>,
    pub shifts: Arc<dyn OperatorShiftRepo>,
    pub frozen_close: Arc<dyn FrozenCloseRepo>,
    pub audit_repo: Arc<dyn AuditRepo>,
    pub outbox_repo: Arc<dyn OutboxRepo>,
    pub receipts_dir: PathBuf,
    pub device_id: String,
}

#[derive(Clone)]
pub struct VisitService {
    pool: sqlx::SqlitePool,
    visits: Arc<dyn VisitRepo>,
    adjustments: Arc<dyn InventoryAdjustmentRepo>,
    patients: Arc<dyn PatientRepo>,
    check_types: Arc<dyn CheckTypeRepo>,
    check_subtypes: Arc<dyn CheckSubtypeRepo>,
    doctors: Arc<dyn DoctorRepo>,
    doctor_pricing: Arc<dyn DoctorPricingRepo>,
    operators: Arc<dyn OperatorRepo>,
    operator_specialties: Arc<dyn OperatorSpecialtyRepo>,
    mandoubs: Arc<dyn MandoubRepo>,
    consumption: Arc<dyn InventoryConsumptionRepo>,
    #[allow(dead_code)]
    inventory_items: Arc<dyn InventoryItemRepo>,
    shifts: Arc<dyn OperatorShiftRepo>,
    frozen_close: Arc<dyn FrozenCloseRepo>,
    writer: AuditWriter,
    receipts_dir: PathBuf,
    device_id: String,
}

impl VisitService {
    pub fn new(cfg: VisitServiceConfig) -> Self {
        let writer = AuditWriter::new(cfg.audit_repo, cfg.outbox_repo, cfg.device_id.clone());
        Self {
            pool: cfg.pool,
            visits: cfg.visits,
            adjustments: cfg.adjustments,
            patients: cfg.patients,
            check_types: cfg.check_types,
            check_subtypes: cfg.check_subtypes,
            doctors: cfg.doctors,
            doctor_pricing: cfg.doctor_pricing,
            operators: cfg.operators,
            operator_specialties: cfg.operator_specialties,
            mandoubs: cfg.mandoubs,
            consumption: cfg.consumption,
            inventory_items: cfg.inventory_items,
            shifts: cfg.shifts,
            frozen_close: cfg.frozen_close,
            writer,
            receipts_dir: cfg.receipts_dir,
            device_id: cfg.device_id,
        }
    }

    pub fn visits_repo(&self) -> Arc<dyn VisitRepo> {
        self.visits.clone()
    }

    pub fn adjustments_repo(&self) -> Arc<dyn InventoryAdjustmentRepo> {
        self.adjustments.clone()
    }

    pub fn shifts_repo(&self) -> Arc<dyn OperatorShiftRepo> {
        self.shifts.clone()
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

    async fn load_visit(&self, id: Uuid) -> AppResult<Visit> {
        self.visits
            .get_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("visit {id}")))
    }

    /// Resolve the catalog dye price the same way `money_math::dye_price`
    /// does: the subtype's `dye_price_iqd` when a subtype is chosen, else the
    /// check type's. `None` means dye is not available for this check.
    fn resolve_dye_price(ct: &CheckType, sub: Option<&CheckSubtype>) -> Option<i64> {
        match sub {
            Some(s) => s.dye_price_iqd,
            None => ct.dye_price_iqd,
        }
    }

    /// Immutability guard: reject a mutation that would change the totals of a
    /// frozen day. `instant` is the UTC moment whose local day is affected -- for
    /// a lock it's "now" (the lock adds revenue to today's close); for a void
    /// it's the visit's `locked_at` (the void changes that day's close). A
    /// superadmin must reopen the close before the day can be touched again.
    async fn ensure_day_not_frozen(
        &self,
        entity_id: &str,
        instant: DateTime<Utc>,
    ) -> AppResult<()> {
        let day = utc_to_local_date(instant, baghdad_offset_seconds());
        if self
            .frozen_close
            .find_in_force_for_date(entity_id, day)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "the day {day} is frozen; a superadmin must reopen the daily close before editing it"
            )));
        }
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Visit> {
        self.load_visit(id).await
    }

    pub async fn list_today_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
    ) -> AppResult<Vec<Visit>> {
        let (start, end) = today_bounds();
        self.visits
            .list_today_by_check(entity_id, check_type_id, start, end)
            .await
    }

    pub async fn list_drafts_by_check(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
    ) -> AppResult<Vec<Visit>> {
        self.visits
            .list_drafts_by_check(entity_id, check_type_id)
            .await
    }

    pub async fn list_workspace(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
        filters: WorkspaceFilters,
        limit: i64,
    ) -> AppResult<Vec<Visit>> {
        self.visits
            .list_workspace(entity_id, check_type_id, &filters, limit)
            .await
    }

    pub async fn lines_run_today(&self, entity_id: &str, operator_id: Uuid) -> AppResult<i64> {
        let (start, end) = today_bounds();
        self.visits
            .lines_run_today_by_operator(entity_id, operator_id, start, end)
            .await
    }

    pub async fn checks_grid(&self, entity_id: &str) -> AppResult<Vec<ChecksGridCard>> {
        let cts = self
            .check_types
            .list(
                crate::domains::catalog::domain::repositories::CatalogListFilter {
                    entity_id: entity_id.to_string(),
                    include_deleted: false,
                    include_inactive: false,
                    query: None,
                },
            )
            .await?;
        let (start, end) = today_bounds();
        let mut out = Vec::with_capacity(cts.len());
        for ct in cts {
            let todays = self
                .visits
                .count_today_by_check(entity_id, ct.id, start, end)
                .await?;
            // Dye is available for the card when the check type itself has a
            // dye price, OR any of its live subtypes carries one -- a
            // subtyped check type always has a None dye price on itself
            // (§3 catalog invariant), so subtypes are the only source there.
            let dye_available = ct.dye_price_iqd.is_some()
                || self
                    .check_subtypes
                    .list_by_type(ct.id)
                    .await?
                    .iter()
                    .any(|s| s.dye_price_iqd.is_some());
            out.push(ChecksGridCard {
                check_type_id: ct.id,
                name_ar: ct.name_ar,
                name_en: ct.name_en,
                has_subtypes: ct.has_subtypes,
                dye_available,
                todays_visits: todays,
            });
        }
        Ok(out)
    }

    pub async fn create_draft(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        entity_id: &str,
        input: CreateDraftInput,
    ) -> AppResult<Visit> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        // §4.create_draft step 1-3: parent check_type lookup + subtype +
        // dye/report consistency. The entity itself enforces the legal
        // transition matrix.
        let ct = self
            .check_types
            .get_by_id(input.check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {}", input.check_type_id)))?;
        if ct.deleted_at.is_some() {
            return Err(AppError::Validation("check type is deleted".into()));
        }
        if ct.has_subtypes && input.check_subtype_id.is_none() {
            return Err(AppError::Validation(
                "check type has subtypes; subtype id required".into(),
            ));
        }
        if !ct.has_subtypes && input.check_subtype_id.is_some() {
            return Err(AppError::Validation(
                "check type does not allow a subtype".into(),
            ));
        }
        let subtype = if let Some(sub_id) = input.check_subtype_id {
            let sub = self
                .check_subtypes
                .get_by_id(sub_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("check_subtype {sub_id}")))?;
            if sub.check_type_id != ct.id || sub.deleted_at.is_some() {
                return Err(AppError::Validation(
                    "subtype does not belong to this check type".into(),
                ));
            }
            Some(sub)
        } else {
            None
        };
        if input.dye && Self::resolve_dye_price(&ct, subtype.as_ref()).is_none() {
            return Err(AppError::Validation(
                "dye not available for this check".into(),
            ));
        }
        // A discount zeroes the referring doctor's cut, so it requires a real
        // referring doctor. The entity also enforces this, but checking at the
        // boundary gives a precise error before building the draft.
        if input.discount && input.doctor_id.is_none() {
            return Err(AppError::Validation(
                "discount requires a referring doctor".into(),
            ));
        }
        // A مندوب may only be referenced with a real referring doctor, and the
        // referenced row must exist and not be soft-deleted (the frontend is
        // untrusted). The entity also enforces the doctor-required invariant.
        if let Some(mandoub_id) = input.mandoub_id {
            if input.doctor_id.is_none() {
                return Err(AppError::Validation(
                    "mandoub requires a referring doctor".into(),
                ));
            }
            let m = self
                .mandoubs
                .get_by_id(mandoub_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("mandoub {mandoub_id}")))?;
            if m.deleted_at.is_some() {
                return Err(AppError::Validation("mandoub is deleted".into()));
            }
        }
        let visit = Visit::create_draft(VisitCreateDraftInput {
            patient_id: input.patient_id,
            receptionist_user_id: actor_user_id,
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            doctor_id: input.doctor_id,
            mandoub_id: input.mandoub_id,
            dye: input.dye,
            report: input.report,
            dalal: input.dalal,
            discount: input.discount,
            price_override_iqd: input.price_override_iqd,
            entity_id: entity_id.to_string(),
            origin_device_id: Some(self.device_id.clone()),
        })?;
        let id = visit.id;
        let write = UpsertVisitWrite {
            before: None,
            after: visit,
            repo: self.visits.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Create,
                "visits",
                &id.to_string(),
                entity_id,
                None,
                write,
            )
            .await?;
        self.load_visit(id).await
    }

    pub async fn update_draft(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        input: UpdateDraftInput,
    ) -> AppResult<Visit> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        let current = self.load_visit(input.visit_id).await?;
        let entity_id = current.entity_id.clone();
        // Reassigning the patient is only meaningful while the visit is still a
        // draft, and the target patient must actually exist (the frontend is
        // untrusted). `edit_draft` already rejects non-draft visits.
        if let Some(new_patient_id) = input.patient_id {
            if self.patients.get_by_id(new_patient_id).await?.is_none() {
                return Err(AppError::NotFound(format!("patient {new_patient_id}")));
            }
        }
        // When the patch SETS a مندوب, the referenced row must exist and be
        // live. The doctor-required invariant + auto-clear are enforced by the
        // entity's `edit_draft` (a مندوب is dropped if the doctor ends None).
        if let Some(Some(mandoub_id)) = input.mandoub_id {
            let m = self
                .mandoubs
                .get_by_id(mandoub_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("mandoub {mandoub_id}")))?;
            if m.deleted_at.is_some() {
                return Err(AppError::Validation("mandoub is deleted".into()));
            }
        }
        let updated = current.clone().edit_draft(VisitDraftPatch {
            patient_id: input.patient_id,
            check_subtype_id: input.check_subtype_id,
            doctor_id: input.doctor_id,
            mandoub_id: input.mandoub_id,
            dye: input.dye,
            report: input.report,
            dalal: input.dalal,
            discount: input.discount,
            price_override_iqd: input.price_override_iqd,
        })?;
        // Re-validate dye against parent. Report is universally available now.
        let ct = self
            .check_types
            .get_by_id(updated.check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {}", updated.check_type_id)))?;
        let subtype = if let Some(sub_id) = updated.check_subtype_id {
            Some(
                self.check_subtypes
                    .get_by_id(sub_id)
                    .await?
                    .ok_or_else(|| AppError::NotFound(format!("check_subtype {sub_id}")))?,
            )
        } else {
            None
        };
        if updated.dye && Self::resolve_dye_price(&ct, subtype.as_ref()).is_none() {
            return Err(AppError::Validation(
                "dye not available for this check".into(),
            ));
        }
        let write = UpsertVisitWrite {
            before: Some(current),
            after: updated,
            repo: self.visits.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Update,
                "visits",
                &input.visit_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.load_visit(input.visit_id).await
    }

    pub async fn discard(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        visit_id: Uuid,
    ) -> AppResult<()> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        let current = self.load_visit(visit_id).await?;
        if current.status != VisitStatus::Draft {
            return Err(AppError::Validation(format!(
                "cannot discard a {} visit",
                current.status.as_str()
            )));
        }
        let entity_id = current.entity_id.clone();
        let updated = current.clone().soft_delete()?;
        let write = UpsertVisitWrite {
            before: Some(current),
            after: updated,
            repo: self.visits.clone(),
        };
        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Discard,
                "visits",
                &visit_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await
            .map(|_| ())
    }

    /// Compute the eligibility set: operators currently on an open shift
    /// (any check type) AND with a specialty row for this check_type. The
    /// result is filtered to active, non-deleted operators only.
    pub async fn qualified_operators(
        &self,
        entity_id: &str,
        check_type_id: Uuid,
    ) -> AppResult<Vec<Operator>> {
        let open_shifts = self.shifts.list_open(entity_id).await?;
        let mut out = Vec::new();
        for shift in open_shifts {
            let Some(op) = self.operators.get_by_id(shift.operator_id).await? else {
                continue;
            };
            if !op.is_active || op.deleted_at.is_some() {
                continue;
            }
            let specialty = self
                .operator_specialties
                .find_match(op.id, check_type_id)
                .await?;
            let qualifies = specialty.map(|s| s.deleted_at.is_none()).unwrap_or(false);
            if qualifies {
                out.push(op);
            }
        }
        Ok(out)
    }

    /// Resolve a fresh snapshot block for the named draft. Read-only; does
    /// not mutate the visit row (§7.43).
    pub async fn resolve_snapshots(
        &self,
        visit_id: Uuid,
        settings: MoneySettings,
    ) -> AppResult<ResolvedSnapshots> {
        let visit = self.load_visit(visit_id).await?;
        if visit.status != VisitStatus::Draft {
            return Err(AppError::Validation(
                "snapshots can only be resolved on draft visits".into(),
            ));
        }
        // Dryrun: no operator and no lock-time مندوب cut yet. The cut is 0; the
        // snapshot surfaces the مندوب name (if any) for the form, and the actual
        // 500/1000 cut lands only at lock. No collected-cash override either --
        // a draft has no collected amount yet, so the preview shows the full
        // editable price.
        let bundle = self
            .resolve_bundle(visit, &visit_operator_id_placeholder(), 0, None, settings)
            .await?;
        Ok(ResolvedSnapshots {
            snapshots: bundle.0,
        })
    }

    async fn resolve_bundle(
        &self,
        visit: Visit,
        operator_id: &Option<Uuid>,
        mandoub_cut_iqd: i64,
        amount_paid_override_iqd: Option<i64>,
        settings: MoneySettings,
    ) -> AppResult<(VisitSnapshots, LockBundle)> {
        let patient = self
            .patients
            .get_by_id(visit.patient_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("patient {}", visit.patient_id)))?;
        if patient.deleted_at.is_some() || patient.name.trim().is_empty() {
            return Err(AppError::Validation("patient invalid or deleted".into()));
        }
        let check_type = self
            .check_types
            .get_by_id(visit.check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {}", visit.check_type_id)))?;
        let check_subtype = if let Some(sid) = visit.check_subtype_id {
            Some(
                self.check_subtypes
                    .get_by_id(sid)
                    .await?
                    .ok_or_else(|| AppError::NotFound(format!("check_subtype {sid}")))?,
            )
        } else {
            None
        };
        let doctor = if let Some(did) = visit.doctor_id {
            Some(
                self.doctors
                    .get_by_id(did)
                    .await?
                    .ok_or_else(|| AppError::NotFound(format!("doctor {did}")))?,
            )
        } else {
            None
        };
        let doctor_pricing = if let Some(did) = visit.doctor_id {
            self.doctor_pricing
                .find_match(did, visit.check_type_id, visit.check_subtype_id)
                .await?
        } else {
            None
        };
        // مندوب: loaded for its name snapshot when the visit references one. The
        // 500/1000 cut is supplied by the caller (lock-time), NOT read from the
        // مندوب row (the row has no cut). It is pure passthrough into compute().
        let mandoub = if let Some(mid) = visit.mandoub_id {
            Some(
                self.mandoubs
                    .get_by_id(mid)
                    .await?
                    .ok_or_else(|| AppError::NotFound(format!("mandoub {mid}")))?,
            )
        } else {
            None
        };
        let resolved_operator = match operator_id {
            Some(op_id) => self
                .operators
                .get_by_id(*op_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("operator {op_id}")))?,
            None => {
                // For dryrun the operator is unknown -- synthesize a stub with
                // zero base cut to compute the price/dye/report block.
                Operator {
                    id: Uuid::nil(),
                    name: "(unassigned)".into(),
                    phone: None,
                    base_cut_per_check_iqd: 0,
                    is_active: true,
                    notes: None,
                    created_at: visit.created_at,
                    updated_at: visit.created_at,
                    deleted_at: None,
                    version: 0,
                    dirty: false,
                    last_synced_at: None,
                    origin_device_id: None,
                    entity_id: visit.entity_id.clone(),
                }
            }
        };
        // The cut only applies when the visit actually references a مندوب; with
        // no مندوب the passthrough cut is forced to 0 and the name stays None so
        // the snapshot stays coherent regardless of what the caller passed.
        let effective_mandoub_cut = if mandoub.is_some() {
            mandoub_cut_iqd
        } else {
            0
        };
        let mandoub_name = mandoub.as_ref().map(|m| m.name.as_str());
        let snap = money_math::compute(&MoneyMathInputs {
            check_type: &check_type,
            check_subtype: check_subtype.as_ref(),
            doctor: doctor.as_ref(),
            doctor_pricing: doctor_pricing.as_ref(),
            operator: &resolved_operator,
            patient_name: &patient.name,
            dye: visit.dye,
            report: visit.report,
            dalal: visit.dalal,
            discount: visit.discount,
            mandoub_cut_iqd: effective_mandoub_cut,
            mandoub_name,
            // The engine defaults to the catalog price and full payment when
            // these are None; the caller decides what "collected" means for
            // its context (dry-run preview vs. an actual lock).
            price_override_iqd: visit.price_override_iqd,
            amount_paid_override_iqd,
            settings,
        })?;
        Ok((
            snap,
            LockBundle {
                visit,
                patient,
                check_type,
                check_subtype,
                doctor,
                doctor_pricing,
                operator: resolved_operator,
                mandoub,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn lock(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        visit_id: Uuid,
        operator_id: Uuid,
        amount_paid_override_iqd: Option<i64>,
        mandoub_cut_iqd: Option<i64>,
        settings: MoneySettings,
        receipt_options: ReceiptRenderOptions,
    ) -> AppResult<LockResult> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        // The receptionist may record that the patient paid less than billed
        // (e.g. could not afford the full price). Zero is allowed; negative is
        // not. Validated here at the boundary and again in `Visit::lock`.
        if let Some(paid) = amount_paid_override_iqd {
            if paid < 0 {
                return Err(AppError::Validation(
                    "amount_paid_override_iqd must be >= 0".into(),
                ));
            }
        }
        let current = self.load_visit(visit_id).await?;
        if current.status != VisitStatus::Draft {
            return Err(AppError::Validation(format!(
                "cannot lock a {} visit",
                current.status.as_str()
            )));
        }
        // Immutability: a lock adds revenue to TODAY's close; refuse if today is
        // already signed & frozen (a superadmin must reopen it first).
        self.ensure_day_not_frozen(&current.entity_id, Utc::now())
            .await?;
        // §7.13: re-validate the patient.
        // Operator eligibility -- compute fresh and reject if the chosen
        // operator is not in the qualified set (§7.12 TOCTOU guard).
        let qualified = self
            .qualified_operators(&current.entity_id, current.check_type_id)
            .await?;
        if qualified.is_empty() {
            return Err(AppError::Validation(
                "no qualified operator on shift for this check type".into(),
            ));
        }
        if !qualified.iter().any(|o| o.id == operator_id) {
            return Err(AppError::Validation(
                "selected operator is not qualified or no longer on shift".into(),
            ));
        }
        // مندوب cut coherence at lock: a visit WITH a مندوب must lock with a
        // 500/1000 cut; a visit WITHOUT one must not carry a cut. The entity's
        // lock() re-checks the snapshot, but validating the lock arg here gives
        // a precise error before the heavier money/render work.
        let mandoub_cut = if current.mandoub_id.is_some() {
            match mandoub_cut_iqd {
                Some(c) if matches!(c, 500 | 1000) => c,
                _ => {
                    return Err(AppError::Validation(
                        "a mandoub visit must lock with a cut of 500 or 1000".into(),
                    ));
                }
            }
        } else {
            if mandoub_cut_iqd.is_some() {
                return Err(AppError::Validation(
                    "mandoub_cut supplied for a visit with no mandoub".into(),
                ));
            }
            0
        };
        // Resolve snapshots + bundle (catalog references). The money engine
        // itself computes cut_base off the collected amount (§ cuts-paid-basis)
        // and sets amount_paid_override_iqd on the returned snapshot -- no
        // after-the-fact overlay needed.
        let (snap, bundle) = self
            .resolve_bundle(
                current.clone(),
                &Some(operator_id),
                mandoub_cut,
                amount_paid_override_iqd,
                settings,
            )
            .await?;

        let locked_at = Utc::now();
        let entity_id = current.entity_id.clone();
        let locked_visit = current.clone().lock(operator_id, snap.clone(), locked_at)?;

        // Resolve inventory consumption rows BEFORE opening the tx so the
        // catalog reads do not hold WAL.
        let consumption_rows = self
            .consumption
            .list_by_check_type(bundle.check_type.id)
            .await?;
        let now = Utc::now();
        let consumes: Vec<InventoryAdjustment> = consumption_rows
            .iter()
            .filter(|m| {
                m.deleted_at.is_none()
                    && (m.check_subtype_id == bundle.visit.check_subtype_id
                        || m.check_subtype_id.is_none())
                    && (!m.on_dye_only || bundle.visit.dye)
            })
            .map(
                |m: &InventoryConsumptionMap| -> AppResult<InventoryAdjustment> {
                    InventoryAdjustment::try_new(AdjustmentNewInput {
                        item_id: m.item_id,
                        delta: -m.quantity_per_check,
                        reason: AdjustmentReason::ConsumeVisit,
                        visit_id: Some(visit_id),
                        note: Some(format!("consume on lock of visit {}", visit_id)),
                        by_user_id: actor_user_id,
                        entity_id: entity_id.clone(),
                        origin_device_id: Some(self.device_id.clone()),
                    })
                    .map(|mut a| {
                        a.created_at = now;
                        a.updated_at = now;
                        a
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;

        // Render receipts to memory BEFORE the tx (§7.16 step 5). On
        // failure, abort before we touch the DB.
        let artifacts = render_receipts(&locked_visit, &receipt_options, &self.receipts_dir)?;

        // Single audit-first write for the visit. The closure also writes
        // every adjustment and recomputes inventory totals. To keep
        // audit-first ordering for each adjustment, we emit a single
        // `lock` audit row on the visit and one outbox push per
        // adjustment from inside the same tx so they ride together.
        // The patient snapshot is passed by value so the closure never
        // needs a second connection while the tx is held.
        let patient_for_outbox = if bundle.patient.dirty {
            Some(bundle.patient.clone())
        } else {
            None
        };
        let write = LockVisitWrite {
            before: Some(current.clone()),
            after: locked_visit.clone(),
            consumes,
            patient_for_outbox,
            visits_repo: self.visits.clone(),
            adjustments_repo: self.adjustments.clone(),
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id.clone(),
            actor_user_id,
        };

        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Lock,
                "visits",
                &visit_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;

        // We rendered receipts BEFORE the tx; they are now persisted.
        // Discard unused bundle parts to silence unused warnings.
        let _ = bundle.doctor_pricing;

        let final_visit = self.load_visit(visit_id).await?;
        Ok(LockResult {
            visit: final_visit,
            artifacts,
        })
    }

    pub async fn void(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        visit_id: Uuid,
        reason: String,
    ) -> AppResult<Visit> {
        Self::require_role(actor_role, &[UserRole::Superadmin])?;
        let current = self.load_visit(visit_id).await?;
        if current.status != VisitStatus::Locked {
            return Err(AppError::Validation(format!(
                "cannot void a {} visit",
                current.status.as_str()
            )));
        }
        // Immutability: a void reverses revenue from the day the visit was
        // LOCKED on; refuse if that day is signed & frozen. A locked visit
        // always has `locked_at`; fall back to now if somehow absent.
        let affected_day_instant = current.locked_at.unwrap_or_else(Utc::now);
        self.ensure_day_not_frozen(&current.entity_id, affected_day_instant)
            .await?;
        let trimmed = reason.trim();
        if trimmed.chars().count() < 5 {
            return Err(AppError::Validation(
                "void reason must be at least 5 characters".into(),
            ));
        }
        let at = Utc::now();
        let entity_id = current.entity_id.clone();
        let voided = current
            .clone()
            .void(trimmed.to_string(), actor_user_id, at)?;

        // Load existing consume rows so we can build offsetting positive
        // deltas to reverse inventory.
        let existing = self.adjustments.list_consume_for_visit(visit_id).await?;
        let mut offsets: Vec<InventoryAdjustment> = Vec::with_capacity(existing.len());
        let now = Utc::now();
        for a in existing {
            let mut offset = InventoryAdjustment {
                id: Uuid::now_v7(),
                item_id: a.item_id,
                delta: -a.delta,
                reason: AdjustmentReason::ConsumeVisit,
                visit_id: Some(visit_id),
                note: Some(format!("void offset of {}", a.id)),
                by_user_id: actor_user_id, // §7.15
                created_at: now,
                updated_at: now,
                deleted_at: None,
                version: 1,
                dirty: true,
                last_synced_at: None,
                origin_device_id: Some(self.device_id.clone()),
                entity_id: a.entity_id.clone(),
            };
            // Consume rows have negative delta; offset must therefore be
            // positive. Re-route through the entity validator to be safe.
            if offset.delta == 0 {
                continue;
            }
            // The receive-positive / writeoff-negative CHECK is for
            // receive/writeoff specifically, not consume_visit, so we are
            // OK keeping reason=ConsumeVisit even when delta > 0.
            offset.updated_at = now;
            offsets.push(offset);
        }

        let write = VoidVisitWrite {
            before: Some(current),
            after: voided.clone(),
            offsets,
            visits_repo: self.visits.clone(),
            adjustments_repo: self.adjustments.clone(),
            device_id: self.device_id.clone(),
        };

        self.writer
            .with_audit(
                &self.pool,
                actor_user_id,
                AuditAction::Void,
                "visits",
                &visit_id.to_string(),
                &entity_id,
                None,
                write,
            )
            .await?;
        self.load_visit(visit_id).await
    }

    pub async fn render_receipt(
        &self,
        visit_id: Uuid,
        options: ReceiptRenderOptions,
    ) -> AppResult<ReceiptArtifacts> {
        let v = self.load_visit(visit_id).await?;
        render_receipts(&v, &options, &self.receipts_dir)
    }
}

fn visit_operator_id_placeholder() -> Option<Uuid> {
    None
}

fn today_bounds() -> (DateTime<Utc>, DateTime<Utc>) {
    // Use the Baghdad local day so reception "today" matches shifts and Daily
    // Close. UTC midnight put reception 3 hours behind the local day, so the
    // first 3 hours of each local day showed yesterday's counts.
    crate::shared::tz::baghdad_today_utc_range()
}

// -------------------- BusinessWrite closures -------------------------------

struct UpsertVisitWrite {
    before: Option<Visit>,
    after: Visit,
    repo: Arc<dyn VisitRepo>,
}

#[async_trait]
impl BusinessWrite for UpsertVisitWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(VisitPushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.repo.upsert(tx, &self.after).await?;
        let after = serde_json::to_value(VisitPushPayload::from(&self.after))?;
        let payload = serde_json::to_vec(&VisitPushPayload::from(&self.after))?;
        let op = OutboxOp::new("visits", self.after.id.to_string(), payload);
        Ok((after, vec![op]))
    }
}

#[allow(clippy::struct_excessive_bools)]
struct LockVisitWrite {
    before: Option<Visit>,
    after: Visit,
    consumes: Vec<InventoryAdjustment>,
    /// Pre-loaded patient row. Some when the patient was dirty before the
    /// lock (typically because the receptionist just created the row
    /// inline). The receipt §7.18 mandates we enqueue an extra outbox op
    /// for it so the engine pushes it even if the inline create's outbox
    /// op is still in-flight.
    patient_for_outbox: Option<Patient>,
    visits_repo: Arc<dyn VisitRepo>,
    adjustments_repo: Arc<dyn InventoryAdjustmentRepo>,
    device_id: String,
    entity_id_tenant: String,
    actor_user_id: Uuid,
}

#[async_trait]
impl BusinessWrite for LockVisitWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(VisitPushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        // 1. Visit row update.
        self.visits_repo.upsert(tx, &self.after).await?;

        let mut outbox: Vec<OutboxOp> = Vec::new();
        let after = serde_json::to_value(VisitPushPayload::from(&self.after))?;
        outbox.push(OutboxOp::new(
            "visits",
            self.after.id.to_string(),
            serde_json::to_vec(&VisitPushPayload::from(&self.after))?,
        ));

        // 2. Each consume adjustment row.
        for adj in &self.consumes {
            self.adjustments_repo.append(tx, adj).await?;
            let payload = serde_json::to_vec(&InventoryAdjustmentPushPayload::from(adj))?;
            outbox.push(OutboxOp::new(
                "inventory_adjustments",
                adj.id.to_string(),
                payload,
            ));
            // 3. Recompute the item totals. The inventory_items table
            // already updates dirty=1 so the sync engine will push it.
            self.adjustments_repo
                .recompute_item_quantity(tx, adj.item_id)
                .await?;
        }

        // 4. Patient outbox enqueue (§7.18): if the patient was created
        // inline before this lock, restage its outbox op so it rides this
        // tx. Duplicates are idempotent server-side via `op_id`.
        if let Some(patient) = &self.patient_for_outbox {
            let payload = serde_json::to_vec(
                &crate::domains::patients::service::push_payloads::PatientPushPayload::from(
                    patient,
                ),
            )?;
            outbox.push(OutboxOp::new("patients", patient.id.to_string(), payload));
        }

        // Drop unused fields to silence lints.
        let _ = (&self.device_id, &self.entity_id_tenant, self.actor_user_id);

        Ok((after, outbox))
    }
}

struct VoidVisitWrite {
    before: Option<Visit>,
    after: Visit,
    offsets: Vec<InventoryAdjustment>,
    visits_repo: Arc<dyn VisitRepo>,
    adjustments_repo: Arc<dyn InventoryAdjustmentRepo>,
    device_id: String,
}

#[async_trait]
impl BusinessWrite for VoidVisitWrite {
    async fn before(&mut self, _tx: &mut crate::db::Tx<'_>) -> AppResult<Value> {
        Ok(match &self.before {
            Some(b) => serde_json::to_value(VisitPushPayload::from(b))?,
            None => Value::Null,
        })
    }

    async fn write(&mut self, tx: &mut crate::db::Tx<'_>) -> AppResult<(Value, Vec<OutboxOp>)> {
        self.visits_repo.upsert(tx, &self.after).await?;
        let mut outbox: Vec<OutboxOp> = Vec::new();
        outbox.push(OutboxOp::new(
            "visits",
            self.after.id.to_string(),
            serde_json::to_vec(&VisitPushPayload::from(&self.after))?,
        ));
        for adj in &self.offsets {
            self.adjustments_repo.append(tx, adj).await?;
            outbox.push(OutboxOp::new(
                "inventory_adjustments",
                adj.id.to_string(),
                serde_json::to_vec(&InventoryAdjustmentPushPayload::from(adj))?,
            ));
            self.adjustments_repo
                .recompute_item_quantity(tx, adj.item_id)
                .await?;
        }
        let _ = &self.device_id;
        Ok((
            serde_json::to_value(VisitPushPayload::from(&self.after))?,
            outbox,
        ))
    }
}
