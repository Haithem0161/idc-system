//! Bounded contexts.
//!
//! Each subdirectory is a self-contained domain following the DDD layout:
//!
//! ```text
//! domains/<name>/
//! ├── domain/           # entities, services, repository traits (no deps)
//! ├── infrastructure/   # SQLite repos, HTTP clients, jobs
//! └── commands.rs       # #[tauri::command] handlers
//! ```

pub mod _example;
pub mod audit;
pub mod auth;
pub mod catalog;
pub mod inventory;
pub mod patients;
pub mod receipts;
pub mod reports;
pub mod settings;
pub mod shifts;
pub mod sync;
pub mod visits;
