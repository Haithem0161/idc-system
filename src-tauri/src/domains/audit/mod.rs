//! Audit bounded context (phase-08).
//!
//! Hosts the audit query service (merge-paginator + role gate), the daily
//! vacuum job, the diagnostics summary (PRD §1.3 success metrics), and the
//! Tauri commands that expose them.

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use service::{
    AuditQueryService, AuditVacuumJob, AuditVacuumOutcome, DiagnosticsService, DiagnosticsSummary,
};
