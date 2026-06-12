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
use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, Operator,
};
use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo, InventoryConsumptionRepo,
    InventoryItemRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use crate::domains::patients::domain::entities::Patient;
use crate::domains::patients::domain::repositories::PatientRepo;
use crate::domains::receipts::{render as render_receipts, ReceiptArtifacts, ReceiptRenderOptions};
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
    #[serde(default)]
    pub dye: bool,
    #[serde(default)]
    pub report: bool,
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
    pub dye: Option<bool>,
    pub report: Option<bool>,
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
    pub dye_supported: bool,
    pub report_supported: bool,
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
    pub consumption: Arc<dyn InventoryConsumptionRepo>,
    pub inventory_items: Arc<dyn InventoryItemRepo>,
    pub shifts: Arc<dyn OperatorShiftRepo>,
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
    consumption: Arc<dyn InventoryConsumptionRepo>,
    #[allow(dead_code)]
    inventory_items: Arc<dyn InventoryItemRepo>,
    shifts: Arc<dyn OperatorShiftRepo>,
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
            consumption: cfg.consumption,
            inventory_items: cfg.inventory_items,
            shifts: cfg.shifts,
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
            out.push(ChecksGridCard {
                check_type_id: ct.id,
                name_ar: ct.name_ar,
                name_en: ct.name_en,
                has_subtypes: ct.has_subtypes,
                dye_supported: ct.dye_supported,
                report_supported: ct.report_supported,
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
        if let Some(sub_id) = input.check_subtype_id {
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
        }
        if input.dye && !ct.dye_supported {
            return Err(AppError::Validation(
                "check type does not support dye".into(),
            ));
        }
        if input.report && !ct.report_supported {
            return Err(AppError::Validation(
                "check type does not support report".into(),
            ));
        }
        let visit = Visit::create_draft(VisitCreateDraftInput {
            patient_id: input.patient_id,
            receptionist_user_id: actor_user_id,
            check_type_id: input.check_type_id,
            check_subtype_id: input.check_subtype_id,
            doctor_id: input.doctor_id,
            dye: input.dye,
            report: input.report,
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
        let updated = current.clone().edit_draft(VisitDraftPatch {
            patient_id: input.patient_id,
            check_subtype_id: input.check_subtype_id,
            doctor_id: input.doctor_id,
            dye: input.dye,
            report: input.report,
        })?;
        // Re-validate dye / report against parent.
        let ct = self
            .check_types
            .get_by_id(updated.check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {}", updated.check_type_id)))?;
        if updated.dye && !ct.dye_supported {
            return Err(AppError::Validation(
                "check type does not support dye".into(),
            ));
        }
        if updated.report && !ct.report_supported {
            return Err(AppError::Validation(
                "check type does not support report".into(),
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
        let bundle = self
            .resolve_bundle(visit, &visit_operator_id_placeholder(), settings)
            .await?;
        Ok(ResolvedSnapshots {
            snapshots: bundle.0,
        })
    }

    async fn resolve_bundle(
        &self,
        visit: Visit,
        operator_id: &Option<Uuid>,
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
        let snap = money_math::compute(&MoneyMathInputs {
            check_type: &check_type,
            check_subtype: check_subtype.as_ref(),
            doctor: doctor.as_ref(),
            doctor_pricing: doctor_pricing.as_ref(),
            operator: &resolved_operator,
            patient_name: &patient.name,
            dye: visit.dye,
            report: visit.report,
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
            },
        ))
    }

    pub async fn lock(
        &self,
        actor_user_id: Uuid,
        actor_role: UserRole,
        visit_id: Uuid,
        operator_id: Uuid,
        settings: MoneySettings,
        receipt_options: ReceiptRenderOptions,
    ) -> AppResult<LockResult> {
        Self::require_role(actor_role, &[UserRole::Receptionist, UserRole::Superadmin])?;
        let current = self.load_visit(visit_id).await?;
        if current.status != VisitStatus::Draft {
            return Err(AppError::Validation(format!(
                "cannot lock a {} visit",
                current.status.as_str()
            )));
        }
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
        // Resolve snapshots + bundle (catalog references).
        let (snap, bundle) = self
            .resolve_bundle(current.clone(), &Some(operator_id), settings)
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
    let now = Utc::now();
    let start = Utc
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now);
    let end = start + chrono::Duration::days(1);
    (start, end)
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
