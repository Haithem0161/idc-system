# IDC System

**Offline-first desktop app** built on a Torch Tauri v2 template, paired with a **Fastify sync/backup server**.
Tech Stack: Tauri v2 | React 19 | TypeScript | Rust | SQLite | Fastify | Prisma | PostgreSQL | Tailwind v4 | shadcn/ui | Vite | pnpm

## Surfaces

This repo is two surfaces planned and reasoned about together:

| Surface | Where | Purpose |
|-|-|-|
| **Frontend** | [src/](src/) | React 19 UI -- runs inside the Tauri webview (and inside Business OS embedded mode). |
| **Tauri / Rust** | [src-tauri/](src-tauri/) | Local runtime, SQLite persistence, IPC commands, sync engine, embedded HTTP server. |
| **Sync Server** | `sync-server/` (when introduced) | Fastify + Prisma + Postgres. Sync push/pull, backups, exports. **Not** a general-purpose API for the frontend. |

The desktop app is the source of truth for the user's day-to-day workflow. The server exists for sync, backup, and cross-device collaboration.

## Core Principles

1. **Offline-First.** Every read goes to local SQLite; every write commits locally first; the sync engine ships changes later. A network outage NEVER breaks the user's workflow. See [`.claude/rules/offline-first.md`](.claude/rules/offline-first.md).
2. **Plugin-First (sync server).** Use Fastify plugins for everything; don't reinvent the wheel.
3. **DDD Everywhere.** Each domain is a bounded context with isolated domain logic. See [`.claude/rules/ddd.md`](.claude/rules/ddd.md).
4. **Comprehensive Swagger (sync server).** Every server route has a TypeBox schema with description, tags, summary, body, response, security.
5. **No Emojis.** Never in code, comments, docs, commit messages, or user-facing strings.
6. **Context7 First.** NEVER write code without first querying Context7 for up-to-date docs on every library/plugin/framework being used. Mandatory, not optional.
7. **Always pnpm.** Never `npm` or `yarn`.

## Critical Rules

### Git Commits
**NEVER commit with Claude authorship or co-authorship.** No `Co-Authored-By: Claude`, no Anthropic emails, no modifying git config. All commits must appear as solely human-made.

### Pre-Push Validation (MANDATORY)
**NEVER push without local validation passing.** This mirrors what CI runs:
- `pnpm lint` -- ESLint passes.
- `pnpm build` -- TS + Vite build passes.
- `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- When the sync server exists: its lint, typecheck, and unit tests.

### Destructive Actions
The PreToolUse hook in `.claude/hooks/block-destructive.sh` blocks the most dangerous commands, but the rule stands regardless:
- **NEVER** `docker rm`, `docker compose rm`, `docker system prune`, `docker container prune`, `docker volume prune`, `docker image prune`.
- **NEVER** `git push --force` to `main`, `git reset --hard` shared branches, `git branch -D`, `git filter-branch`.
- **NEVER** `--no-verify` or `--no-gpg-sign` on commits.

### Context7 Documentation Lookup (MANDATORY)
Before writing ANY implementation code using a library/plugin/framework:
1. Call `resolve-library-id` to find the library.
2. Call `query-docs` with your specific use case.
3. Use the returned docs/examples as the basis for the implementation.

This applies to: Tauri plugins, Tokio, sqlx/rusqlite, Axum, Fastify plugins, Prisma, BullMQ, TypeBox, undici, jsonwebtoken, React 19, React Router v7, TanStack Query, Zustand, Zod, framer-motion, react-i18next, shadcn/ui -- any library, no matter how familiar.

### Package Installation
**NEVER hand-edit `package.json` dependency sections.** Use `pnpm add <pkg>` / `pnpm add -D <pkg>` / `pnpm remove <pkg>`.
**NEVER hand-edit `Cargo.toml` `[dependencies]`.** Use `cargo add <crate>` (with `--features` as needed). Hand-edit only `[workspace]`, `[profile]`, `[features]`, `[patch]`.

## Development Workflow

1. **Study the Plan.** Read the relevant `docs/<plan-name>/phase-XX.md`. Identify surfaces touched.
2. **Research & Documentation (MANDATORY).** Query Context7 for every library/plugin you'll touch. Read existing patterns in this repo.
3. **Design Schema.** Local SQLite migration (`src-tauri/migrations/`). Server Prisma model edit. Idempotent, forward-only.
4. **Implement Domain Layer.** Entities, value objects, services, repository interfaces. Pure logic.
5. **Implement Infrastructure.** SQLite repos (Rust), Prisma repos (server), sync adapters, jobs.
6. **Implement Presentation.** Tauri commands + register in `lib.rs`. Server routes with Swagger. Frontend pages, queries, mutations.
7. **Run the App.** `pnpm tauri dev`. Smoke-test new screens.
8. **Sync Round-Trip.** Create offline -> reconnect -> server has it. Server change -> client pulls. Conflict scenario per declared policy.
9. **Pre-Push Validation (MANDATORY).** Lint, typecheck, build, clippy, cargo test. Catches failures locally instead of in CI.
10. **Fix and Iterate.** Schema -> runtime -> validation -> Swagger/IPC contract gaps. Fix root causes.
11. **Update `status.md` (MANDATORY).** Before committing, update `docs/<plan-name>/status.md`: flip the phase row to `complete` with started/completed dates, refresh Cumulative Totals (tables/models/IPC commands/routes/pages/conflict policies/locales), and append a phase-completion note under "Blockers & Notes". Do the same on partial progress (`in_progress`). Never let `status.md` drift behind the code.

See [`.claude/rules/dev-workflow.md`](.claude/rules/dev-workflow.md) for the full loop.

## Frontend Cheatsheet

| Concern | Tool | Detail |
|-|-|-|
| Routing | React Router v7 (`createBrowserRouter`) | [`REACT-ROUTER.md`](REACT-ROUTER.md) |
| Server state | TanStack React Query v5 | [`REACT-QUERY.md`](REACT-QUERY.md) |
| Client state | Zustand v5 | [`ZUSTAND.md`](ZUSTAND.md) |
| Validation | Zod v4 | [`ZOD.md`](ZOD.md) |
| Styling | Tailwind v4 + shadcn/ui | [`TAILWIND.md`](TAILWIND.md), [`SHADCN.md`](SHADCN.md) |
| Design system | Editorial visual language (tokens, components) | [`.claude/rules/design-system.md`](.claude/rules/design-system.md) |
| i18n | react-i18next (en + ar with RTL) | [`I18N.md`](I18N.md) |
| Animations | framer-motion | [`FRAMER-MOTION.md`](FRAMER-MOTION.md) |
| HTTP | axios (typed instance) | [`AXIOS.md`](AXIOS.md) |
| SEO/meta | @dr.pogodin/react-helmet | [`REACT-HELMET.md`](REACT-HELMET.md) |
| Path alias | `@/` -> `src/` | configured in vite.config.ts + tsconfig.app.json |

Detailed conventions: [`.claude/rules/frontend.md`](.claude/rules/frontend.md).

## Tauri Cheatsheet

| Concern | Tool | Detail |
|-|-|-|
| Commands | `#[tauri::command]` async fns | [`src-tauri/TAURI.md`](src-tauri/TAURI.md) |
| Async runtime | Tokio | [`src-tauri/TOKIO.md`](src-tauri/TOKIO.md) |
| Embedded HTTP | Axum 0.8 | [`src-tauri/AXUM.md`](src-tauri/AXUM.md) |
| Serialization | serde + serde_json + rmp-serde | [`src-tauri/SERDE.md`](src-tauri/SERDE.md) |
| Errors | thiserror | [`src-tauri/THISERROR.md`](src-tauri/THISERROR.md) |
| Logging | tracing + tracing-subscriber | [`src-tauri/TRACING.md`](src-tauri/TRACING.md) |
| Business OS | embedded mode IPC | [`src-tauri/BUSINESS-OS-INTEGRATION.md`](src-tauri/BUSINESS-OS-INTEGRATION.md) |

Detailed conventions: [`.claude/rules/tauri.md`](.claude/rules/tauri.md), [`.claude/rules/rust.md`](.claude/rules/rust.md).

## Sync Server Cheatsheet (when introduced)

| Concern | Tool |
|-|-|
| Framework | Fastify + plugin autoload |
| ORM | Prisma + Postgres |
| Validation | TypeBox schemas |
| Auth | RS256 JWT, plugin-based |
| Background jobs | BullMQ |
| Docs | Swagger UI at `/documentation` |
| Container | Docker compose + Dockerfile.dev with auto schema sync |

Detailed conventions: [`.claude/rules/sync-server.md`](.claude/rules/sync-server.md), [`.claude/rules/auth.md`](.claude/rules/auth.md), [`.claude/rules/docker.md`](.claude/rules/docker.md).

## Key Commands

```bash
pnpm install                # install JS deps
pnpm tauri dev              # run desktop app (frontend + Rust)
pnpm tauri build            # build production bundle
pnpm dev                    # frontend-only Vite dev (verify in tauri dev before declaring done)
pnpm build                  # frontend type-check + Vite build
pnpm lint                   # ESLint check

# Rust (run from src-tauri/)
cargo check                 # fastest signal
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
cargo add <crate>           # NEVER hand-edit [dependencies]
```

## Detailed Rules (auto-loaded by path)

Architecture details, patterns, and conventions live in `.claude/rules/`:

- [`planning.md`](.claude/rules/planning.md) -- Plan structure, phase template, gap analysis methodology.
- [`prd-writing.md`](.claude/rules/prd-writing.md) -- PRD section template, file naming, quality bar, anti-patterns.
- [`offline-first.md`](.claude/rules/offline-first.md) -- Sync engine, conflict resolution, local-schema invariants.
- [`tauri.md`](.claude/rules/tauri.md) -- Tauri v2 commands, capabilities, dual-mode, build/release.
- [`frontend.md`](.claude/rules/frontend.md) -- React 19, Vite, Tailwind v4, state architecture.
- [`design-system.md`](.claude/rules/design-system.md) -- Editorial visual language: color tokens, typography, components, motion, RTL conventions.
- [`rust.md`](.claude/rules/rust.md) -- Rust conventions for the Tauri backend.
- [`sync-server.md`](.claude/rules/sync-server.md) -- Fastify, Prisma, sync endpoints, Swagger.
- [`ddd.md`](.claude/rules/ddd.md) -- Domain-Driven Design layout across all surfaces.
- [`auth.md`](.claude/rules/auth.md) -- Offline-first JWT auth, token storage, refresh.
- [`docker.md`](.claude/rules/docker.md) -- Docker rules for the sync-server stack.
- [`dev-workflow.md`](.claude/rules/dev-workflow.md) -- The 10-step development loop.
- [`testing.md`](.claude/rules/testing.md) -- Test plan structure, 5-layer pyramid, 8 edge categories, coverage gates, perf SLOs, snapshot rules, DoD.

## Subagent Rules

When launching subagents (Agent tool), include relevant rule content directly in the agent prompt -- subagents do NOT auto-load `.claude/rules/`. For Tauri work include `tauri.md`, `rust.md`, `offline-first.md`. For server work include `sync-server.md`, `ddd.md`, `auth.md`. For frontend / UI work include `frontend.md`, `design-system.md`. Always include "no Claude authorship" and "Context7 first".

## Common Pitfalls

- Forgetting to register a new Tauri command in `lib.rs::generate_handler!` -- compiles fine, fails at runtime.
- Adding a syncable model server-side without updating the local SQLite schema (or vice-versa) -- breaks the round trip.
- Storing tokens in `localStorage` -- bypasses Rust's secure-storage path. Always go through IPC.
- Holding a SQLite write transaction across an HTTP call -- locks WAL and stalls other writers.
- Skipping the conflict-resolution declaration in a phase file -- the engine has nothing to dispatch on.
- Letting `pnpm dev` mask a Tauri-only bug -- always verify in `pnpm tauri dev`.
- Using `tokio::sync::Mutex` across an `await` -- prefer `RwLock`, or scope the lock and clone the value out.
- Schema validation errors in TypeBox -- nullable fields use `Type.Union([T, Type.Null()])`, not `Type.Optional()`.

<!-- MEMORY:START -->
# Menu

_Last updated: 2026-05-14 | 0 active memories, 0 total_

_For deeper context, use memory_search, memory_related, or memory_ask tools._
<!-- MEMORY:END -->
