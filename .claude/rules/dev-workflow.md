---
paths:
  - "**"
---

# Development Workflow

This is the canonical rhythm for any change in this repo. Follow it from the moment you pick up a task to the moment you push.

## The 10-Step Loop

1. **Study the Plan.** Read the relevant `docs/<plan-name>/phase-XX.md`. Confirm goals, surfaces (Frontend / Tauri / Sync Server), success criteria.
2. **Research & Documentation (MANDATORY).** Query Context7 MCP for every library/framework/plugin you'll touch. Read existing patterns in this repo (look at sibling features). Do not write code from memory.
3. **Design Schema.** Write SQL migrations for the local SQLite layer and Prisma model edits for the server. Plan migrations as forward-only and idempotent.
4. **Implement Domain Layer.** Entities, value objects, services, repository interfaces. Pure logic, no I/O.
5. **Implement Infrastructure.** Repository implementations (sqlx for Tauri, Prisma for server), sync adapters, jobs, IPC bridges.
6. **Implement Presentation.**
   - Tauri: `#[tauri::command]` handlers + register in `lib.rs`.
   - Sync server: routes with full Swagger schemas.
   - Frontend: pages, components, queries, mutations, schemas.
7. **Run the App.** `pnpm tauri dev`. Smoke-test the new screens. For sync-server work also run its compose target.
8. **Sync Round-Trip Test.** For any syncable change: create a record offline, reconnect, confirm it lands on the server; create a record on the server (curl MCP), confirm the client pulls it; force a conflict and confirm the declared policy is enforced.
9. **Pre-Push Validation (MANDATORY).** Run the equivalent of `./tools/pre-push-check.sh`:
   - `pnpm lint` -- ESLint passes.
   - `pnpm build` -- TS + Vite build passes.
   - `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
   - When the sync server exists: its lint, typecheck, and unit tests.
10. **Fix and Iterate.** Failures cascade in this order: schema -> runtime -> validation -> Swagger/IPC contract completeness. Fix root causes; never bypass with `--no-verify`.

## Tooling Choices

- **Always use `pnpm`.** Never `npm` / `yarn`.
- **Tauri dev:** `pnpm tauri dev` (NOT plain `pnpm dev` -- that runs Vite without the Rust side).
- **Tauri build:** `pnpm tauri build`.
- **Frontend-only work:** `pnpm dev` is fine, but verify in the Tauri webview before declaring done.
- **Rust feedback loop:** `cargo check` from `src-tauri/` is the fastest signal; reach for `cargo clippy` before commits.

## Git Hygiene

- **NEVER commit with Claude authorship or co-authorship.** No `Co-Authored-By: Claude`, no Anthropic emails, no `git config` changes. Commits appear as solely human-made.
- **Commit message style:** present-tense imperative. Subject line under 72 chars. Body explains *why*, not *what*. Group commits by logical change, not by file.
- **Branch naming:** `feat/<short-slug>`, `fix/<short-slug>`, `chore/<short-slug>`, `docs/<short-slug>`.
- **NEVER push without pre-push validation passing.** Lint, typecheck, unit tests are mandatory; integration tests that need a network or DB locally may fail -- those are the CI responsibility.

## Package Installation

- **NEVER hand-edit `package.json` dependency sections.** Use `pnpm add <pkg>` / `pnpm add -D <pkg>` / `pnpm remove <pkg>`.
- **NEVER hand-edit `Cargo.toml` `[dependencies]`.** Use `cargo add <crate>` (with `--features` as needed).
- After adding npm packages used inside Docker (sync server): `docker compose up -d --force-recreate -V <service>` to bust the anonymous-volume cache.

## Context7 Documentation Lookup (MANDATORY)

Before writing ANY implementation code using a library, plugin, or framework:
1. Call `resolve-library-id` to find the library.
2. Call `query-docs` with your specific use case.
3. Use the returned docs/examples as the basis for the implementation.

This applies to: Tauri plugins, Tokio APIs, sqlx / rusqlite, Axum extractors, Fastify plugins, Prisma, BullMQ, TypeBox, undici, React 19 features, React Router v7, TanStack Query, Zustand, Zod, framer-motion, react-i18next, shadcn/ui patterns -- any library, regardless of how familiar it feels.

## HTTP Testing Tools (Sync Server)

- All HTTP requests in dev/test MUST go through `mcp__curl__*` tools. Bash `curl` is acceptable only when MCP curl lacks a feature (e.g., specific multipart/binary edge cases).
- Auth flow:
  1. `mcp__curl__curl_post` to `/auth/login` with `{ email, password }`.
  2. Use returned `accessToken` in `Authorization: Bearer <token>` header for subsequent calls.
  3. Token expires in 15m -- re-login on 401.

## Subagent Rules

When launching subagents (Agent tool), include relevant rule content directly in the prompt. Subagents do NOT auto-load `.claude/rules/`. For Tauri work, paste the relevant sections of `tauri.md`, `rust.md`, `offline-first.md`. For server work, paste `sync-server.md`, `ddd.md`, `auth.md`. Always include the "no Claude authorship" and "Context7 first" rules.

## Common Pitfalls (Cross-Surface)

- Forgetting to register a new Tauri command in `lib.rs::generate_handler!` -- compiles fine, fails at runtime.
- Adding a syncable model server-side without updating the local SQLite schema (or vice-versa) -- breaks the round trip.
- Editing Tauri capabilities without rebuilding -- caches can mask the change.
- Holding a SQLite write transaction across an HTTP call -- locks the WAL and stalls every other writer.
- Skipping the conflict-resolution declaration in a phase file -- the engine has nothing to dispatch on.
- Letting `pnpm dev` mask a Tauri-only bug -- always verify in `pnpm tauri dev` before declaring done.
