---
paths:
  - "**/Cargo.toml"
  - "**/*.rs"
  - "src-tauri/**"
---

# Rust Rules (Tauri Backend)

This file covers Rust conventions for `src-tauri/`. Tauri-specific rules (capabilities, commands, dual-mode) live in `tauri.md`. Sync-engine rules live in `offline-first.md`.

## Mandatory Rules

1. **Context7 First.** Before writing implementation code with any crate -- `tokio`, `axum` (embedded mode only), `sqlx` / `rusqlite`, `serde`, `tracing`, `thiserror`, `reqwest`, `tauri`, etc. -- query Context7 (`resolve-library-id`, then `query-docs`) and base the code on the returned docs.
2. **Pre-commit Validation.** `cargo clippy -- -D warnings`, `cargo build`, `cargo test` MUST pass before every commit. There is no CI safety net for "clippy failures are fine".
3. **Package management:** never hand-edit `Cargo.toml` `[dependencies]` -- use `cargo add <crate>` (with `--features` as needed). Hand-editing is OK only for `[workspace]`, `[profile]`, `[features]`, and `[patch]`.
4. **No Claude authorship in commits.** Same as the rest of the project.
5. **Edition:** Rust 2021 (matching the existing template). Bump only as a deliberate decision in a phase file.
6. **No emojis** anywhere.

## Core Stack

| Crate | Purpose |
|-|-|
| `tauri` | Desktop framework. |
| `tauri-plugin-log` | Frontend-side log forwarding (paired with `tracing`). |
| `tokio` | Async runtime (full features). |
| `axum`, `tower-http` | HTTP server for embedded mode (Business OS integration). |
| `serde`, `serde_json`, `rmp-serde` | Serialization (JSON for IPC, MessagePack for embedded IPC + outbox). |
| `sqlx` (preferred) or `rusqlite` | Local SQLite. Pick one and stick to it -- do not mix. |
| `tracing`, `tracing-subscriber` | Structured logging. |
| `thiserror` | Typed errors at module boundaries. |
| `anyhow` | Application-level glue (top-level handlers only). |
| `chrono` | UTC timestamps for sync columns. |
| `uuid` (`v7` feature) | Time-sortable IDs for syncable entities. |
| `reqwest` | HTTP client to the sync server (with `rustls-tls`). |
| `tokio-util` | `CancellationToken`, `TaskTracker` for graceful shutdown. |

Never add a crate without first checking whether the desired functionality already lives in `tauri` or `tokio`.

## Project Structure (DDD)

Mirrors `sync-server.md` adapted for Rust modules. See `tauri.md` for the layout.

- **Domain** (`domains/<name>/domain/`): pure structs, methods, repository traits, errors. ZERO external deps -- no `tauri`, no `sqlx`, no `reqwest`.
- **Infrastructure** (`domains/<name>/infrastructure/`): `sqlx` repository impls, sync adapters, background workers.
- **Presentation** (`domains/<name>/commands.rs`): `#[tauri::command]` handlers (boundary layer). Validate inputs, call domain, map errors.

## Error Handling

- One `AppError` enum at `src/error.rs` (already exists in template). Variants for each error category (`Db`, `Sync`, `Auth`, `Validation`, `NotFound`, `Conflict`, `Internal`).
- Use `thiserror` for the enum, `anyhow` only inside `main.rs` / `lib.rs::run()` glue.
- `AppError` MUST implement `serde::Serialize` so Tauri commands return typed errors. Never return raw `String` errors to the frontend.
- Map crate-specific errors with `From` impls or `.map_err(...)`. Never use `unwrap` / `expect` in non-test code except where the invariant is statically guaranteed and documented.
- For SQLite constraint conflicts (`UNIQUE`, `FK`), match on the SQL state and convert to `AppError::Conflict` -- do not bubble raw `sqlx::Error`.

## Async and Concurrency

- All I/O is async. Use `tokio::spawn` for background work; track handles with `TaskTracker` so shutdown waits for them.
- `tokio::sync::RwLock` for shared state across `await` points; never `std::sync::Mutex` across awaits.
- `tokio::sync::mpsc` for fan-in pipelines (UI events -> sync engine -> server).
- Cooperative cancellation: every long-running task accepts a `CancellationToken` clone and checks `is_cancelled()` at safe points.
- Use `tokio::select!` for "either work or shutdown" loops.

## Database (SQLite)

- One pool: `Arc<sqlx::SqlitePool>` opened in `db/mod.rs::init()`.
- Connection options: `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`, `busy_timeout = 5000`.
- Migrations live in `src-tauri/migrations/NNN_<name>.sql`. The runner applies them in order on app start; migration state is tracked in a `_migrations` table. Migrations are idempotent and forward-only.
- Use prepared statements (`sqlx::query!` / `sqlx::query_as!`) -- they get compile-time checking against the DB schema. Run `cargo sqlx prepare` in CI/dev to refresh `.sqlx/` metadata.
- Wrap multi-step writes in `pool.begin().await?` transactions. Commit local FIRST, then dispatch network work.

## Logging

- `tracing` only -- no `println!`, no direct `log::*`. The subscriber is initialized in `lib.rs::run()`.
- `#[instrument]` on every command and every public service method. `skip(state, db, ...)` to avoid logging large structs.
- Levels: `error!` unrecoverable, `warn!` retryable, `info!` lifecycle, `debug!` per-request / per-row, `trace!` SQL bind params.
- NEVER log JWTs, passwords, full payloads with PII at `info!`. Gate detailed payload logs behind a `debug!` + a feature flag for development.

## Testing

- Unit tests live in the same file as the code they test (`#[cfg(test)] mod tests`).
- Integration tests live in `src-tauri/tests/`. Sync-engine tests get their own subdirectory (`tests/sync/`).
- Use `tokio::test` for async tests.
- For DB tests, open an in-memory SQLite (`sqlite::memory:`), run migrations, then exercise repositories.
- Mock HTTP with `mockito` or `wiremock` -- never hit the real sync server in tests.
- `cargo nextest run` is preferred over `cargo test` (faster, better output) when available.

## Commands and Style

- Run from `src-tauri/` directory:
  - `cargo check` -- fastest signal during dev.
  - `cargo clippy --all-targets -- -D warnings` -- mandatory before commit.
  - `cargo fmt` -- run before commit.
  - `cargo test` (or `cargo nextest run`).
  - `cargo build --release` -- only when actually building a release.
- Format style: rustfmt defaults; do not introduce a custom `rustfmt.toml` without a phase decision.
- Module visibility: prefer `pub(crate)` over `pub`. Only the public API of a domain crosses module boundaries.
- Naming: snake_case for files and functions, PascalCase for types, SCREAMING_SNAKE_CASE for constants.

## Common Pitfalls

- `tokio::sync::Mutex` guard held across an `await` deadlocks under load -- prefer `RwLock` or scope the lock and clone out the value.
- `sqlx` macro errors at compile time when `.sqlx/` is stale -- regenerate with `cargo sqlx prepare`.
- `tauri::generate_handler!` does not error when a command is missing -- the runtime fails with `Command not found`. Always update `lib.rs` when adding commands.
- `chrono::Utc::now()` returns `DateTime<Utc>` -- serialize as RFC3339 (`.to_rfc3339()`) for sync columns.
- Building on macOS aarch64 vs x86_64: features may differ -- the release pipeline is the source of truth, not local builds.
