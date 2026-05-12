//! Visits bounded context (PRD §6.1.10, §6.1.14).

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use service::VisitService;
