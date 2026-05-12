//! Shifts bounded context (PRD §6.1.8, phase-04).
//!
//! Operator clock-in / clock-out plus superadmin retroactive edit, soft-delete,
//! and overlap detection. Conflict policy: `additive-only` with LWW for
//! updates of the same row (phase-04 §4 + §7.6, §7.9).

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use service::ShiftService;
