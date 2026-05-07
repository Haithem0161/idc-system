//! Migration runner.
//!
//! Applies SQL files from `src-tauri/migrations/NNN_<name>.sql` in order on
//! app start. Tracks state in a `_migrations` table. Forward-only, idempotent.
