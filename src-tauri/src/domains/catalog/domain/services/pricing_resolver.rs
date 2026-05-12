//! Effective-price resolver (PRD §6.1.5 inv 5; phase-03 §7.26).
//!
//! Read-only contract consumed by phase-05 `VisitService::lock`. Never
//! mutates state. The resolver is constructed with repository handles and
//! exposes a single async method.

use std::sync::Arc;

use uuid::Uuid;

use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo,
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy)]
pub struct EffectivePriceQuery {
    pub doctor_id: Option<Uuid>,
    pub check_type_id: Uuid,
    pub check_subtype_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct PricingResolver {
    check_type_repo: Arc<dyn CheckTypeRepo>,
    check_subtype_repo: Arc<dyn CheckSubtypeRepo>,
    pricing_repo: Arc<dyn DoctorPricingRepo>,
}

impl PricingResolver {
    pub fn new(
        check_type_repo: Arc<dyn CheckTypeRepo>,
        check_subtype_repo: Arc<dyn CheckSubtypeRepo>,
        pricing_repo: Arc<dyn DoctorPricingRepo>,
    ) -> Self {
        Self {
            check_type_repo,
            check_subtype_repo,
            pricing_repo,
        }
    }

    pub async fn effective_price(&self, q: EffectivePriceQuery) -> AppResult<i64> {
        let fallback = self
            .fallback_price(q.check_type_id, q.check_subtype_id)
            .await?;
        let Some(doctor_id) = q.doctor_id else {
            return Ok(fallback);
        };
        let pricing = self
            .pricing_repo
            .find_match(doctor_id, q.check_type_id, q.check_subtype_id)
            .await?;
        Ok(pricing
            .and_then(|p| p.price_override_iqd)
            .unwrap_or(fallback))
    }

    async fn fallback_price(
        &self,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
    ) -> AppResult<i64> {
        let ct = self
            .check_type_repo
            .get_by_id(check_type_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("check_type {check_type_id}")))?;

        if let Some(sub_id) = check_subtype_id {
            let sub = self
                .check_subtype_repo
                .get_by_id(sub_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("check_subtype {sub_id}")))?;
            Ok(sub.price_iqd)
        } else {
            ct.base_price_iqd.ok_or_else(|| {
                AppError::Validation(
                    "check_type has subtypes; check_subtype_id required for pricing".into(),
                )
            })
        }
    }
}
