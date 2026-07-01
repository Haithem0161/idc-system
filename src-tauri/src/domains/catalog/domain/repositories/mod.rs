//! Repository ports for the catalog bounded context. One trait per
//! aggregate root; implementations live in `infrastructure/repositories`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Mandoub, Operator, OperatorSpecialty,
};

#[derive(Debug, Clone, Default)]
pub struct CatalogListFilter {
    pub entity_id: String,
    pub include_deleted: bool,
    pub include_inactive: bool,
    pub query: Option<String>,
}

#[async_trait]
pub trait CheckTypeRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, ct: &CheckType) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<CheckType>>;
    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<CheckType>>;
    async fn count_live_subtypes(&self, check_type_id: Uuid) -> AppResult<i64>;
    async fn count_live_references(&self, check_type_id: Uuid) -> AppResult<i64>;
    /// Every row across ALL tenants, including tombstoned (`deleted_at`) and
    /// already-synced (`dirty = 0`) rows. Used only by the sync resync sweep
    /// (`sync_resync_local`) to re-enqueue the full local dataset; never gate
    /// this by `entity_id`/`deleted_at`/`dirty` -- the whole point is to
    /// re-push rows the normal write path would skip.
    async fn list_all_for_resync(&self) -> AppResult<Vec<CheckType>>;
}

#[async_trait]
pub trait CheckSubtypeRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, sub: &CheckSubtype) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<CheckSubtype>>;
    async fn list_by_type(&self, check_type_id: Uuid) -> AppResult<Vec<CheckSubtype>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<CheckSubtype>>;
}

#[async_trait]
pub trait DoctorRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, doc: &Doctor) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Doctor>>;
    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<Doctor>>;
    async fn search_fts(
        &self,
        entity_id: &str,
        query: &str,
        include_inactive: bool,
    ) -> AppResult<Vec<Doctor>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Doctor>>;
}

#[async_trait]
pub trait DoctorPricingRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, p: &DoctorCheckPricing) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<DoctorCheckPricing>>;
    async fn list_by_doctor(&self, doctor_id: Uuid) -> AppResult<Vec<DoctorCheckPricing>>;
    async fn find_match(
        &self,
        doctor_id: Uuid,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
    ) -> AppResult<Option<DoctorCheckPricing>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<DoctorCheckPricing>>;
}

#[async_trait]
pub trait OperatorRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, op: &Operator) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Operator>>;
    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<Operator>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Operator>>;
}

#[async_trait]
pub trait MandoubRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, m: &Mandoub) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Mandoub>>;
    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<Mandoub>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<Mandoub>>;
}

#[async_trait]
pub trait OperatorSpecialtyRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, sp: &OperatorSpecialty) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<OperatorSpecialty>>;
    async fn list_by_operator(&self, operator_id: Uuid) -> AppResult<Vec<OperatorSpecialty>>;
    async fn find_match(
        &self,
        operator_id: Uuid,
        check_type_id: Uuid,
    ) -> AppResult<Option<OperatorSpecialty>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<OperatorSpecialty>>;
}

#[async_trait]
pub trait InventoryItemRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, item: &InventoryItem) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<InventoryItem>>;
    async fn list(&self, filter: CatalogListFilter) -> AppResult<Vec<InventoryItem>>;
    async fn count_live_consumption_refs(&self, item_id: Uuid) -> AppResult<i64>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<InventoryItem>>;
}

#[async_trait]
pub trait InventoryConsumptionRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, c: &InventoryConsumptionMap) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<InventoryConsumptionMap>>;
    async fn list_by_check_type(
        &self,
        check_type_id: Uuid,
    ) -> AppResult<Vec<InventoryConsumptionMap>>;
    async fn list_by_item(&self, item_id: Uuid) -> AppResult<Vec<InventoryConsumptionMap>>;
    async fn find_match(
        &self,
        check_type_id: Uuid,
        check_subtype_id: Option<Uuid>,
        item_id: Uuid,
        on_dye_only: bool,
    ) -> AppResult<Option<InventoryConsumptionMap>>;
    /// See `CheckTypeRepo::list_all_for_resync`. All tenants, all rows.
    async fn list_all_for_resync(&self) -> AppResult<Vec<InventoryConsumptionMap>>;
}
