# IDC System v0.1.x Development Plan

**Start date:** 2026-05-11
**Target:** Ship PRD-V0.1.0 (V0.1.1 draft) across the Tauri desktop app and the Fastify sync/backup server. Single-site Iraqi medical imaging center; bilingual (ar default + en); offline-first.
**Source PRD:** [PRD-V0.1.0.md](./PRD-V0.1.0.md)

## Scope (Hard Numbers)

| Metric | v0.1.0 Target |
|-|-|
| Syncable entities | 15 (plus `outbox`, `sync_state`) |
| Pages | 29 (per PRD §3.2) |
| Tauri IPC commands | ~55 (per PRD §5.1) |
| Sync-server routes | 10 (per PRD §5.2) |
| Modules | 5 (Reception, Accounting, Inventory, Admin, Audit) plus Auth/system |
| Locales | 2 (ar default + en) |
| Conflict policies in use | 3 (`last-write-wins`, `additive-only`, `manual`) |
| Local audit retention | 90 days |
| Server audit retention | indefinite |

## Phase Overview Table

| # | Phase Name | Surfaces | Scope | Size | Depends On | Status |
|-|-|-|-|-|-|-|
| 01 | Foundation & Sync Plumbing | All | `outbox`, `sync_state`, `audit_log`; sync engine push/pull/conflict mechanism; tenant scoping; Tauri lifecycle; app shell; sync-server bootstrap (Fastify + JWT + tenant plugin + Swagger); `/healthz`, `/sync/push`, `/sync/pull`, `/sync/conflicts/:opId/resolve`. | L | None | not_started |
| 02 | Authentication & Users | All | `users` and `settings`; `/auth/*` routes; offline login via Argon2id-cached hash; lock screen; Login + No-Access pages; Admin Users CRUD + password reset; Admin Settings page. | L | 01 | not_started |
| 03 | Catalog & Reference Data | All | `check_types`, `check_subtypes`, `doctors` (+FTS5), `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`; Admin module shell; admin list+detail pages for each. | XL | 02 | not_started |
| 04 | Operator Shifts | Frontend, Tauri | `operator_shifts` (additive-only); `/reception/shifts` page; clock-in / clock-out / retroactive edit commands. | S | 03 | not_started |
| 05 | Reception & Visit Lock | All | `patients` (+FTS5), `visits` (manual conflict), `inventory_adjustments`; Reception module pages; lock workflow (snapshot + operator + inventory + audit + receipt) inside one SQLite tx; void workflow; A5 PDF + thermal text receipts. | XL | 04 | not_started |
| 06 | Inventory Operations | All | Operational layer on `inventory_adjustments`; Inventory list/detail/adjust pages; receive/writeoff/count_correction commands; quantity recompute. | M | 05 | not_started |
| 07 | Accounting & Reports | All | Accounting dashboard + 4 reports (Visits / Doctors / Operators / Daily Close); CSV export; server `/reports/visits` and `/reports/daily-close/:date` endpoints. | L | 06 | not_started |
| 08 | Audit, Conflict Resolver & Polish | All | `/audit` page; `/sync/conflicts` resolver UI; `GET /audit/query` server endpoint; audit vacuum job; i18n/RTL final sweep; soak + performance verification. | M | 07 | not_started |
| 09 | Pre-Ship Hardening (Sync Server Persistence + Cleanup) | All | Wire `PrismaSyncStore` and `PrismaUserStore` against the existing 19-model schema (Phases 1-8 ran on in-memory Maps); add `Dockerfile.dev` + `docker-compose.yaml` + `init-custom-sql.sql`; enforce RS256 (no `dev-only-secret` fallback in production); real `/healthz` Postgres + Redis probes; audit-log row on manual conflict resolution; fix `.env.template` schema mismatch (`JWT_PUBLIC_KEY_PATH` -> `JWT_PUBLIC_KEY`, add `BOOTSTRAP_*` / `METRICS_TOKEN` / `DEFAULT_ENTITY_ID`); replace raw `Error` throws with `DomainError` in auth refresh path; frontend `console.log` and MVP-placeholder cleanup; `unreachable!()` -> `AppError::Internal` in inventory service; stale "phase-04" comment removal in operator service. | L | 08 | not_started |

## Dependency Graph

```
+---------+   +---------+   +---------+   +---------+
| Phase 1 |-->| Phase 2 |-->| Phase 3 |-->| Phase 4 |
+---------+   +---------+   +---------+   +---------+
                                              |
                                              v
+---------+   +---------+   +---------+   +---------+
| Phase 8 |<--| Phase 7 |<--| Phase 6 |<--| Phase 5 |
+---------+   +---------+   +---------+   +---------+
     |
     v
+---------+
| Phase 9 |   pre-ship hardening (server persistence + cleanup)
+---------+
```

Each phase contains parallel tracks for the three surfaces (Frontend / Tauri-Rust / Sync Server). Within a phase the tracks run concurrently after the schema migration files land; cross-track integration is verified in the phase's §6 Verification step.

## New Local Entities by Phase (SQLite)

| Phase | New Local Tables / Virtual Tables |
|-|-|
| 01 | `outbox`, `sync_state`, `audit_log` |
| 02 | `users`, `settings` |
| 03 | `check_types`, `check_subtypes`, `doctors`, `doctors_fts`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map` |
| 04 | `operator_shifts` |
| 05 | `patients`, `patients_fts`, `visits`, `inventory_adjustments` |
| 06 | (no new tables; full ops on `inventory_adjustments` from 05) |
| 07 | (no new tables; reports read from existing snapshot columns) |
| 08 | (no new tables; vacuum and resolver only) |

Total new local tables at v0.1.0 ship: 17 base tables + 2 FTS5 virtual tables + `outbox` + `sync_state` = 21 SQLite objects (15 syncable business entities plus `audit_log` plus 2 FTS + 2 engine tables).

## New Server Entities by Phase (Prisma)

| Phase | New Prisma Models |
|-|-|
| 01 | `AuditLog`, `ProcessedOp` (server-only idempotency), `SyncCursor` (server-only per-device cursor), `ConflictParked` (server-only stash for manual conflicts) |
| 02 | `User` (+ `UserRole` enum), `Setting` (+ `SettingType` enum) |
| 03 | `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing` (+ `CutKind` enum), `Operator`, `OperatorSpecialty`, `InventoryItem`, `InventoryConsumptionMap` |
| 04 | `OperatorShift` |
| 05 | `Patient`, `Visit` (+ `VisitStatus` enum), `InventoryAdjustment` (+ `AdjustmentReason` enum) |
| 06 | (no new models) |
| 07 | (no new models) |
| 08 | (no new models) |

Server-only models (`ProcessedOp`, `SyncCursor`, `ConflictParked`) are not in PRD §6 because they are sync infrastructure rather than domain entities; they live on the server only and are documented in Phase 1.

## New Business Engines by Phase

| Phase | Frontend | Tauri / Rust | Sync Server |
|-|-|-|-|
| 01 | `SyncStatusStore` (Zustand), `useSyncStatus`, `useSyncConflicts` (hooks), `<AppShell>`, `<SyncPill>` | `SyncEngine` (Tokio task), `OutboxRepo`, `PullCursorRepo`, `ConflictDispatcher`, `AuditWriter::with_audit`, `device_id` boot | `SyncPushService`, `SyncPullService`, `ConflictResolveService`, `TenantPlugin`, `JwtPlugin`, `OpDeduper` |
| 02 | `<LoginForm>`, `<LockScreen>`, `<IdleWatcher>`, `useAuth`, `useCurrentUser`, `useSettings` | `AuthService` (online + offline), `StrongholdCredsCache`, `JwtVerifier`, `UserService`, `SettingsService`, `IdleTimer` | `AuthService` (login/refresh/logout/change-password), `UserService`, `SettingsService` |
| 03 | `<AdminShell>` (sub-sidebar), `<CheckTypeForm>`, `<DoctorPricingEditor>`, `<OperatorSpecialtyPicker>`, `<ConsumptionMapEditor>` | `CheckTypeService`, `CheckSubtypeService`, `DoctorService`, `DoctorPricingService`, `OperatorService`, `OperatorSpecialtyService`, `InventoryItemService`, `InventoryConsumptionMapService` | Same service set on the server (Prisma-backed) |
| 04 | `<ShiftsPage>`, `<ClockInDialog>`, `useOpenShifts`, `useShiftHistoryToday` | `ShiftService` (clock_in / clock_out / list_open / history_today / edit), `OpenShiftGuard` | `ShiftService` |
| 05 | `<ChecksGrid>`, `<CheckWorkspace>`, `<NewVisitForm>`, `<OperatorPicker>`, `<VisitDetail>` (with Audit/Receipts tabs), `<VoidModal>`, `<ReceiptPreview>` | `PatientService`, `VisitService::create`/`update`/`discard`/`lock`/`void`, `MoneyMath`, `OperatorEligibility`, `InventoryConsumer`, `ReceiptGenerator` (A5 PDF + thermal) | `PatientService`, `VisitService`, `InventoryAdjustmentService` (consume_visit acceptance) |
| 06 | `<InventoryList>`, `<ItemDetail>` (Overview/Map/Adjustments/Audit), `<AdjustForm>` | `InventoryAdjustmentService` (operations: receive/writeoff/count_correction), `QuantityRecomputer` | `InventoryAdjustmentService` |
| 07 | `<AccountingDashboard>`, `<VisitsReport>`, `<DoctorEarnings>`, `<OperatorEarnings>`, `<DailyClose>`, `<CsvExportButton>` | `ReportsService` (locals only), `CsvWriter`, `DailyCloseGenerator` | `ReportsService` (cross-90-day aggregates), `DailyCloseSigner` (deferred to Horizon 1) |
| 08 | `<AuditPage>`, `<ConflictResolver>`, `<DeltaViewer>`, i18n lint, soak harness | `AuditQueryService` (local + remote fallback), `AuditVacuumJob` (daily) | `AuditQueryService`, `AuditVacuumJob` (no-op on server; server keeps indefinitely) |

## Sync Contracts by Phase

| Phase | Entity | Push Payload Shape | Pull Behaviour | Conflict Policy | Idempotency Key |
|-|-|-|-|-|-|
| 01 | (engine plumbing; no domain entities yet) | n/a | n/a | n/a | `outbox.op_id` (UUID v7) |
| 02 | `users` | full row except `password_hash` (server-canonical) | LWW apply | `last-write-wins` | `op_id` |
| 02 | `settings` | full row | 409 on conflict | `manual` | `op_id` |
| 03 | `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map` | full row | LWW apply | `last-write-wins` | `op_id` |
| 04 | `operator_shifts` | full row | append; never overwrite | `additive-only` | `op_id` |
| 05 | `patients` | full row | LWW apply | `last-write-wins` | `op_id` |
| 05 | `visits` | full row including all snapshots | 409 on `manual` conflict | `manual` | `op_id` |
| 05 | `inventory_adjustments` | full row | append; never overwrite | `additive-only` | `op_id` |
| 06 | (operational only; same `inventory_adjustments` contract) | full row | append | `additive-only` | `op_id` |
| 07 | (no push contracts; reports read locally and pull server aggregates via `/reports/*`) | n/a | n/a | n/a | n/a |
| 08 | `audit_log` | full row | append | `additive-only` | `op_id` (audit rows are also pushed via outbox) |

## Cumulative IPC Command and Route Targets

| Phase | New IPC Commands | New Sync-Server Routes | Cumulative IPC | Cumulative Routes |
|-|-|-|-|-|
| 01 | 0 domain commands; engine emits `sync:*` events | `/healthz`, `/sync/push`, `/sync/pull`, `/sync/conflicts/:opId/resolve` | 0 | 4 |
| 02 | ~12 (`auth::*`, `users::*`, `settings::*`, `lock::trigger`) | `/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password` | ~12 | 8 |
| 03 | ~16 (catalog services) | 0 (catalog flows through `/sync/push`) | ~28 | 8 |
| 04 | 5 (`shifts::*`) | 0 | ~33 | 8 |
| 05 | ~12 (`patients::*`, `visits::*`, `receipts::*`) | 0 | ~45 | 8 |
| 06 | ~5 (`inventory::*`) | 0 | ~50 | 8 |
| 07 | ~5 (`reports::*`, `daily_close::run`, `export::csv`) | `/reports/visits`, `/reports/daily-close/:date` | ~55 | 10 |
| 08 | 0 (audit reuses query commands; resolver uses Phase-1 endpoint) | `/audit/query` | ~55 | 11 |

Aggregate IPC count (~67) exceeds PRD §5.1's `~55` lower bound by virtue of fine-grained admin CRUD plus first-launch UX, telemetry / diagnostics, and resolver-conflict commands added by Pass-1+Pass-2+Pass-3 §7.x. Sync-server route count (11 incl. `/audit/query`) matches PRD §5.2 (10 + `/healthz`). Status.md line 18 reflects the actual cumulative totals.

## Gap Analysis Additions

Pass 1 completed 2026-05-11 against PRD-V0.1.0 (V0.1.1 draft). 119 gaps logged across all phases as Section 7.x subsections. Zero CRITICAL gaps in Pass 1; the bulk are MEDIUM completeness gaps in service-method specs, UI element enumeration, and cross-surface validation symmetry.

Pass 2 completed 2026-05-11. Six parallel sub-agents re-validated the Pass-1 §7.x additions and probed Pass-1 focus areas commonly missed: state-machine completeness (visits/shifts), field completeness (snapshot columns, server `pulledAt`), sync-contract symmetry (delete-vs-edit, tiebreaks, idempotency replay, additive enforcement), cross-phase wiring (handshakes, orphaned IPC/components), reports/accounting drill-down, and cross-cutting infrastructure (first-launch UX, telemetry, a11y, capabilities, error envelopes). 88 new gaps logged plus 2 amendments to existing Pass-1 §7.x content (phase-02 §7.1 corrected `value_type='integer'` → `'int'` to satisfy the CHECK constraint; phase-08 §1 audit action union extended from 10 to 12 values to match phase-01 §7.8). 5 CRITICAL gaps found (all schema/state-machine integrity: outbox `op` enum dead-code, doctor-pricing NULL uniqueness flaw, inventory_adjustments mutability trigger, visit illegal-transition matrix, first-launch superadmin bootstrap UX). Pass 2 hot spots: phase-05 (20 gaps; orphaned IPC/component receipts forwarded from earlier phases), phase-01 (15 gaps; sync-engine corners and cross-cutting infra), phase-07 (14 gaps; groupBy/drill-down/role gating).

Pass 3 completed 2026-05-11 (final pass). Six parallel sub-agents covered: (A) sync-engine corners + Pass-2-noted focus areas (Prisma migration ordering, i18n key inventory completeness, `metrics_events` pruning); (B) entities §6.1.1-§6.1.8 field completeness; (C) entities §6.1.9-§6.1.15 field completeness; (D) workflows §4 / §8 / §10; (E) module specs §7 (page UI elements + role gating); (F) cross-cutting + verification of 15 representative §7.x items. 47 new gaps logged across all eight phase files. 2 CRITICAL gaps found (Prisma `User` and `Operator` models missing `OperatorShift` back-relations -- `prisma generate` would fail). Pass 3 hot spots: phase-05 (8 sections, 9 gaps -- name-snapshot Prisma symmetry + telemetry emission + UI affordances), phase-01 (7 sections, 7 gaps -- audit-log carve-out + telemetry emission + embedded-mode gating + missing `daily_close_run` enum), phase-03 (7 sections, 8 gaps -- back-relations + migration ordering + FTS soft-delete filter). The 4 HIGH "missing role-gate" gaps (E-6, E-7, E-8, E-9) cluster around route-level `<RequireRole>` wrappers that were declared but never wired at any module's outlet; fixed in phase-03 §7.36, phase-05 §7.58, phase-06 §7.13, phase-07 §7.28, phase-08 §7.23. Verification: 15/15 representative §7.x spot-checks pass.

| Pass | Date | Gaps Found | Critical | High | Medium | Low | Status |
|-|-|-|-|-|-|-|-|
| 1 | 2026-05-11 | 119 | 0 | 38 | 61 | 20 | complete |
| 2 | 2026-05-11 | 88  | 5 | 33 | 36 | 14 | complete |
| 3 | 2026-05-11 | 47  | 2 | 11 | 19 | 15 | complete |

### Pass-1 Distribution by Phase

| Phase | Gaps | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing  | 14 | 0 | 6 | 5  | 3 |
| 02 Authentication & Users      | 16 | 0 | 7 | 8  | 1 |
| 03 Catalog & Reference Data    | 18 | 0 | 4 | 9  | 5 |
| 04 Operator Shifts             |  7 | 0 | 2 | 2  | 3 |
| 05 Reception & Visit Lock      | 30 | 0 | 9 | 18 | 3 |
| 06 Inventory Operations        |  8 | 0 | 2 | 5  | 1 |
| 07 Accounting & Reports        | 13 | 0 | 5 | 6  | 2 |
| 08 Audit, Conflict Resolver    | 13 | 0 | 3 | 8  | 2 |
| **Total**                      | 119| 0 | 38| 61 | 20|

### Pass-2 Distribution by Phase

| Phase | Gaps | Critical | High | Medium | Low | New §7.x range |
|-|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing  | 15 | 1 | 3 | 7 | 4 | §7.15 - §7.29 |
| 02 Authentication & Users      | 12 | 1 | 8 | 3 | 0 | §7.17 - §7.28 (plus §7.1 corrected) |
| 03 Catalog & Reference Data    | 11 | 1 | 3 | 6 | 1 | §7.19 - §7.29 |
| 04 Operator Shifts             |  5 | 0 | 2 | 3 | 0 | §7.8  - §7.12 |
| 05 Reception & Visit Lock      | 20 | 2 |10 | 6 | 2 | §7.31 - §7.50 |
| 06 Inventory Operations        |  4 | 0 | 2 | 2 | 0 | §7.9  - §7.12 |
| 07 Accounting & Reports        | 14 | 0 | 3 | 6 | 5 | §7.14 - §7.27 |
| 08 Audit, Conflict Resolver    |  7 | 0 | 2 | 3 | 2 | §7.14 - §7.20 (plus §1 enum corrected) |
| **Total**                      | 88 | 5 |33 |36 |14 |               |

### Pass-1 Distribution by Category

| Category | Count | Notes |
|-|-|-|
| Missing UI Element                 | 14 | Mostly column/filter/component enumeration in admin and accounting pages |
| Missing Business Rule              | 10 | Service-layer enforcement of PRD invariants (XOR rules, parent-state guards, soft-delete blocks) |
| Missing Service Method             |  9 | Explicit method specs (create/update/soft_delete) where only one path was documented |
| Missing Validation                 |  9 | Cross-surface symmetry (server re-validates what Rust does, and vice versa) |
| Missing Logic                      |  8 | Workflow gaps in PRD §8 (pricing/settings banners, daily-close breakdowns, tz boundary) |
| Missing IPC Command                |  7 | `receipts::print_*`, `settings::list_printers`, `sync::outbox_count`, `shifts::lines_run_today`, drafts list |
| Missing Constraint                 |  6 | SQLite CHECK constraints for snapshot invariants and per-reason delta signs |
| Wrong Order / Audit-first Ordering |  3 | PRD §4.3 audit-first violations in `with_audit`, lock workflow, void workflow |
| Missing Concurrency Guard          |  3 | Operator-eligibility TOCTOU, clock-out race, single-open-shift sync conflict |
| Missing Plugin / Capability        |  3 | tauri-plugin-fs, fs:scope for logs, printer capability |
| Missing A11y Requirement           |  3 | WCAG 2.1 AA baseline, aria-label keys, a11y verification step |
| Missing Setup                      | 10 | i18n scaffolding, formatters, AppState construction, JWT pin, redaction layer |
| Missing Setting Key                |  2 | `thermal_width`, `thermal_printer_name` |
| Missing Index                      |  5 | (entity_id, is_active), drafts index, audit tenant index, history-today, server Prisma `Visit` |
| Missing Component                  |  6 | `<RequireRole>`, `<CheckTypeForm>`, `<ReceiptPreview>`, `<DirtyDot>`, `<StatusBar>`, `<Breadcrumbs>` |
| Missing Page                       |  2 | `/` root redirect (phase-02), accounting read-only visit detail contract (phase-05/07) |
| Missing Route                      |  2 | `GET /sync/conflicts`, `GET /auth/jwks` (JWT public key endpoint) |
| Missing Snapshot Column            |  1 | Human-readable name snapshots on `visits` (`patient_name_snapshot`, etc.) |
| Missing Sync Rule                  |  1 | LWW tiebreak re-stated per entity |
| Missing Audit Trigger              |  4 | Consumption-map audits, void-offset `by_user_id`, recompute audit, settings audit |
| Missing Retention Policy           |  1 | `AuditRepo::vacuum_unsynced_safe` API change |
| Missing Vacuum Job                 |  1 | Missed-run handling on audit vacuum |
| Mismatched Path / Naming           |  3 | `lock::trigger` vs `auth::lock`; LockError names; tab naming PRD §3.3 vs §7.1.4 |
| Incomplete Coverage                |  6 | Lines-run wiring, void-monetary aggregation, daily-close hash, JWT pinning |
| Missing UI Behavior                |  2 | Click-to-resolver, no-phantom-toasts policy |
| Missing Sweep                      |  2 | i18n lint implementation, pre-phase-08 enforcement |
| Type Mismatch                      |  1 | Money snapshot Prisma `Int` vs Rust `i64` |
| Missing Banner / Pill              |  3 | Sync-pill badge, settings-changed banner, pricing-changed banner |
| Missing Enum Value                 |  1 | `audit_action` extensions documented |

### Notes on Pass-1 Approach

- Pass 1 was executed by six parallel sub-agents, each scoped to a slice of the PRD (entities §6.1.1-7, §6.1.8-11, §6.1.12-15; modules §7; workflows §4/§8; cross-cutting §5/§9/§10). Each returned a structured gap list; this file synthesizes the merged inventory.
- A small number of agent-flagged items were dropped during synthesis (no real gap): TENANT_MODELS final-state assertion (already incremental per phase), some forward-reference Prisma relations (intentionally deferred), and a Patient `min length 2` "stricter than PRD" comment that wraps around to a fix (now in phase-05 §7.9).

### Notes on Pass-2 Approach

- Pass 2 was executed by six parallel sub-agents on non-overlapping slices: (1) §6.1.1-7 entities and auth, (2) §6.1.8-15 entities and reception workflows, (3) sync contracts and conflict resolution across all phases, (4) cross-phase wiring and §7.x consistency review, (5) accounting and reports, (6) cross-cutting (i18n/a11y/security/capabilities/telemetry/error envelopes). Six structured gap lists were merged and de-duplicated; ~15 overlapping findings collapsed (e.g., name-snapshot columns flagged by both the entity-completeness and cross-phase-wiring agents).
- Pass 2 confirmed the 119 Pass-1 §7.x sections are correctly indexed in the per-phase counts. Two existing §7.x entries had wrong content and were amended in place (not new gaps, corrected gaps): phase-02 §7.1 (settings-seed `value_type` literal) and phase-08 §1 (audit action enum). Both fixes preserve the original gap numbering.
- All 5 Pass-2 CRITICALs trace to schema/state-machine integrity issues that Pass 1 missed because they probed the PRD entity catalogue but not the cross-cutting invariants (outbox enum dead code, NULL-distinct uniqueness flaws in Postgres, mutation triggers on append-only tables, illegal-transition exhaustiveness, first-launch bootstrap).
- A Pass 3 may still uncover gaps, but the planning rule "continue until 0 true gaps" applies. Recommended Pass 3 focus areas if scheduled: (a) Server Prisma migration ordering across phases (raw-SQL migrations added in §7.20/§7.21/§7.33 need a documented order vs `prisma migrate`), (b) i18n key inventory verification (phase-03 §7.29 catalogue against actual `errors:*` keys emitted by each domain error variant in source), (c) the new `metrics_events` table's pruning semantics under the shared 90-day vacuum (phase-01 §7.28 vs phase-08 §4 — does the vacuum touch this table or only `audit_log`?).

### Pass-3 Distribution by Phase

| Phase | Gaps | Critical | High | Medium | Low | New §7.x range |
|-|-|-|-|-|-|-|
| 01 Foundation & Sync Plumbing  |  7 | 0 | 1 | 5 | 1 | §7.30 - §7.36 |
| 02 Authentication & Users      |  5 | 1 | 0 | 1 | 3 | §7.29 - §7.33 |
| 03 Catalog & Reference Data    |  8 | 1 | 2 | 3 | 0 (combined into 7 sections, §7.30 - §7.36; GAP-B-2 + GAP-B-3 merged into §7.30) | §7.30 - §7.36 |
| 04 Operator Shifts             |  4 | 0 | 1 | 2 | 1 | §7.13 - §7.16 |
| 05 Reception & Visit Lock      |  9 | 0 | 4 | 3 | 2 (combined into 8 sections, §7.51 - §7.58) | §7.51 - §7.58 |
| 06 Inventory Operations        |  3 | 0 | 2 | 0 | 1 | §7.13 - §7.15 |
| 07 Accounting & Reports        |  3 | 0 | 1 | 1 | 1 | §7.28 - §7.30 |
| 08 Audit, Conflict Resolver    |  6 | 0 | 1 | 2 | 3 | §7.21 - §7.26 |
| **Total**                      | 47 | 2 |11 |19 |15 |               |

### Pass-3 Notes

- Pass 3 was executed by six parallel sub-agents on non-overlapping slices: (A) sync engine + Pass-2 deferred focus areas, (B) entities §6.1.1-§6.1.8 field completeness, (C) entities §6.1.9-§6.1.15 field completeness, (D) workflows §4/§8/§10, (E) module specs §7 (UI + role gates), (F) cross-cutting + verification spot-checks.
- The 2 CRITICAL gaps both reflect Prisma relation graph integrity: phase-04 declared named back-relations on `User` and `Operator` but the inverse fields were missing, so `prisma generate` would have failed at the very first server build. Fixed by phase-02 §7.29 and phase-03 §7.30 (combined fix).
- The 11 HIGH gaps cluster into three families: (1) server `pulledAt` symmetry across all syncable models (phase-01 §7.32, phase-04 §7.13, phase-05 §7.52); (2) raw-SQL migration ordering documentation (phase-03 §7.31, phase-05 §7.51); (3) frontend route-level role gates that backend gating left exposed at the URL layer (phase-03 §7.36, phase-04 §7.16, phase-05 §7.58, phase-06 §7.13, phase-07 §7.28, phase-08 §7.23). Plus the missing visit name-snapshot CHECK + Prisma columns (phase-05 §7.52, §7.53), the `inventory_adjustments` per-reason CHECK as a concrete server raw migration (phase-06 §7.14), and the missing lock-time + receipt-print telemetry emission (phase-05 §7.54).
- Verification spot-check: 15/15 representative items from Pass-1+Pass-2 §7.x verified by Agent F as having complete schema/IPC/service/sync rule. Cross-cutting `ErrorResponseSchema` (phase-01 §7.26) + `AppError` (§7.27) consistency holds across all consumers.
- No Pass 4 needed. With 0 true new gaps remaining (all 47 logged + fixed within this pass), the dev plan is ready for implementation per planning.md §Verification Pass.

Per phase, Section 7.x subsections capture each gap with severity, category, target phase, and resolution.

## Maintenance

- `roadmap.md` is updated when phase scope changes (rare). Section 8 grows as gap analysis runs.
- `status.md` is updated after every phase or significant milestone within a phase.
- `frontend-summary.md` is updated after EACH phase, never batched.
- Verification reports (`PHASES-X-Y-Z-VERIFICATION.md`) are created on demand by Verification Passes, not as part of normal development.
