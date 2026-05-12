//! Patients bounded context (PRD §6.1.9).

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use service::PatientService;
