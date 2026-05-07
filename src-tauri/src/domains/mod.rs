//! Bounded contexts.
//!
//! Each subdirectory is a self-contained domain following the DDD layout
//! described in `.claude/rules/ddd.md`:
//!
//! ```text
//! domains/<name>/
//! ├── domain/           # entities, services, repository traits (no deps)
//! ├── infrastructure/   # SQLite repos, sync adapters
//! └── commands.rs       # #[tauri::command] handlers
//! ```
//!
//! Copy `_example/` to start a new domain.

pub mod _example;
