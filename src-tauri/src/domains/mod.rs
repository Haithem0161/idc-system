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
pub mod sync;
