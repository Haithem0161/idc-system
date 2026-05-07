//! Tauri command handlers for this domain.
//!
//! Each `#[tauri::command] async fn` validates input, calls a domain service,
//! and maps errors to the typed `AppError`. Register every command in
//! `lib.rs::run()` via `tauri::generate_handler!`.
