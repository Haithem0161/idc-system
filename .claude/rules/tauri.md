---
paths:
  - "src-tauri/**"
  - "**/tauri.conf.json"
  - "**/capabilities/**"
  - "**/*.rs"
---

# Tauri v2 Rules

The desktop app is **Tauri v2**, running as a standalone window app. The Rust side owns local persistence (SQLite), background sync, and the IPC surface to the frontend.

## Core Principles

1. **Context7 First (MANDATORY).** Before writing code that uses any Tauri plugin, Tokio API, Axum extractor, sea-orm/sqlx feature, or any other crate -- query Context7 (`resolve-library-id`, then `query-docs`) and base the implementation on the returned docs. Do not rely on memorized patterns.
2. **Capabilities are deny-by-default.** Every new Tauri API or plugin used by the frontend MUST be added to a capability file in `src-tauri/capabilities/`. If a capability isn't declared, the call fails. Never broaden a scope wider than the screens that use it.
3. **CSP is strict.** `tauri.conf.json` `security.csp` must remain a strict policy. Never add `'unsafe-inline'` for scripts. Inline styles are tolerated only when shadcn/Tailwind requires them.
4. **No emojis** in code, comments, commit messages, or user-facing strings (use i18n keys + locale files).
5. **Package management:** never edit `Cargo.toml` `[dependencies]` by hand -- use `cargo add <crate>`. `[workspace]`, `[profile]`, `[features]`, `[patch]` are the only sections you may hand-edit.

## Project Layout

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── capabilities/
│   └── default.json          # Allowlist of plugin APIs by window
├── icons/
└── src/
    ├── main.rs               # thin entry: lib::run()
    ├── lib.rs                # Tauri builder setup, command registration
    ├── state.rs              # AppState (RwLock auth + Arc<Db>)
    ├── error.rs              # AppError + IntoResponse + Tauri error mapping
    ├── config.rs             # typed config from env / app data dir
    ├── db/                   # SQLite connection, migrations runner
    ├── domains/<name>/
    │   ├── domain/           # entities, services, repository traits
    │   ├── infrastructure/   # SQLite repos, sync adapters
    │   └── commands.rs       # #[tauri::command] handlers
    └── sync/                 # sync engine: queue, pusher, puller, conflict resolver
```

Each domain mirrors the DDD layout in `ddd.md` -- domain layer is dependency-free, infrastructure implements the traits, presentation here means Tauri commands.

## Tauri Commands

| Rule | Detail |
|-|-|
| One handler per file isn't required, but one **module** per domain is | All commands for a domain live in `domains/<name>/commands.rs` and are re-exported from `lib.rs`. |
| Always typed | Use `serde::Deserialize` for inputs, `serde::Serialize` for outputs. Never accept or return `serde_json::Value` outside debugging. |
| Errors implement `Serialize` | Wrap `AppError` with `impl Serialize` so the frontend gets a typed `{ kind, message }` payload, not a stringified Rust panic. |
| Async by default | Commands that touch the DB or network MUST be `async fn`. Use `tauri::State<'_, AppState>` for dependencies. |
| Validate at the boundary | Re-validate inputs with the domain's `try_new()` constructor even if the frontend ran Zod -- the frontend is untrusted. |
| Idempotency on writes | Mutations that may be retried (sync, slow IPC) accept a client-generated `op_id` and dedupe by it. |
| Register | Every new command MUST be added to `tauri::Builder::default().invoke_handler(tauri::generate_handler![...])` in `lib.rs`. |
| No long-running work | Commands return quickly; spawn long jobs onto `tokio::spawn` and emit progress with Tauri events (`app_handle.emit("progress", ...)`). |

## Capabilities

- File `src-tauri/capabilities/default.json` is the baseline; create per-window capability files when scopes diverge.
- Granting `fs:default` or `shell:default` is FORBIDDEN. Use scoped permissions (`fs:allow-read-text-file` with a `path` scope, `shell:allow-execute` with a `name` allowlist).
- Plugins must be enabled in `tauri.conf.json` AND added to a capability file.
- When a capability change touches a release, document it in the phase file's "Infrastructure Updates" section.

## State and Concurrency

- `AppState` is wrapped in `Arc<>` and shared via `tauri::State`.
- Mutable fields use `tokio::sync::RwLock`. Never `std::sync::Mutex` across `await` points.
- Database handle is a single `Arc<sqlx::SqlitePool>` (or equivalent), opened once at startup.
- Use a `CancellationToken` (from `tokio-util`) to coordinate graceful shutdown of background tasks.
- The sync engine runs as a background task spawned at startup; it must stop cleanly when the app closes.

## Logging

- Use `tracing`, never `println!` or `log::*` directly. The template ships `tracing-subscriber`.
- One `#[instrument]` per command at minimum (`skip(state)` to avoid logging the whole AppState).
- Log levels: `error!` for unrecoverable issues, `warn!` for retryable failures, `info!` for lifecycle events, `debug!` for SQL/queue traces.
- The `tauri-plugin-log` is wired but `tracing` is the source of truth on the Rust side.

## Build & Release

- Dev: `pnpm tauri dev` (starts Vite + Rust). Use `cargo check` from `src-tauri/` for fast Rust feedback during dev.
- Release: `pnpm tauri build`. Verify the bundle starts on a clean profile (no leftover `app.db`).
- Release pipeline lives in `.github/workflows/` (multi-platform: linux-x86_64, windows-x86_64, macos-x86_64, macos-aarch64).

## Common Pitfalls

- Holding a `tokio::sync::Mutex` guard across an `await` will deadlock under load -- prefer `RwLock`, or scope the lock and clone the value.
- `tauri::generate_handler!` is a macro: a missing entry compiles fine but fails at runtime with `Command not found`. Add new commands here BEFORE testing from the frontend.
- `tauri.conf.json` schema validation runs on `tauri build` only -- always run a build after editing.
- The Tauri webview cannot reach `localhost:<port>` unless that port is whitelisted in `tauri.conf.json` `app.security.dangerousDisableAssetCspModification` -- prefer IPC commands over local HTTP.
