# IDC System — Development Roadmap

## Header

| Field | Value |
|-|-|
| Plan | idc-system |
| Spec | [PRD-V0.1.0.md](./PRD-V0.1.0.md) (active version: V0.1.1, 2026-05-10) |
| Start date | 2026-05-10 |
| Target | Bilingual (ar default + en) offline-first Tauri v2 desktop app + Fastify sync/backup server for an Iraqi medical imaging center. Single tenant. Three roles: superadmin, receptionist, accountant. |
| Surfaces | Frontend (React 19 + Vite + Tailwind v4 + shadcn) / Tauri/Rust (SQLite + sync engine + receipt printing) / Sync Server (Fastify + Prisma + Postgres) |
| Scope hard numbers | 15 PRD entities + 6 local-only tables (`outbox`, `sync_state`, `_migrations`, `visit_daily_rollup`, `sync_conflicts`, `backup_state`) + 3 FTS5 virtual tables (`doctors_fts`, `patients_fts`, `audit_log_fts`) = **24 local objects**. **18 Prisma models** (15 PRD + 3 server-only: `RefreshToken`, `Session`, `BackupArtifact`). ~38 routed pages (PRD §3.1 lists 29 primary; phases add ~9 auxiliary: `/lock`, `/sync/conflicts`, `/sync/conflicts/:id`, `/admin/backups`, `/admin/restore`, `/admin/inventory/items`, `/admin/inventory/items/:id`, `/admin/inventory/consumption-map`, plus `/admin/users/:id` etc. as detail-page splits). **~91 Tauri IPC commands** (auth/sync 10 + ref-data 46 + shifts 5 + reception 9 + inventory 10 + accounting 6 + void 1 + audit 6 + backup 4). **~75 sync-server HTTP routes** (auth 4 + sync 3 + healthz 1 + ref-data 37 + shifts 5 + visits 4 + inventory 11 + reports 4 + audit 1 + backup 4 + MFA stub 1). ~25 services across surfaces. Bilingual i18n across 9 namespaces. |
| Cadence | 10 phases, strict sequential. No parallel surface tracks in v1. |
| Definition of Done | All phases verified per `.claude/rules/planning.md` Section 6 + `PHASES-1-10-VERIFICATION.md` reports `status: complete` with `gaps: []`. |

## Phase Overview Table

| # | Phase Name | Surfaces | Scope | Size | Depends On | Status |
|-|-|-|-|-|-|-|
| 1 | Tauri Spine | Frontend, Tauri/Rust | Migration runner; `users`, `audit_log`, `outbox`, `sync_state`, `_migrations` tables; `with_audit` transactional helper; sync engine skeleton (cancellable Tokio task, push/pull/cursor; server endpoints stubbed locally — round-trip lands in P3); Tauri plugins (sql, store, dialog, stronghold, os, log) + capabilities allowlist; `AppState` with SqlitePool; app shell (sidebar, top bar, sync pill, language toggle); login + lock screens with offline-cached creds; role gate; shadcn baseline. | XL | — | Not Started |
| 2 | Sync Server Foundation | Sync Server | Fastify + Prisma + Postgres in Docker compose; plugins (sensible, cors, helmet, rate-limit, jwt RS256, env, compress, swagger + ui, multipart, prisma); env config; tenant plugin; error envelope; routes `/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password`, `/sync/push`, `/sync/pull`, `/sync/conflicts/:opId/resolve`, `/healthz`; Prisma `User`, `AuditLog`, server-only `RefreshToken`; TENANT_MODELS = `[User, AuditLog]`. | XL | 1 | Not Started |
| 3 | Reference Data & Admin CRUD | All | Entities `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `settings`, `patients` (LWW; `settings` = manual). Admin module shell with macOS-System-Settings sub-nav. IPC + sync routes per entity; bilingual `name_ar`/`name_en` form fields; FTS5 `doctors_fts` and `patients_fts`. First end-to-end sync round-trip exercised. | XL | 2 | Not Started |
| 4 | Operator Shifts | All | Entity `operator_shifts` (additive-only). Sub-page `/reception/shifts`; clock-in / clock-out commands; partial-unique-index on open shift; retroactive edit (superadmin only); shift-history view. | M | 3 | Not Started |
| 5 | Reception Per-Check & Lock Workflow | All | Entity `visits` with all inlined check/snapshot fields (manual). Routes `/reception` (Checks Grid), `/reception/checks/:slug` (Workspace), `/reception/checks/:slug/new` (New Visit), `/reception/visits/:id` (Detail). Money math (PRD §6.1.10). Lock workflow (§8.1) — single SQLite txn writes snapshots, sets `status='locked'`, writes audit, generates receipt PDF + thermal text. Operator picker filtered by clocked-in + specialty. Patient FTS autocomplete. Receipt templates per locale. | XL | 4 | Not Started |
| 6 | Inventory & Auto-Decrement | All | Entities `inventory_items` (LWW), `inventory_consumption_map` (LWW), `inventory_adjustments` (additive-only). Routes `/inventory`, `/inventory/items/:id`, `/inventory/adjust`. Lock workflow extension: matching consumption rows decrement stock in the same txn (`reason='consume_visit'`). Manual receive / writeoff / count_correction; low-stock badge in sidebar; admin Inventory section gains items + map editor. | L | 5 | Not Started |
| 7 | Accounting Reports & Daily Close | All | Read-only routes `/accounting`, `/accounting/visits`, `/accounting/visits/:id`, `/accounting/doctors`, `/accounting/doctors/:id`, `/accounting/operators`, `/accounting/operators/:id`, `/accounting/daily-close`. Local SQL aggregations + server-side fallback `/reports/visits`, `/reports/daily-close/:date`. CSV export (UTF-8 BOM). Trend cards. Drill-downs to visit detail. | L | 6 | Not Started |
| 8 | Void Workflow | All | Superadmin-only void on a locked visit (PRD §8.2). Reverses inventory consumption via offsetting `inventory_adjustments`. Confirm modal with `void_reason >= 5 chars`. Re-print of voided receipt watermark. Visible from Reception and Accounting visit detail. Audit `void` row with delta. | M | 6 | Not Started |
| 9 | Audit Page, FTS Polish & Vacuum | All | Global `/audit` page with deep filters (actor, action, entity, date range, free-text on `delta`). Server-backed query when out of local 90-day retention (`GET /audit/query`). Daily vacuum job soft-deleting audit rows older than 90d with `dirty=0`. Polished FTS5 indexes; conflict resolver UI; per-row pending-sync indicators. | M | 8 | Not Started |
| 10 | Backup, Ops & Final Verification | All | Sync-server nightly Postgres backup endpoint; restore wizard on Tauri side; pre-push validation script (`./tools/pre-push-check.sh`); CI workflow stub; tracing PII redaction layer; final verification pass per `planning.md`. | M | 9 | Not Started |

**Sizing tally:** 4 XL (1, 2, 3, 5), 2 L (6, 7), 4 M (4, 8, 9, 10).

## Dependency Graph

```
Frontend + Tauri/Rust track          Sync Server track            Cross-cutting
+--------------------------+         +-------------------+
| Phase 1: Tauri Spine     |         |                   |
| - users, audit_log       |         |                   |
| - outbox, sync_state     |         |                   |
| - with_audit txn helper  |         |                   |
| - sync engine skeleton   |         |                   |
| - app shell + i18n + RTL |         |                   |
| - login + lock screens   |         |                   |
+----------+---------------+         +-------------------+
           |                                                      Phase 1 lands client-only;
           v                                                      sync engine runs but server
+--------------------------+         +-------------------+        endpoints don't exist yet.
|                          |         | Phase 2: Sync Srv |
|                          |  <----  | Foundation        |
|                          |         | - JWT RS256       |
|                          |         | - tenant plugin   |
|                          |         | - sync push/pull  |
|                          |         | - User, AuditLog  |
|                          |         +---------+---------+
|                          |                   |
+--------------------------+                   |
                                               |
+----------------------------------------------+----------------+
|                                                               |
v                                                               v
+----------------------------------------------------------------------+
| Phase 3: Reference Data + Admin CRUD                                 |
| - check_types, check_subtypes, doctors, doctor_check_pricing,        |
|   operators, operator_specialties, settings, patients                |
| - Admin module shell (macOS-System-Settings sub-nav)                 |
| - First sync round-trip end-to-end                                   |
+----------------------------------+-----------------------------------+
                                   |
                                   v
                       +-----------------------+
                       | Phase 4: Op Shifts    |
                       | operator_shifts       |
                       +-----------+-----------+
                                   |
                                   v
                       +-----------------------+
                       | Phase 5: Reception    |
                       | visits + lock + recpt |
                       +-----------+-----------+
                                   |
                                   v
                       +-----------------------+
                       | Phase 6: Inventory    |
                       | items + map + adj     |
                       | auto-decrement on     |
                       | lock                  |
                       +-----+-----------+-----+
                             |           |
              +--------------+           +--------------+
              v                                         v
    +------------------+                     +-----------------+
    | Phase 7: Acct.   |                     | Phase 8: Void   |
    | reports + close  |                     | reverse cuts +  |
    | CSV export       |                     | inv adjustments |
    +--------+---------+                     +--------+--------+
             |                                        |
             +-------------------+--------------------+
                                 v
                       +---------------------+
                       | Phase 9: Audit page |
                       | FTS + vacuum        |
                       +----------+----------+
                                  |
                                  v
                       +---------------------+
                       | Phase 10: Backup    |
                       | Ops + Verify        |
                       +---------------------+
```

Although Phase 7 and Phase 8 both depend only on Phase 6, the user-locked plan keeps execution strictly sequential (P7 then P8). The graph above shows the data-dependency view; the schedule is linear.

## New Local Entities by Phase (SQLite)

| Phase | Tables | Notes |
|-|-|-|
| 1 | `users`, `audit_log`, `outbox`, `sync_state`, `_migrations` | `outbox`, `sync_state`, `_migrations` are local-only. `audit_log` uses `entity_id_tenant` (not `entity_id`) because `entity_id` already means audited row. |
| 2 | none | Server-side only this phase. |
| 3 | `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `settings`, `patients` + FTS5 virtual tables `doctors_fts`, `patients_fts` | XOR invariant on `check_types.has_subtypes`. `settings` is k/v singleton. |
| 4 | `operator_shifts` | Partial unique index `(operator_id) WHERE check_out_at IS NULL`. |
| 5 | `visits` | Partial indexes per PRD §6.1.10 on `(check_type_id, locked_at)`, `(doctor_id, locked_at)`, `(operator_id, locked_at)`. |
| 6 | `inventory_items`, `inventory_consumption_map`, `inventory_adjustments` | Materialized `quantity_on_hand` recomputed in the same txn as adjustments. |
| 7 | `visit_daily_rollup` (local-only denorm helper) | Materialized for snappy Dashboard KPI cards. |
| 8 | none | Void reuses `visits`, `inventory_adjustments`, `audit_log`. |
| 9 | `sync_conflicts`, `audit_log_fts` (FTS5) | Conflict resolver state + opt-in audit-log full-text. |
| 10 | `backup_state` (local-only k/v) | Tracks last restore for ops visibility. |

**Total objects created in SQLite: 24** = 15 PRD entities + 6 local-only (`outbox`, `sync_state`, `_migrations`, `visit_daily_rollup`, `sync_conflicts`, `backup_state`) + 3 FTS5 virtual tables (`doctors_fts`, `patients_fts`, `audit_log_fts`).

## New Server Entities by Phase (Prisma / Postgres)

| Phase | Models | Notes |
|-|-|-|
| 1 | none | Server scaffold doesn't exist until P2. |
| 2 | `User`, `AuditLog`, server-only `RefreshToken`, server-only `Session` | `User` and `AuditLog` are TENANT_MODELS. `RefreshToken` and `Session` are server-only and not synced. |
| 3 | `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `Setting`, `Patient` | All TENANT_MODELS additions. |
| 4 | `OperatorShift` | TENANT_MODELS addition. |
| 5 | `Visit` | TENANT_MODELS addition. Carries all snapshot columns. |
| 6 | `InventoryItem`, `InventoryConsumptionMap`, `InventoryAdjustment` | TENANT_MODELS additions. |
| 7 | none | Reports are aggregate queries against existing models. |
| 8 | none | Void writes to existing models. |
| 9 | none | Audit query route reads `AuditLog`. |
| 10 | none | Backup is a Postgres `pg_dump` orchestration. |

**Final TENANT_MODELS list at P10:** `User`, `AuditLog`, `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `Setting`, `Patient`, `OperatorShift`, `Visit`, `InventoryItem`, `InventoryConsumptionMap`, `InventoryAdjustment`. **15 models** matching the 15 PRD entities.

**Total Prisma models at P10: 18** = 15 TENANT_MODELS + 3 server-only (`RefreshToken`, `Session`, `BackupArtifact`).

## New Business Engines by Phase

| Phase | Frontend services | Tauri/Rust services | Server services |
|-|-|-|-|
| 1 | `AuthProvider`, `SyncStatusProvider`, `useOnlineStatus`, `RoleGate` | `AuthService` (offline cache), `SyncEngine` (Tokio, cancellable), `OutboxRepo`, `WithAuditTxn`, `MigrationRunner`, `AppState`, `DeviceIdProvider` | none |
| 2 | `apiClient` (axios + JWT interceptor + 401 refresh) | `SyncClient` (reqwest, retry, backoff) | `AuthService`, `JwtService`, `RefreshTokenService`, `SyncService` (push/pull/cursor), `ConflictResolverService`, `TenantPlugin`, `ErrorEnvelope` |
| 3 | reference-data hooks per entity, bilingual form components | `CheckTypeService`, `DoctorService` (with FTS), `OperatorService`, `SettingsService`, `PatientService` (with FTS), `RefDataRepos` (one per entity) | matching repos + presentation routes, all TypeBox-schemed |
| 4 | `useOpenShifts`, `useShiftHistory` | `ShiftService` (open-shift invariant, retroactive edits) | `ShiftService` |
| 5 | `useChecksGrid`, `useCheckWorkspace`, `useNewVisit`, `useLockVisit`, `ReceiptPrinter` | `VisitService::create`, `VisitService::lock`, `MoneyMath`, `OperatorEligibility`, `ReceiptRenderer` (PDF + thermal) | `VisitService` (read), conflict handler for `manual`-policy |
| 6 | inventory hooks | `InventoryService::consume_for_visit`, `InventoryService::adjust`, `StockMaterializer` | `InventoryService` |
| 7 | accounting report hooks, CSV exporter | `ReportingService` (local rollups) | `ReportsService` (server rollups, signed daily close stub) |
| 8 | `useVoidVisit` | `VisitService::void` | `VisitService::void` |
| 9 | audit search hooks | `AuditQueryService`, `VacuumJob` | `AuditQueryService` |
| 10 | restore wizard | `BackupService::restore` | `BackupService::snapshot`, `BackupService::list` |

## Sync Contracts by Phase

Per `.claude/rules/offline-first.md`, every push payload is MessagePack of the row plus its sync columns. Pull returns the same shape.

| Phase | Push entities (and policy) | Pull entities | Conflict dispatch |
|-|-|-|-|
| 1 | none in flight (engine runs against stub-only locally) | none | n/a |
| 2 | `users` (LWW), `audit_log` (additive-only) | same | LWW/additive-only |
| 3 | `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `patients` (LWW); `settings` (manual) | same | LWW for catalog; manual for settings (resolver UI from P9 polishes the screen, but the `409` path lands in P3) |
| 4 | `operator_shifts` (additive-only) | same | additive-only |
| 5 | `visits` (manual) | same | manual; resolver shows local vs server state for the contested fields |
| 6 | `inventory_items` (LWW), `inventory_consumption_map` (LWW), `inventory_adjustments` (additive-only) | same | LWW / LWW / additive-only |
| 7 | none new | none new | n/a |
| 8 | `visits` updates only (status/voided fields); writes also `inventory_adjustments` (additive) | same | manual on the void itself |
| 9 | none new | none new | n/a |
| 10 | none new | none new | n/a |

**Local-only (never synced):** `outbox`, `sync_state`, `_migrations`, `tauri-plugin-store` UI prefs, FTS5 virtual tables. **Server-only:** `RefreshToken`, `Session`.

## Cross-Cutting Concerns

- **Auth**: Phase 1 wires the client; Phase 2 wires the server. RS256 JWT (15m access, 30d refresh, sliding refresh). Stronghold caches refresh + Argon2id-hashed password for offline login.
- **i18n**: 9 namespaces (`common`, `auth`, `reception`, `accounting`, `inventory`, `admin`, `audit`, `errors`, `receipts`). Every UI string keyed; lint rule introduced in Phase 1.
- **RTL**: Tailwind logical properties (`ps-*`, `pe-*`, `ms-*`, `me-*`, `text-start`, `text-end`). Mirroring chevrons via `rtl:rotate-180`. Tested per phase.
- **Audit**: Every domain write goes through `with_audit`. Phase 9 adds the search UI + vacuum.
- **Currency**: IQD only. `arabic_numerals` setting toggles Eastern-Arabic digits in Arabic locale (default off).
- **Receipts**: Generated locally at lock time; PDF (A5) + thermal text alternative. Persisted to `$APPDATA/idc-system/receipts/<YYYY>/<MM>/<visit-id>.{pdf,txt}`.

## Definition of Done (per phase)

A phase file is implementation-ready when:

1. All 6 sections present (Local Schema, Server Schema, DDD, Business Logic, Infrastructure, Verification).
2. Every entity has copy-paste-ready SQLite + Prisma blocks.
3. Every IPC command listed as `| Command | Args | Returns | Description |`.
4. Every HTTP route listed as `| Method | Path | Description |`.
5. Every syncable entity declares its conflict-resolution policy.
6. Verification subsection lists concrete commands.
7. Phase explicitly states what it does NOT touch.

A phase is **complete in code** when:

1. `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo test` passes.
2. `pnpm lint && pnpm build` passes.
3. `pnpm tauri dev` boots; new screens load.
4. Sync round-trip works (where applicable).
5. Conflict scenario for any new `manual` entity surfaces correctly.
6. `frontend-summary.md` is updated for that phase (never batched).
7. `status.md` Phase Status row is flipped to `Completed` with counters bumped.

## Gap Analysis Additions

Pass log appended after each gap-analysis pass.

### Pass 0 (this pass — pre-write)
**Date:** 2026-05-10
**Status:** Pending. Phase files not yet drafted; gap analysis cannot run until phase files exist.
**Counts:** n/a.

### Pass 1 (initial)
**Date:** 2026-05-10
**Status:** Complete.
**Gap count:** 14.
**Severity distribution:** 0 CRITICAL · 0 HIGH · 4 MEDIUM · 10 LOW.
**Category distribution:** Missing Logic 6 · Missing Integration 4 · Missing Verification 2 · Missing Setup 2.
**Distribution by phase:**
- Phase 1 — 2 (Accessibility verification; MFA out-of-scope decision)
- Phase 2 — 3 (Rate-limit defaults; offline-cache refresh on password change; `/auth/mfa` 501 stub)
- Phase 3 — 3 (Doctor soft-delete cascade; `has_subtypes` toggle invariant; conflict-resolver intermediate behavior)
- Phase 4 — 1 (Operator soft-delete blocked while open shift)
- Phase 5 — 3 (Pricing-change banner; subtype soft-delete blocked when visits reference it; thermal Arabic word-wrap)
- Phase 6 — 2 (Status pill enumeration; concurrent consumption from two devices)
- Phase 7 — 3 (Daily-close RTL fidelity; PDF-only assertion; trend-card lookback bounds)
- Phase 10 — 3 (Success-metric instrumentation; receipts directory housekeeping; local SQLite backup story)
- Phases 8, 9 — 0

Each gap is filed as a Section 7.x entry in the originating phase file with severity, category, and remediation steps. The remediations are scoped to the phase that introduces the affected surface; cross-phase remediations (e.g. operator soft-delete in P3 extended in P4) are explicitly cross-referenced.

### Pass 2 (iterative)
**Date:** 2026-05-10
**Status:** Complete.
**Gap count:** 0 (no new gaps surfaced after Pass-1 remediations were folded into the phase files).

### Pass V (initial final verification)
**Date:** 2026-05-10
**Status:** Reported `gaps: []` but the audit was author-led; subsequent Pass V+ disproved the claim.

### Pass V+ (independent verification — author bias correction)
**Date:** 2026-05-10
**Method:** Two parallel external-style verification agents audited (a) schema parity field-by-field, (b) workflow / page / system-feature coverage and counter math.
**Status:** Complete.
**Gap count:** 16 (2 CRITICAL, 2 HIGH, 8 MEDIUM, 4 LOW).
**Severity distribution:**
- **CRITICAL:** P6 `inventory_items` schema dropped bilingual `name_ar`/`name_en` (PRD §6.1.12) — fixed in P6 §1, §2. P5 `Visit` Prisma was missing the `inventoryAdjustments` back-relation while P6 referenced it — fixed by deferring the relation to P6 §2's new "Existing models updated" subsection.
- **HIGH:** P6 was missing explicit copy-paste deltas for back-relations on CheckType, CheckSubtype, Visit, User — fixed (P6 §2). P3 forward-reference notes — covered by the same subsection in P6.
- **MEDIUM:** `note` vs `notes` mismatch on `operator_shifts` (PRD §6.1.8 = `note` singular) — fixed in P4. Settings seed missing `clinic_display_name_en` — fixed in P3. Counter math in roadmap header / status §2 / P10 summary — fixed (24 local objects, 18 server models, ~91 IPC, ~75 routes, ~38 pages). `with_audit` lint-rule unowned — assigned to P9 §7.1 (Pass V+ entry). Operator Shifts page missing Columns + States subsection — added P4 §7.0.
- **LOW:** P1 in-memory conflict queue undefined — added P1 §7.3. `sync_conflicts.policy` CHECK widening — P9 §7.2. First-boot migration of in-memory conflicts — P9 §7.3. P3 `bool` parser undocumented — added inline to P3 settings note.
**Outcome:** all 16 gaps remediated in-place (phase files + master files updated). `PHASES-1-10-VERIFICATION.md` rewritten as Pass-V+ to reflect this audit.

### Pass V++ (post-remediation re-audit)
*Pending. Should re-run the same two verification agents to confirm zero remaining gaps before declaring the plan implementation-ready.*
