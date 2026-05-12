//! Catalog bounded context: reference data that drives the visit form.
//!
//! Owns 8 syncable entities:
//! - `check_types`, `check_subtypes`
//! - `doctors`, `doctor_check_pricing`
//! - `operators`, `operator_specialties`
//! - `inventory_items`, `inventory_consumption_map`
//!
//! All entities use `last-write-wins` conflict resolution. The doctors table
//! has an FTS5 virtual sibling (`doctors_fts`) maintained via triggers.

pub mod commands;
pub mod domain;
pub mod events;
pub mod infrastructure;
pub mod service;

pub use service::CatalogServices;
