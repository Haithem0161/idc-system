---
paths:
  - "docs/**"
  - "**/roadmap.md"
  - "**/phase-*.md"
  - "**/status.md"
  - "**/frontend-summary.md"
  - "**/*VERIFICATION*"
---

# Development Plan Writing Rules

All development plans follow the V3.6 phased pattern. Plans live in `docs/<plan-name>/` and consist of 6 mandatory files. The IDC system is **two surfaces** that must be planned jointly: the Tauri offline-first desktop app (React + Rust) and the Fastify sync/backup server. Every phase declares which surfaces it touches.

## Plan Structure

| File | Purpose |
|-|-|
| `roadmap.md` | Master blueprint: phase table, dependency graph, entity/engine inventories, gap analysis log |
| `research.md` | Domain research (regulations, algorithms, sync semantics, formats), decisions log with date/decision/rationale |
| `phase-XX.md` | Individual phase specs -- the core deliverable (see template below) |
| `status.md` | Living tracker: phase status table, cumulative totals, blockers |
| `frontend-summary.md` | Cross-team handoff -- updated after EACH phase completion, never batched |
| `PHASES-X-Y-Z-VERIFICATION.md` | Verification reports with YAML frontmeta (score, status, per-truth pass/fail) |

## Roadmap.md Sections (in order)

1. **Header** -- Title, start date, target description, scope with hard numbers (entities, screens, IPC commands, sync endpoints, reports).
2. **Phase Overview Table** -- Columns: #, Phase Name, Surfaces (Frontend/Tauri/Server/All), Scope, Size (S/M/L/XL), Depends On, Status.
3. **Dependency Graph** -- ASCII art showing phase relationships and parallel tracks across surfaces.
4. **New Local Entities by Phase** -- SQLite tables in the Tauri app, per phase.
5. **New Server Entities by Phase** -- Prisma models on the sync server, per phase.
6. **New Business Engines by Phase** -- Domain services per surface (frontend, Rust local, server).
7. **Sync Contracts by Phase** -- Push/pull payloads, event topics, and conflict-resolution rules added per phase.
8. **Gap Analysis Additions** -- Running log updated after each pass (count, categories, distribution across phases).

## Phase File Template

Every `phase-XX.md` MUST have these sections in this exact order:

### Header
```
# Phase N: <Name>

**Goal:** <One sentence describing what this phase delivers>

**Surfaces:** Frontend | Tauri/Rust | Sync Server | All
**Dependencies:** Phase X, Phase Y (or "None")
**Complexity:** S | M | L | XL
```

### Section 1: Local Schema Changes (Tauri SQLite)
- New tables: full SQL `CREATE TABLE` blocks with PK, FK, indexes, `created_at`/`updated_at`/`deleted_at`/`sync_version` columns.
- Modified tables: column-by-column additions with exact types and constraints.
- New enums: stored as `TEXT CHECK (col IN (...))` constraints.
- Migration file name: `migrations/NNN_<name>.sql` (idempotent where possible).

### Section 2: Server Schema Changes (Prisma / Postgres)
- New models: full Prisma model blocks, copy-paste ready, with `@map`, `@db.Timestamptz`, `@default(uuid())`, indexes.
- Modified models: field-by-field additions.
- New enums: values listed explicitly.
- Sync columns: `lastSyncedAt`, `tombstone`, `version`, `originDeviceId` where applicable.

### Section 3: DDD Implementation
For EACH surface that changes, document:

**Frontend (React)**
- New pages/routes (`| Path | File | Description |`).
- Zustand stores added or extended.
- React Query keys + hooks (`use<Entity>List`, `use<Entity>Detail`, mutation hooks).
- Zod schemas in `src/lib/schemas/`.

**Tauri/Rust**
- Domain entity: struct definition, constructor validation (`fn try_new()`), methods with signatures.
- Repository trait: method signatures (in `domain/repositories/`).
- SQLite repository: notes on prepared statements, transactions.
- Tauri `#[tauri::command]` table (`| Command | Args | Returns | Description |`).

**Sync Server (Fastify)**
- Entity class: properties, constructor validation, `toResponse()`, new methods with signatures.
- Repository interface: method signatures with parameter types.
- Prisma repository: notes on includes/joins.
- Schemas (TypeBox): list of schema names and their purpose.
- Route table: `| Method | Path | Description |` format.

### Section 4: Business Logic
- Service classes/structs with method signatures (per surface).
- Configuration JSON examples (stored in entity-level settings).
- Step-by-step logic for each method (numbered steps, not prose).
- Sync semantics: which entities push, which pull, conflict-resolution policy (LWW / merge / manual), idempotency key.

### Section 5: Infrastructure Updates
- TENANT_MODELS additions on the server (or "No new entries needed").
- Audit trigger additions (or "No new triggers needed").
- Local SQLite indexes added.
- Tauri `capabilities/` permission changes (allowlists, dialog/fs/path scopes).
- New Tauri plugin registrations.
- New Fastify plugins or queues (BullMQ).

### Section 6: Verification
Numbered checklist of concrete steps:
1. `cd src-tauri && cargo clippy -- -D warnings` -- no lint errors.
2. `cd src-tauri && cargo test` -- all tests pass.
3. `pnpm lint && pnpm build` -- frontend builds cleanly.
4. `pnpm tauri dev` -- desktop app boots; smoke-test the new screens.
5. `cd sync-server && pnpm nx test <service>` -- server tests pass (when sync server exists).
6. Sync round-trip: create record offline -> reconnect -> server has it; create on server -> client pulls it.
7. Conflict scenario: edit same record on two clients while offline -> reconnect both -> conflict resolved per policy with no data loss.
8. Specific functional tests (create X, verify Y).
9. Run existing tests -- no regressions.

### Section 7+: PRD Gap Additions
Appended by gap analysis passes. Numbered subsections (7.1, 7.2, ...) each containing:
- Field additions with exact SQL/Prisma types.
- New IPC commands or HTTP routes.
- Business logic additions.
- Reference to gap ID and severity.

---

## Gap Analysis Methodology (Mandatory)

Iterative passes are REQUIRED. Do not mark a plan "ready for implementation" until a verification pass finds 0 true gaps.

### Pass 1 (Initial)
After phase files are written, compare EVERY PRD entity, field, screen, IPC command, HTTP endpoint, business rule, state machine, sync contract, and integration point against phase specs. Log each gap with:
- Severity: CRITICAL / HIGH / MEDIUM / LOW
- Category: Missing Local Table, Missing Server Model, Missing Fields, Missing IPC Command, Missing Endpoint, Missing Logic, Missing Sync Rule, Missing Conflict Policy, Missing Report, Missing Setup, Missing Integration, Missing Dashboard, Incomplete Coverage
- Target phase for incorporation.

Append gaps to respective phase files as Section 7.x subsections. Update `roadmap.md` gap log.

### Pass 2+ (Iterative)
Re-compare updated phase files against PRD. Focus areas that passes commonly miss:
- State machines (transition tables, type-specific rules).
- Field completeness (compare every PRD field against schema, both local and server).
- Sync contracts (push/pull symmetry, deletion handling, ordering guarantees).
- Conflict resolution (every entity must declare a policy).
- Integration points (events published/consumed, notification triggers).
- Setup/config screens.
- Report drill-down and dynamic grouping params.

Continue passes until a pass finds 0 true gaps.

### Verification Pass (Final)
Audit N representative items across all phases (mix of Critical/High/Medium/Low). For each item, verify it exists in the phase files with:
- Complete local SQL schema (if local).
- Complete Prisma schema (if server).
- Route or IPC command table entry.
- Service method signature.
- Business logic description.
- Sync rule declared.

Report as YAML frontmatter in `PHASES-X-Y-Z-VERIFICATION.md`:
```yaml
---
phase: <plan-name>-phases-X-Y-Z
verified: <ISO timestamp>
status: complete | gaps_found
score: N/M must-haves verified
gaps: [...]
---
```

## Status.md Sections

1. **Phase Status Table** -- Columns: #, Phase, Surfaces, Status, Started, Completed, Local Tables Added, Server Models Added, IPC Commands Added, Routes Added, Services Added.
2. **Cumulative Totals** -- Columns: Metric, Before, Current, Target.
3. **Gap Analysis Summary** -- Per-pass summary with counts and category breakdown.
4. **Blockers & Notes** -- Critical dependencies, blocking items, parallel track notes.

## Rules

- SQL and Prisma schemas MUST be copy-paste ready -- exact field names, types, constraints, relations.
- IPC command tables MUST use `| Command | Args | Returns | Description |` format.
- Route tables MUST use `| Method | Path | Description |` format.
- Service specs MUST include method signatures and numbered step-by-step logic.
- Each phase MUST state what it does NOT touch ("No new TENANT_MODELS entries", "No new Tauri capabilities", "No new sync contracts").
- Verification steps MUST be concrete (specific commands, specific assertions), never vague ("verify it works").
- Frontend summary MUST be updated after EACH phase, not batched at the end.
- Phase sizes: S (<5 IPC commands or routes), M (5-15), L (15-25), XL (25+ or any new sync engine).
- Gap severity: CRITICAL = blocks other phases, HIGH = missing core functionality, MEDIUM = missing enhancement, LOW = nice-to-have.
- Every entity that syncs MUST have a declared conflict-resolution policy in its phase file.
