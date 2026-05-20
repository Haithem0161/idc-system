# Tauri Backend - CLAUDE.md

This directory contains the Rust/Tauri backend for the desktop application.

## Overview

Tauri v2 desktop wrapper running as a standalone window app.

## Commands

```bash
pnpm tauri dev              # Run Tauri app in development (starts frontend + Rust backend)
pnpm tauri build            # Build production Tauri app bundle
cargo check                 # Type-check Rust code (run from src-tauri/)
cargo clippy                # Lint Rust code
cargo test                  # Run Rust tests
```

## Tech Stack

- **Framework:** Tauri v2 (`tauri` 2.10.0)
- **Async Runtime:** Tokio (full features)
- **HTTP Server:** Axum 0.8
- **Serialization:** Serde + serde_json + rmp-serde (MessagePack)
- **Error Handling:** thiserror 2
- **Logging:** tracing + tracing-subscriber (env-filter)
- **Language:** Rust 2021 edition

## Architecture

```
src-tauri/
├── Cargo.toml              # Rust dependencies
├── tauri.conf.json         # Tauri config (window, CSP, bundling)
├── capabilities/
│   └── default.json        # Permission declarations
├── build.rs                # Tauri build script
├── icons/                  # App icons (all platforms)
└── src/
    ├── main.rs             # Entry point → lib::run()
    ├── lib.rs              # Tauri builder setup, command registration
    ├── state.rs            # AppState: RwLock<token, user> for thread-safe auth
    └── error.rs            # AppError enum + AppResult<T> alias
```

## Key Configuration

- **tauri.conf.json** - App identifier, window config, CSP, build commands, bundling
- **Cargo.toml** - Rust dependencies and crate metadata
- **capabilities/default.json** - Tauri permission system (core:default)

## Reference Guides

- [TAURI.md](TAURI.md) - Tauri v2 framework patterns (commands, events, plugins, windows, capabilities)
- [SERDE.md](SERDE.md) - Serialization with serde, serde_json, and rmp-serde (MessagePack)
- [TOKIO.md](TOKIO.md) - Async runtime (tasks, channels, signals, sync primitives, I/O)
- [AXUM.md](AXUM.md) - HTTP server (routing, extractors, state, static files, graceful shutdown)
- [THISERROR.md](THISERROR.md) - Error handling patterns with thiserror
- [TRACING.md](TRACING.md) - Structured logging with tracing and tracing-subscriber

<!-- MEMORY:START -->
# src-tauri

_Last updated: 2026-05-20 | 0 active memories, 0 total_

_For deeper context, use memory_search, memory_related, or memory_ask tools._
<!-- MEMORY:END -->
