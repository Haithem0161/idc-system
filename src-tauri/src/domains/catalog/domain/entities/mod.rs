//! Catalog entities. Each has `try_new`, `with_updated_fields`, and
//! `soft_deleted` semantics; pure data, no I/O.

pub mod check_subtype;
pub mod check_type;
pub mod doctor;
pub mod doctor_pricing;
pub mod inventory_consumption;
pub mod inventory_item;
pub mod operator;
pub mod operator_specialty;

pub use check_subtype::CheckSubtype;
pub use check_type::CheckType;
pub use doctor::Doctor;
pub use doctor_pricing::DoctorCheckPricing;
pub use inventory_consumption::InventoryConsumptionMap;
pub use inventory_item::InventoryItem;
pub use operator::Operator;
pub use operator_specialty::OperatorSpecialty;
