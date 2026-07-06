//! Catalog application services. Each service orchestrates a single
//! aggregate's lifecycle using audit-first writes and emits sync events.

mod check_subtype_service;
mod check_type_service;
mod consumption_service;
mod doctor_pricing_service;
mod doctor_service;
mod inventory_item_service;
mod mandoub_service;
mod operator_service;
pub mod operator_specialty_service;
pub mod push_payloads;

use std::sync::Arc;

use tauri::AppHandle;

pub use check_subtype_service::{
    CheckSubtypeCreateInput, CheckSubtypeService, CheckSubtypeUpdateInput,
};
pub use check_type_service::{CheckTypeCreateInput, CheckTypeService, CheckTypeUpdateInput};
pub use consumption_service::{ConsumptionCreateInput, ConsumptionService, ConsumptionUpdateInput};
pub use doctor_pricing_service::{DoctorPricingService, DoctorPricingUpsertInput};
pub use doctor_service::{
    DoctorCreateInput, DoctorService, DoctorUpdateInput, DuplicateDoctorGroup,
};
pub use inventory_item_service::{
    InventoryItemCreateInput, InventoryItemService, InventoryItemUpdateInput,
};
pub use mandoub_service::{MandoubCreateInput, MandoubService, MandoubUpdateInput};
pub use operator_service::{OperatorCreateInput, OperatorService, OperatorUpdateInput};
pub use operator_specialty_service::OperatorSpecialtyService;

use crate::domains::catalog::domain::repositories::{
    CheckSubtypeRepo, CheckTypeRepo, DoctorPricingRepo, DoctorRepo, InventoryConsumptionRepo,
    InventoryItemRepo, MandoubRepo, OperatorRepo, OperatorSpecialtyRepo,
};
use crate::domains::catalog::domain::services::PricingResolver;
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};

/// Bundle of every catalog service. One per process, registered in `AppState`.
pub struct CatalogServices<R: tauri::Runtime = tauri::Wry> {
    pub check_types: Arc<CheckTypeService<R>>,
    pub check_subtypes: Arc<CheckSubtypeService<R>>,
    pub doctors: Arc<DoctorService>,
    pub doctor_pricing: Arc<DoctorPricingService<R>>,
    pub operators: Arc<OperatorService>,
    pub operator_specialties: Arc<OperatorSpecialtyService>,
    pub mandoubs: Arc<MandoubService>,
    pub inventory_items: Arc<InventoryItemService>,
    pub consumption: Arc<ConsumptionService>,
    pub pricing_resolver: Arc<PricingResolver>,
    pub check_type_repo: Arc<dyn CheckTypeRepo>,
    pub check_subtype_repo: Arc<dyn CheckSubtypeRepo>,
    pub doctor_repo: Arc<dyn DoctorRepo>,
    pub doctor_pricing_repo: Arc<dyn DoctorPricingRepo>,
    pub operator_repo: Arc<dyn OperatorRepo>,
    pub operator_specialty_repo: Arc<dyn OperatorSpecialtyRepo>,
    pub mandoub_repo: Arc<dyn MandoubRepo>,
    pub inventory_item_repo: Arc<dyn InventoryItemRepo>,
    pub consumption_repo: Arc<dyn InventoryConsumptionRepo>,
}

pub struct CatalogServicesConfig<R: tauri::Runtime = tauri::Wry> {
    pub pool: sqlx::SqlitePool,
    pub check_type_repo: Arc<dyn CheckTypeRepo>,
    pub check_subtype_repo: Arc<dyn CheckSubtypeRepo>,
    pub doctor_repo: Arc<dyn DoctorRepo>,
    pub doctor_pricing_repo: Arc<dyn DoctorPricingRepo>,
    pub operator_repo: Arc<dyn OperatorRepo>,
    pub operator_specialty_repo: Arc<dyn OperatorSpecialtyRepo>,
    pub mandoub_repo: Arc<dyn MandoubRepo>,
    pub inventory_item_repo: Arc<dyn InventoryItemRepo>,
    pub consumption_repo: Arc<dyn InventoryConsumptionRepo>,
    pub audit_repo: Arc<dyn AuditRepo>,
    pub outbox_repo: Arc<dyn OutboxRepo>,
    pub device_id: String,
    pub app_handle: AppHandle<R>,
}

impl<R: tauri::Runtime> Clone for CatalogServices<R> {
    fn clone(&self) -> Self {
        Self {
            check_types: self.check_types.clone(),
            check_subtypes: self.check_subtypes.clone(),
            doctors: self.doctors.clone(),
            doctor_pricing: self.doctor_pricing.clone(),
            operators: self.operators.clone(),
            operator_specialties: self.operator_specialties.clone(),
            mandoubs: self.mandoubs.clone(),
            inventory_items: self.inventory_items.clone(),
            consumption: self.consumption.clone(),
            pricing_resolver: self.pricing_resolver.clone(),
            check_type_repo: self.check_type_repo.clone(),
            check_subtype_repo: self.check_subtype_repo.clone(),
            doctor_repo: self.doctor_repo.clone(),
            doctor_pricing_repo: self.doctor_pricing_repo.clone(),
            operator_repo: self.operator_repo.clone(),
            operator_specialty_repo: self.operator_specialty_repo.clone(),
            mandoub_repo: self.mandoub_repo.clone(),
            inventory_item_repo: self.inventory_item_repo.clone(),
            consumption_repo: self.consumption_repo.clone(),
        }
    }
}

impl<R: tauri::Runtime> CatalogServices<R> {
    pub fn new(cfg: CatalogServicesConfig<R>) -> Self {
        let CatalogServicesConfig {
            pool,
            check_type_repo,
            check_subtype_repo,
            doctor_repo,
            doctor_pricing_repo,
            operator_repo,
            operator_specialty_repo,
            mandoub_repo,
            inventory_item_repo,
            consumption_repo,
            audit_repo,
            outbox_repo,
            device_id,
            app_handle,
        } = cfg;

        let pricing_resolver = Arc::new(PricingResolver::new(
            check_type_repo.clone(),
            check_subtype_repo.clone(),
            doctor_pricing_repo.clone(),
        ));

        let check_types = Arc::new(CheckTypeService::new(
            pool.clone(),
            check_type_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
            app_handle.clone(),
        ));

        let check_subtypes = Arc::new(CheckSubtypeService::new(
            pool.clone(),
            check_type_repo.clone(),
            check_subtype_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
            app_handle.clone(),
        ));

        let doctors = Arc::new(DoctorService::new(
            pool.clone(),
            doctor_repo.clone(),
            doctor_pricing_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
        ));

        let doctor_pricing = Arc::new(DoctorPricingService::new(
            pool.clone(),
            check_type_repo.clone(),
            doctor_pricing_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
            app_handle.clone(),
        ));

        let operators = Arc::new(OperatorService::new(
            pool.clone(),
            operator_repo.clone(),
            operator_specialty_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
        ));

        let operator_specialties = Arc::new(OperatorSpecialtyService::new(
            pool.clone(),
            operator_specialty_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
        ));

        let mandoubs = Arc::new(MandoubService::new(
            pool.clone(),
            mandoub_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
        ));

        let inventory_items = Arc::new(InventoryItemService::new(
            pool.clone(),
            inventory_item_repo.clone(),
            audit_repo.clone(),
            outbox_repo.clone(),
            device_id.clone(),
        ));

        let consumption = Arc::new(ConsumptionService::new(
            pool,
            check_type_repo.clone(),
            check_subtype_repo.clone(),
            consumption_repo.clone(),
            audit_repo,
            outbox_repo,
            device_id,
        ));

        Self {
            check_types,
            check_subtypes,
            doctors,
            doctor_pricing,
            operators,
            operator_specialties,
            mandoubs,
            inventory_items,
            consumption,
            pricing_resolver,
            check_type_repo,
            check_subtype_repo,
            doctor_repo,
            doctor_pricing_repo,
            operator_repo,
            operator_specialty_repo,
            mandoub_repo,
            inventory_item_repo,
            consumption_repo,
        }
    }
}
