//! Tauri commands for the catalog bounded context.
//!
//! Every mutator requires the caller to be authenticated as a superadmin
//! (enforced inside each service; commands surface the same `Validation`
//! error). Reads are open to any authenticated user.

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::domains::auth::domain::value_objects::UserRole;
use crate::domains::catalog::domain::entities::{
    CheckSubtype, CheckType, Doctor, DoctorCheckPricing, InventoryConsumptionMap, InventoryItem,
    Operator, OperatorSpecialty,
};
use crate::domains::catalog::domain::services::EffectivePriceQuery;
use crate::domains::catalog::domain::value_objects::CutKind;
use crate::domains::catalog::service::{
    CheckSubtypeCreateInput, CheckSubtypeUpdateInput, CheckTypeCreateInput, CheckTypeUpdateInput,
    ConsumptionCreateInput, ConsumptionUpdateInput, DoctorCreateInput, DoctorPricingUpsertInput,
    DoctorUpdateInput, InventoryItemCreateInput, InventoryItemUpdateInput, OperatorCreateInput,
    OperatorUpdateInput,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn current_actor(state: &AppState) -> AppResult<(Uuid, UserRole, String)> {
    let ctx = state
        .get_current_user()
        .await
        .ok_or(AppError::NotAuthenticated)?;
    let id = Uuid::parse_str(&ctx.user_id)?;
    let role = UserRole::parse(&ctx.role)
        .ok_or_else(|| AppError::Validation(format!("invalid role: {}", ctx.role)))?;
    Ok((id, role, ctx.entity_id))
}

fn catalog(state: &AppState) -> AppResult<crate::domains::catalog::CatalogServices> {
    state
        .catalog_services()
        .ok_or_else(|| AppError::Configuration("catalog services unavailable".into()))
}

// ---- check_types --------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CheckTypesListArgs {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckTypeCreateArgs(pub CheckTypeCreateInput);

#[derive(Debug, Deserialize)]
pub struct CheckTypeUpdateArgs {
    pub id: String,
    #[serde(flatten)]
    pub patch: CheckTypeUpdateInput,
}

#[derive(Debug, Deserialize)]
pub struct CheckTypeToggleArgs {
    pub id: String,
    pub to_value: bool,
    #[serde(default)]
    pub base_price_iqd: Option<i64>,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_list(
    state: State<'_, AppState>,
    args: CheckTypesListArgs,
) -> AppResult<Vec<CheckType>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types
        .list(&entity_id, args.include_inactive, args.query)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_get(state: State<'_, AppState>, args: IdArgs) -> AppResult<CheckType> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types.get(Uuid::parse_str(&args.id)?).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_create(
    state: State<'_, AppState>,
    args: CheckTypeCreateArgs,
) -> AppResult<CheckType> {
    let (id, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types.create(id, role, &entity_id, args.0).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_update(
    state: State<'_, AppState>,
    args: CheckTypeUpdateArgs,
) -> AppResult<CheckType> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types
        .update(uid, role, Uuid::parse_str(&args.id)?, args.patch)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_toggle_subtypes(
    state: State<'_, AppState>,
    args: CheckTypeToggleArgs,
) -> AppResult<CheckType> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types
        .toggle_has_subtypes(
            uid,
            role,
            Uuid::parse_str(&args.id)?,
            args.to_value,
            args.base_price_iqd,
        )
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_types_soft_delete(state: State<'_, AppState>, args: IdArgs) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_types
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- check_subtypes -----------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CheckSubtypesByTypeArgs {
    pub check_type_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckSubtypeCreateArgs(pub CheckSubtypeCreateInput);

#[derive(Debug, Deserialize)]
pub struct CheckSubtypeUpdateArgs {
    pub id: String,
    #[serde(flatten)]
    pub patch: CheckSubtypeUpdateInput,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_subtypes_list_by_type(
    state: State<'_, AppState>,
    args: CheckSubtypesByTypeArgs,
) -> AppResult<Vec<CheckSubtype>> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_subtypes
        .list_by_type(Uuid::parse_str(&args.check_type_id)?)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_subtypes_create(
    state: State<'_, AppState>,
    args: CheckSubtypeCreateArgs,
) -> AppResult<CheckSubtype> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_subtypes
        .create(uid, role, &entity_id, args.0)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_subtypes_update(
    state: State<'_, AppState>,
    args: CheckSubtypeUpdateArgs,
) -> AppResult<CheckSubtype> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_subtypes
        .update(uid, role, Uuid::parse_str(&args.id)?, args.patch)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn check_subtypes_soft_delete(state: State<'_, AppState>, args: IdArgs) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.check_subtypes
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- doctors ------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DoctorsListArgs {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorDetail {
    pub doctor: Doctor,
    pub pricings: Vec<DoctorCheckPricing>,
}

#[derive(Debug, Deserialize)]
pub struct DoctorCreateArgs(pub DoctorCreateInput);

#[derive(Debug, Deserialize)]
pub struct DoctorUpdateArgs {
    pub id: String,
    #[serde(flatten)]
    pub patch: DoctorUpdateInput,
}

#[derive(Debug, Deserialize)]
pub struct DoctorSetActiveArgs {
    pub id: String,
    pub is_active: bool,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_list(
    state: State<'_, AppState>,
    args: DoctorsListArgs,
) -> AppResult<Vec<Doctor>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctors
        .list(&entity_id, args.include_inactive, args.query)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_get(state: State<'_, AppState>, args: IdArgs) -> AppResult<DoctorDetail> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    let (doctor, pricings) = svc
        .doctors
        .get_with_pricings(Uuid::parse_str(&args.id)?)
        .await?;
    Ok(DoctorDetail { doctor, pricings })
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_create(
    state: State<'_, AppState>,
    args: DoctorCreateArgs,
) -> AppResult<Doctor> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctors.create(uid, role, &entity_id, args.0).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_update(
    state: State<'_, AppState>,
    args: DoctorUpdateArgs,
) -> AppResult<Doctor> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctors
        .update(uid, role, Uuid::parse_str(&args.id)?, args.patch)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_set_active(
    state: State<'_, AppState>,
    args: DoctorSetActiveArgs,
) -> AppResult<Doctor> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctors
        .set_active(uid, role, Uuid::parse_str(&args.id)?, args.is_active)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctors_soft_delete(state: State<'_, AppState>, args: IdArgs) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctors
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- doctor_check_pricing ----------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DoctorPricingUpsertArgs {
    pub doctor_id: String,
    pub check_type_id: String,
    #[serde(default)]
    pub check_subtype_id: Option<String>,
    #[serde(default)]
    pub price_override_iqd: Option<i64>,
    pub cut_kind: CutKind,
    pub cut_value: i64,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctor_pricing_upsert(
    state: State<'_, AppState>,
    args: DoctorPricingUpsertArgs,
) -> AppResult<DoctorCheckPricing> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    let parsed = DoctorPricingUpsertInput {
        doctor_id: Uuid::parse_str(&args.doctor_id)?,
        check_type_id: Uuid::parse_str(&args.check_type_id)?,
        check_subtype_id: args
            .check_subtype_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()?,
        price_override_iqd: args.price_override_iqd,
        cut_kind: args.cut_kind,
        cut_value: args.cut_value,
    };
    svc.doctor_pricing
        .upsert(uid, role, &entity_id, parsed)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn doctor_pricing_soft_delete(state: State<'_, AppState>, args: IdArgs) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.doctor_pricing
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

#[derive(Debug, Deserialize)]
pub struct EffectivePriceArgs {
    #[serde(default)]
    pub doctor_id: Option<String>,
    pub check_type_id: String,
    #[serde(default)]
    pub check_subtype_id: Option<String>,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn pricing_effective(
    state: State<'_, AppState>,
    args: EffectivePriceArgs,
) -> AppResult<i64> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.pricing_resolver
        .effective_price(EffectivePriceQuery {
            doctor_id: args.doctor_id.as_deref().map(Uuid::parse_str).transpose()?,
            check_type_id: Uuid::parse_str(&args.check_type_id)?,
            check_subtype_id: args
                .check_subtype_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()?,
        })
        .await
}

// ---- operators ----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OperatorsListArgs {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OperatorDetail {
    pub operator: Operator,
    pub specialties: Vec<OperatorSpecialty>,
}

#[derive(Debug, Deserialize)]
pub struct OperatorCreateArgs(pub OperatorCreateInput);

#[derive(Debug, Deserialize)]
pub struct OperatorUpdateArgs {
    pub id: String,
    #[serde(flatten)]
    pub patch: OperatorUpdateInput,
}

#[derive(Debug, Deserialize)]
pub struct OperatorSetActiveArgs {
    pub id: String,
    pub is_active: bool,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_list(
    state: State<'_, AppState>,
    args: OperatorsListArgs,
) -> AppResult<Vec<Operator>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operators
        .list(&entity_id, args.include_inactive, args.query)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_get(state: State<'_, AppState>, args: IdArgs) -> AppResult<OperatorDetail> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    let (operator, specialties) = svc
        .operators
        .get_with_specialties(Uuid::parse_str(&args.id)?)
        .await?;
    Ok(OperatorDetail {
        operator,
        specialties,
    })
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_create(
    state: State<'_, AppState>,
    args: OperatorCreateArgs,
) -> AppResult<Operator> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operators.create(uid, role, &entity_id, args.0).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_update(
    state: State<'_, AppState>,
    args: OperatorUpdateArgs,
) -> AppResult<Operator> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operators
        .update(uid, role, Uuid::parse_str(&args.id)?, args.patch)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_set_active(
    state: State<'_, AppState>,
    args: OperatorSetActiveArgs,
) -> AppResult<Operator> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operators
        .set_active(uid, role, Uuid::parse_str(&args.id)?, args.is_active)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operators_soft_delete(state: State<'_, AppState>, args: IdArgs) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operators
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- operator_specialties ----------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OperatorSpecialtyUpsertArgs {
    pub operator_id: String,
    pub check_type_id: String,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operator_specialties_upsert(
    state: State<'_, AppState>,
    args: OperatorSpecialtyUpsertArgs,
) -> AppResult<OperatorSpecialty> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operator_specialties
        .upsert(
            uid,
            role,
            &entity_id,
            crate::domains::catalog::service::operator_specialty_service::OperatorSpecialtyInput {
                operator_id: Uuid::parse_str(&args.operator_id)?,
                check_type_id: Uuid::parse_str(&args.check_type_id)?,
            },
        )
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn operator_specialties_soft_delete(
    state: State<'_, AppState>,
    args: IdArgs,
) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.operator_specialties
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- inventory_items ---------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InventoryItemsListArgs {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InventoryItemDetail {
    pub item: InventoryItem,
    pub consumption: Vec<InventoryConsumptionMap>,
}

#[derive(Debug, Deserialize)]
pub struct InventoryItemCreateArgs(pub InventoryItemCreateInput);

#[derive(Debug, Deserialize)]
pub struct InventoryItemUpdateArgs {
    pub id: String,
    #[serde(flatten)]
    pub patch: InventoryItemUpdateInput,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_catalog_list(
    state: State<'_, AppState>,
    args: InventoryItemsListArgs,
) -> AppResult<Vec<InventoryItem>> {
    let (_, _, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.inventory_items
        .list(&entity_id, args.include_inactive, args.query)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_catalog_get(
    state: State<'_, AppState>,
    args: IdArgs,
) -> AppResult<InventoryItemDetail> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    let id = Uuid::parse_str(&args.id)?;
    let item = svc.inventory_items.get(id).await?;
    let consumption = svc.consumption.list_by_item(id).await?;
    Ok(InventoryItemDetail { item, consumption })
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_catalog_create(
    state: State<'_, AppState>,
    args: InventoryItemCreateArgs,
) -> AppResult<InventoryItem> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.inventory_items
        .create(uid, role, &entity_id, args.0)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_catalog_update(
    state: State<'_, AppState>,
    args: InventoryItemUpdateArgs,
) -> AppResult<InventoryItem> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.inventory_items
        .update(uid, role, Uuid::parse_str(&args.id)?, args.patch)
        .await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_catalog_soft_delete(
    state: State<'_, AppState>,
    args: IdArgs,
) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.inventory_items
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

// ---- inventory_consumption_map -----------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConsumptionCreateArgs(pub ConsumptionCreateInput);

#[derive(Debug, Deserialize)]
pub struct ConsumptionUpdateArgs(pub ConsumptionUpdateInput);

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_consumption_create(
    state: State<'_, AppState>,
    args: ConsumptionCreateArgs,
) -> AppResult<InventoryConsumptionMap> {
    let (uid, role, entity_id) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.consumption.create(uid, role, &entity_id, args.0).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_consumption_update(
    state: State<'_, AppState>,
    args: ConsumptionUpdateArgs,
) -> AppResult<InventoryConsumptionMap> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.consumption.update(uid, role, args.0).await
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_consumption_soft_delete(
    state: State<'_, AppState>,
    args: IdArgs,
) -> AppResult<()> {
    let (uid, role, _) = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.consumption
        .soft_delete(uid, role, Uuid::parse_str(&args.id)?)
        .await
}

#[derive(Debug, Deserialize)]
pub struct ConsumptionListByTypeArgs {
    pub check_type_id: String,
}

#[tauri::command]
#[instrument(skip(state, args))]
pub async fn inventory_consumption_list_by_type(
    state: State<'_, AppState>,
    args: ConsumptionListByTypeArgs,
) -> AppResult<Vec<InventoryConsumptionMap>> {
    let _ = current_actor(&state).await?;
    let svc = catalog(&state)?;
    svc.consumption
        .list_by_check_type(Uuid::parse_str(&args.check_type_id)?)
        .await
}
