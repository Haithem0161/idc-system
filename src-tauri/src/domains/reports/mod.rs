//! Accounting & Reports bounded context (phase-07).
//!
//! Reports are read-only query services over the snapshot columns persisted
//! by Phase 5's lock workflow. All money aggregations read from
//! `visits.*_snapshot_iqd` exclusively (PRD §4.1): the snapshot is the
//! historical truth, never the live catalog price.
//!
//! State machine: none. There is no entity to mutate; the only state
//! transition this domain owns is the `audit_log` row emitted by
//! `daily_close` runs (phase-07 §7.18).

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use service::{ReportsService, ReportsServiceConfig};
