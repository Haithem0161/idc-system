//! sqlx-backed catalog repositories. Each module owns one aggregate.

mod check_subtype_repo;
mod check_type_repo;
mod common;
mod doctor_pricing_repo;
mod doctor_repo;
mod inventory_consumption_repo;
mod inventory_item_repo;
mod mandoub_repo;
mod operator_repo;
mod operator_specialty_repo;

pub use check_subtype_repo::SqliteCheckSubtypeRepo;
pub use check_type_repo::SqliteCheckTypeRepo;
pub use doctor_pricing_repo::SqliteDoctorPricingRepo;
pub use doctor_repo::SqliteDoctorRepo;
pub use inventory_consumption_repo::SqliteInventoryConsumptionRepo;
pub use inventory_item_repo::SqliteInventoryItemRepo;
pub use mandoub_repo::SqliteMandoubRepo;
pub use operator_repo::SqliteOperatorRepo;
pub use operator_specialty_repo::SqliteOperatorSpecialtyRepo;
