//! Pure-domain layer for the sync bounded context.
//!
//! Contains entities, value objects, services, and repository ports. No
//! external dependencies beyond serde/chrono/uuid (data primitives).

pub mod entities;
pub mod repositories;
pub mod services;
pub mod value_objects;
