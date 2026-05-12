# IDC System v0.1.x Status

_Last updated: 2026-05-12. Source: [roadmap.md](./roadmap.md)._

## Phase Status Table

| # | Phase | Surfaces | Status | Started | Completed | Local Tables Added | Server Models Added | IPC Commands Added | Routes Added | Services Added |
|-|-|-|-|-|-|-|-|-|-|-|
| 01 | Foundation & Sync Plumbing | All | complete | 2026-05-11 | 2026-05-11 | 4 (`outbox`, `sync_state`, `audit_log`, `metrics_events`) | 4 (`AuditLog`, `ProcessedOp`, `SyncCursor`, `ConflictParked`) | 9 | 5 | 6 |
| 02 | Authentication & Users | All | complete | 2026-05-11 | 2026-05-11 | 2 (`users`, `settings`) | 3 (`User`, `Setting`, `RefreshToken`) + enums (`UserRole`, `SettingType`) | 15 (auth: 6, users: 7, settings: 3 minus overlap) | 5 (`/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password`, `/auth/bootstrap-superadmin`) | 4 (AuthService [Rust], UserService, SettingsService, AuthService [TS]) |
| 03 | Catalog & Reference Data | All | complete | 2026-05-12 | 2026-05-12 | 9 (8 tables + 1 FTS5 `doctors_fts`) | 8 | 38 | 0 | 8 |
| 04 | Operator Shifts | Frontend, Tauri, Server | complete | 2026-05-12 | 2026-05-12 | 1 (`operator_shifts`) | 1 (`OperatorShift`) + back-relations on `User` (`ShiftCheckIn` / `ShiftCheckOut`) and `Operator` (`shifts`) | 7 | 0 | 1 |
| 05 | Reception & Visit Lock | All | not_started | - | - | 4 (`patients`, `patients_fts`, `visits`, `inventory_adjustments`) | 3 (`Patient`, `Visit`, `InventoryAdjustment`) | ~12 | 0 | 5 |
| 06 | Inventory Operations | All | not_started | - | - | 0 | 0 | 5 | 0 | 1 |
| 07 | Accounting & Reports | All | not_started | - | - | 0 | 0 | ~9 | 2 | 3 |
| 08 | Audit, Conflict Resolver & Polish | All | not_started | - | - | 0 | 0 | 2 | 1 | 2 |

Aggregate ship target after Phase 08: 17 base SQLite tables + 2 FTS5 virtual tables + 2 engine tables = 21 SQLite objects; 15 syncable Prisma models + 4 server-only models = 19 server models; ~67 IPC commands (PRD §5.1 estimate "~55" was a lower bound; actual exceeds it by virtue of fine-grained admin CRUD); 11 sync-server routes (PRD §5.2 lists 10 plus `/healthz`); 31 services across surfaces.

## Cumulative Totals

| Metric | Before | Current | Target |
|-|-|-|-|
| SQLite syncable tables | 0 | 12 (`audit_log`, `users`, `settings`, `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`, `operator_shifts`) | 15 (PRD §6.1.1-§6.1.15) |
| SQLite engine tables | 0 | 3 (`outbox`, `sync_state`, `metrics_events`) | 3 |
| SQLite FTS5 virtual tables | 0 | 1 (`doctors_fts`) | 2 (`patients_fts`, `doctors_fts`) |
| Prisma syncable models | 0 | 12 (`AuditLog`, `User`, `Setting`, `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `InventoryItem`, `InventoryConsumptionMap`, `OperatorShift`) | 15 |
| Prisma server-only models | 0 | 4 (`ProcessedOp`, `SyncCursor`, `ConflictParked`, `RefreshToken`) | 4 |
| Tauri IPC commands | 0 | 69 (sync: 9, auth: 6, users: 7, settings: 3, catalog: 37, shifts: 7) | ~55-67 |
| Sync-server routes | 2 (template only) | 11 (`/`, `/healthz`, `/sync/push`, `/sync/pull`, `/sync/lookup-op`, `/sync/conflicts/:opId/resolve`, `/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password`, `/auth/bootstrap-superadmin`) | 11 (incl. `/healthz` and `/documentation`) |
| Frontend pages | 2 (`/`, `*`) | 18 (Phase 1+2: 9 + Phase 3: 8 admin pages + Phase 4: 1 reception page (`/reception/shifts`)) | 31 |
| Conflict policies in use | 0 | 4 (`additive-only` for `audit_log` + `operator_shifts`, `last-write-wins` for `users` + all 8 catalog entities, `manual` for `settings`) | 3 |
| Locales | 2 (ar+en) | 2 (6 namespaces: `common`, `errors`, `receipts`, `auth`, `admin`, `reception`) | 2 |
| Audit retention (local) | n/a | n/a | 90 days |
| Audit retention (server) | n/a | n/a | indefinite |

Targets reconcile with PRD §3.2 (29 pages) plus the auth and sync utility pages introduced across phases.

## Gap Analysis Summary

Pass 1 completed 2026-05-11 via six parallel sub-agents per the methodology in [.claude/rules/planning.md §Gap Analysis Methodology](../../.claude/rules/planning.md#gap-analysis-methodology-mandatory). 119 gaps logged as Section 7.x subsections across phase-01 through phase-08. Zero CRITICAL; the load was concentrated in service-method completeness (phase-03), workflow / UI element enumeration (phase-05, phase-07), and cross-cutting infrastructure (phase-01, phase-02, phase-08).

Pass 2 completed 2026-05-11 via six parallel sub-agents on non-overlapping slices (§6.1.1-7, §6.1.8-15, sync contracts, §7.x consistency, accounting, cross-cutting). 88 new gaps logged plus 2 amendments to existing Pass-1 §7.x content (phase-02 §7.1 settings-seed `value_type` literal corrected; phase-08 §1 audit action union expanded from 10 to 12 values). 5 CRITICAL gaps surfaced (all schema/state-machine integrity): outbox `op` enum dead code, `DoctorCheckPricing` / `InventoryConsumptionMap` Postgres NULL-distinct uniqueness flaws, `inventory_adjustments` mutability trigger, visit illegal-transition exhaustiveness, first-launch superadmin bootstrap UX. All 88 new gaps applied as new §7.x subsections in the corresponding phase files.

Pass 3 (final) completed 2026-05-11 via six parallel sub-agents on non-overlapping slices (sync engine + Pass-2 deferred items, §6.1.1-8 entities, §6.1.9-15 entities, workflows §4/§8/§10, module specs §7 UI + role gates, cross-cutting + verification). 47 new gaps logged across all eight phase files. 2 CRITICAL gaps (Prisma `User` and `Operator` missing `OperatorShift` back-relations -- `prisma generate` would fail; combined fix in phase-02 §7.29 + phase-03 §7.30). 11 HIGH gaps cluster in three families: (a) server `pulledAt` symmetry (phase-01 §7.32, phase-04 §7.13, phase-05 §7.52), (b) raw-SQL migration ordering (phase-03 §7.31, phase-05 §7.51), (c) frontend route-level role gates (phase-03 §7.36, phase-04 §7.16, phase-05 §7.58, phase-06 §7.13, phase-07 §7.28, phase-08 §7.23). Plus visit name-snapshot Prisma symmetry + CHECK extension (phase-05 §7.52, §7.53), `inventory_adjustments` per-reason CHECK as concrete server raw migration (phase-06 §7.14), and lock + receipt-print telemetry emission (phase-05 §7.54). Verification spot-check: 15/15 representative items from Pass-1+Pass-2 §7.x verified clean. No Pass 4 needed.

| Pass | Date | Gaps Found | Critical | High | Medium | Low | Status |
|-|-|-|-|-|-|-|-|
| 1 | 2026-05-11 | 119 | 0 | 38 | 61 | 20 | complete |
| 2 | 2026-05-11 | 88  | 5 | 33 | 36 | 14 | complete |
| 3 | 2026-05-11 | 47  | 2 | 11 | 19 | 15 | complete |

### Per-Phase Distribution (Pass 1)

| Phase | Gaps | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 | 14 | 0 | 6 | 5  | 3 |
| 02 | 16 | 0 | 7 | 8  | 1 |
| 03 | 18 | 0 | 4 | 9  | 5 |
| 04 |  7 | 0 | 2 | 2  | 3 |
| 05 | 30 | 0 | 9 | 18 | 3 |
| 06 |  8 | 0 | 2 | 5  | 1 |
| 07 | 13 | 0 | 5 | 6  | 2 |
| 08 | 13 | 0 | 3 | 8  | 2 |

### Per-Phase Distribution (Pass 2)

| Phase | Gaps | Critical | High | Medium | Low | New §7.x range |
|-|-|-|-|-|-|-|
| 01 | 15 | 1 | 3 | 7 | 4 | §7.15 - §7.29 |
| 02 | 12 | 1 | 8 | 3 | 0 | §7.17 - §7.28 |
| 03 | 11 | 1 | 3 | 6 | 1 | §7.19 - §7.29 |
| 04 |  5 | 0 | 2 | 3 | 0 | §7.8  - §7.12 |
| 05 | 20 | 2 |10 | 6 | 2 | §7.31 - §7.50 |
| 06 |  4 | 0 | 2 | 2 | 0 | §7.9  - §7.12 |
| 07 | 14 | 0 | 3 | 6 | 5 | §7.14 - §7.27 |
| 08 |  7 | 0 | 2 | 3 | 2 | §7.14 - §7.20 |

### Per-Phase Distribution (Pass 3 - Final)

| Phase | Gaps | Critical | High | Medium | Low | New §7.x range |
|-|-|-|-|-|-|-|
| 01 |  7 | 0 | 1 | 5 | 1 | §7.30 - §7.36 |
| 02 |  5 | 1 | 0 | 1 | 3 | §7.29 - §7.33 |
| 03 |  8 | 1 | 2 | 3 | 2 | §7.30 - §7.36 (B-2+B-3 merged in §7.30) |
| 04 |  4 | 0 | 1 | 2 | 1 | §7.13 - §7.16 |
| 05 |  9 | 0 | 4 | 3 | 2 | §7.51 - §7.58 (E-1..E-4 merged in §7.57; A-6+C-3 merged in §7.52) |
| 06 |  3 | 0 | 2 | 0 | 1 | §7.13 - §7.15 |
| 07 |  3 | 0 | 1 | 1 | 1 | §7.28 - §7.30 |
| 08 |  6 | 0 | 1 | 2 | 3 | §7.21 - §7.26 |
| **Total** | 47 | 2 |11 |19 |15 |               |

### Cumulative Gap Counts (Pass 1 + Pass 2 + Pass 3)

| Phase | Total | Critical | High | Medium | Low |
|-|-|-|-|-|-|
| 01 |  36 | 1 | 10 | 17 |  8 |
| 02 |  33 | 2 | 15 | 12 |  4 |
| 03 |  37 | 2 |  9 | 18 |  8 |
| 04 |  16 | 0 |  5 |  7 |  4 |
| 05 |  59 | 2 | 23 | 27 |  7 |
| 06 |  15 | 0 |  6 |  7 |  2 |
| 07 |  30 | 0 |  9 | 13 |  8 |
| 08 |  26 | 0 |  6 | 13 |  7 |
| **Total** | 254 | 7 | 82 | 116 | 49 |

### Top Pass-1 Themes

- **Audit-first write ordering** (PRD §4.3) was not enforced in `with_audit` and the lock/void workflows. Fixed by restructuring to a two-pass closure pattern in phase-01 §7.7, phase-05 §7.10 and §7.11.
- **Snapshot completeness** on `visits`: 7 money snapshots covered but human-readable names (`patient_name`, `doctor_name`, `operator_name`, bilingual `check_*_name_*`) were not. Receipts could drift after rename. Fixed by phase-05 §7.17.
- **Cross-surface validation symmetry**: many entity invariants enforced in Rust but not on the server sync-push path (and vice versa). Tabled per entity (phase-03 §7.6, §7.9; phase-05 §7.6; phase-06 §7.1).
- **Receipt printing path** was incoherent: PRD §8.1 step 12 calls for OS print dialog + thermal printer routing; phase-05 routed thermal through a save-as dialog. Fixed by phase-05 §7.23 with new IPC commands and a `thermal_printer_name` setting.
- **PRD §8.5/§8.6 banners** (pricing-changed, settings-changed) were not in any phase. Fixed by phase-05 §7.27, phase-05 §7.28, phase-02 §7.4.
- **i18n + a11y scaffolding** was implicit; PRD §10.6/§10.7 explicit. Fixed by phase-01 §7.10, §7.11, phase-08 §7.9, §7.13.
- **Sync-pill UX** was missing the pending-count badge and click-to-resolver wiring (PRD §10.8). Fixed by phase-01 §7.4, §7.5.
- **Conflict resolver state recovery** had no `/sync/conflicts` listing endpoint; cold restarts lost the parked queue. Fixed by phase-08 §7.11.
- **Daily-close** lacked the tz boundary definition and per-doctor / per-operator breakdowns mandated by PRD §8.4. Fixed by phase-07 §7.8, §7.9.

### Top Pass-2 Themes

- **State-machine exhaustiveness** for `visits`: Pass 1 wired the legal transitions but left the illegal-transition matrix unblocked; an invalid `(voided -> draft)` would slip through. Fixed by phase-05 §7.32 with a single `assert_transition` helper that every mutator invokes.
- **Schema-level append-only enforcement**: `inventory_adjustments` was advertised as immutable but the local table allowed UPDATE. Fixed by phase-05 §7.33 with a SQLite trigger and a mirrored Postgres trigger.
- **Postgres NULL-distinctness** silently broke uniqueness on `DoctorCheckPricing` and `InventoryConsumptionMap` (NULL `check_subtype_id` rows would all pass `@@unique`). Fixed by phase-03 §7.20 and §7.21 with paired partial unique indexes.
- **Sync engine corners**: delete-vs-edit policy was undefined (would resurrect deleted rows); replay of parked conflicts would loop indefinitely; `SyncCursor` PK couldn't scope per tenant. Fixed by phase-01 §7.16, §7.17, §7.19, §7.20.
- **Orphaned forward references**: Pass-1 §7.x sections in phase-02/05 referenced IPC (`pricing::resolve`, `visits::list_workspace`), routes (`/auth/jwks`), and components (`<SettingsChangedBanner>`, `<UserDeleteConfirm>` row, `<InventoryAdminTable>` row) that the destination phases never declared. Fixed in this pass with explicit receipts and component-table rows.
- **First-launch bootstrap**: a fresh deployment had no path to create the initial superadmin or configure the sync server URL. Fixed by phase-02 §7.21 (`users::create_first_admin` + `/setup/first-run`) and phase-01 §7.22 (`<FirstLaunchSetupModal>` + `config::set_sync_server_url`).
- **LWW tiebreak symmetry**: Pass-1 declared the `origin_device_id` tiebreak only on the catalog (phase-03 §7.17). Phase-02 (`users`) and phase-05 (`patients`) had empty tiebreak notes. Fixed by phase-02 §7.23 and phase-05 §7.40.
- **Telemetry plumbing**: PRD §1.3 names success metrics but no phase had a place to store them. Fixed by phase-01 §7.28 (`metrics_events` local table) and phase-08 §7.17 (`diagnostics::summary` IPC, `/metrics` server endpoint).
- **Audit closed enum across phases**: phase-08 §1 lagged behind phase-01 §7.8 (10 vs 12 values). Corrected in place to keep all writers using the same union.

### Top Pass-3 Themes

- **Prisma relation graph integrity**: phase-04 declared named back-relations on `User` (`ShiftCheckIn`, `ShiftCheckOut`) and `Operator` (`shifts`) but the inverse fields were missing in phase-02 and phase-03; `prisma generate` would have failed at first server build. Both CRITICAL gaps; combined fix in phase-02 §7.29 + phase-03 §7.30 (also added the PRD-mandated `Visit[]` back-relations on `CheckType`, `CheckSubtype`, `Doctor`, `Operator`).
- **Server `pulledAt` symmetry**: phase-02 §7.17 and phase-03 §7.19 added `pulledAt` to user/setting/catalog models; the `AuditLog`, `OperatorShift`, `Patient`, `Visit`, `InventoryAdjustment` server models were silently missed. Fixed by phase-01 §7.32, phase-04 §7.13, phase-05 §7.52.
- **Frontend route-level role gates**: backend role enforcement (Pass-1+Pass-2) was complete but the `<RequireRole>` wrapper from phase-02 §7.8 was never wired around the actual route outlets. Receptionist navigating to `/admin` would hit an error toast, not redirect to `/no-access`. Fixed by phase-03 §7.36 (admin), phase-05 §7.58 (reception), phase-06 §7.13 (inventory), phase-07 §7.28 (accounting), phase-08 §7.23 (audit + sync resolver).
- **Telemetry emission points missing**: phase-01 §7.28 declared the `metrics_events` table and named which kinds each surface emits, but no phase wrote the actual emission code. PRD §1.3 success metrics (lock p95, receipt-print success rate) were unmeasurable. Fixed by phase-01 §7.34 (sync engine push/pull/conflict events) + phase-05 §7.54 (lock_start/lock_end + receipt_print_ok/fail).
- **Raw-SQL migration ordering**: Pass-2 §7.20/§7.21/§7.33 added raw-SQL partial indexes and triggers but never declared how those files coexist with `prisma migrate deploy`. Fixed by phase-03 §7.31 + phase-05 §7.51 (canonical lex-ordered file naming convention).
- **Visit name-snapshot Prisma symmetry**: phase-05 §7.17 added 7 name-snapshot columns to local SQLite but the server `Visit` Prisma model and the §1 CHECK constraint were never extended. Pulled rows would round-trip without the receipt-critical name snapshots. Fixed by phase-05 §7.52 + §7.53.
- **Server `inventory_adjustments` per-reason CHECK as concrete migration**: phase-06 §7.1 cited `@@check` (which doesn't exist in Prisma 5/6) and never wrote the raw-SQL migration. Server would accept malicious `{reason:'receive', delta:-5}` pushes. Fixed by phase-06 §7.14 (concrete `prisma migrate dev --create-only` SQL).
- **Audit-log delete-vs-edit carve-out**: phase-01 §7.16 universal delete-vs-edit rule contradicted §7.21 audit-log immutability. Carve-out written explicitly into phase-01 §7.31 with verification.
- **`metrics_events` vacuum executor**: phase-01 §7.28 promised 30-day retention "via the same vacuum that prunes audit_log" but phase-08 `AuditVacuumJob` only touched `audit_log`. Fixed by phase-08 §7.21 with concrete `MetricsRepo::vacuum_older_than` extension.
- **Resolver mid-flight idempotency**: a network drop between server-commit and client-ack would double-apply the resolution. Fixed by phase-08 §7.22 (stable resolve-op-id, ProcessedOp short-circuit, `409 ALREADY_RESOLVED` on conflicting retry).
- **Audit-action enum daily_close_run**: phase-07 §7.18 wrote the audit row but the enum union (phase-01 §7.8) was never extended; Rust would not compile. Fixed by phase-01 §7.36 (final 14-value enum).
- **i18n key inventory completeness**: phase-03 §7.29 catalogue missed sync-engine error keys, shift errors, RTL lint diagnostic, dye-supported violation. Fixed by phase-01 §7.30 + phase-03 §7.32.

## Blockers & Notes

_No blockers at plan-authoring time._

### Phase 01 completion notes (2026-05-11)

- All §1-§5 plumbing landed; §7.1-§7.36 gap additions applied except where explicitly deferred to later phases (resolver UI to phase-08 §7.5; conflict storage on server is server-canonical, IPC `sync_list_conflicts` is a stub list per §7.5).
- Sync-server Phase-1 ships an in-memory store for `audit_log`, `ProcessedOp`, `SyncCursor`, `ConflictParked` so the suite is hermetic. The Prisma-backed swap will be wired in Phase-2 alongside `User` / `Setting`.
- Verification: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (4 unit + 6 integration), `pnpm lint`, `pnpm build`, sync-server `pnpm test` (11/11) all green.
- Tauri bundle icons regenerated from `public/logo.png` (Windows ICO, macOS ICNS, Linux PNG, iOS AppIcon set, Android mipmaps).

### Design system retrofit (2026-05-12)

After Phases 1 and 2 shipped, the editorial design system in `.claude/rules/design-system.md` was authored. All Phase 1 and Phase 2 frontend was retrofitted to match before Phase 3 begins:

- `src/index.css` rewritten with editorial tokens (`--paper`, `--paper-2`, `--surface`, `--ink/2/3/4`, `--crimson`, `--gold`, `--success`, `--info`, `--line/-2`), shadcn-alias layer mapping these onto the existing `--background` / `--foreground` / etc. names, and Tailwind v4 `@theme` exposing them as `bg-paper`, `text-ink-3`, `border-crimson`, etc.
- Inter Variable + Geist Mono Variable bundled offline via `@fontsource-variable/*`. Body sets Inter; `.font-mono` + tabular-nums for IDs / times / money.
- Component utility classes added: `.eyebrow`, `.status-pill` (+ `is-success/warn/info/danger/live`), `.role-pill` (+ `is-receptionist/accountant/superadmin`), `.count-badge`, `.btn` (+ `btn-primary/ink/ghost/danger`), `.input`, `.field-label`, `.panel` / `.panel-head` / `.panel-body` / `.panel-title`, `.data-table`, `.nav-item`.
- Shell rebuilt: 64px header (just breadcrumbs + language pill + avatar), 32px status bar (sync pill + mono build/device), 256px sidebar (brand + role pill + grouped nav + user card with lock icon at bottom). All chrome sits on `--paper`; only cards rise to `--surface`.
- Auth pages (login, lock, no-access, first-run) + first-launch modal use the eyebrow voice, panel cards, and editorial buttons.
- Admin pages: users list now uses `.data-table` with role-coded pills and status pills; user detail uses role-tinted avatar tile and a `panel`-based danger zone; settings groups keys into editorial panels with a swipe-style toggle for booleans.
- Home + 404 reworked with the eyebrow date stamp and phase-state cards.
- Root redirect simplified -- all roles land on `/home` until per-role landing screens ship in later phases.
- i18n: added `nav.group.{operations,records,admin}`, `auth.role_{role}`, `auth.*_eyebrow`, `admin.eyebrow`, `admin.settings.group.*` + `admin.settings.key.*`, `home.*`, `not_found.*`, `setup.eyebrow` keys to both `en` and `ar` locales.
- Verification: `pnpm lint` clean (only pre-existing unused-disable warnings in unrelated files), `pnpm build` succeeds, fonts bundled into `dist/assets/`.

### Phase 03 completion notes (2026-05-12)

- **Local schema**: migration `003_catalog.sql` adds 8 syncable tables (`check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`) plus FTS5 virtual sibling `doctors_fts` with triggers that filter soft-deleted rows (§7.33). Migration runner extended to parse `CREATE TRIGGER ... BEGIN ... END;` blocks (was previously single-statement). Partial unique indexes on `doctor_check_pricing` and `inventory_consumption_map` handle the SQLite-`IFNULL(...)` and Postgres-NULL-distinct uniqueness flaws from §7.20 / §7.21.
- **Server schema**: 8 Prisma models added with `pulledAt` columns (§7.19), `CutKind` enum (`pct` / `fixed`), foreign-key relations between catalog parents (`CheckType`, `Doctor`, `Operator`) and their children. Schema validates via `tsc`; the in-memory store is the runtime backing for Phase-3 same as Phase-1/2.
- **Tauri / Rust**: New `catalog` bounded context (`src-tauri/src/domains/catalog/`) with 8 entities, 8 repository traits, 8 sqlx implementations, 8 application services, `PricingResolver` domain service (§7.26), and a `catalog:pricing_changed` Tauri event with `PricingChangedPayload` (§7.27 / §7.35). 37 new IPC commands wired into `lib.rs::generate_handler!` and `AppState`. Audit-first ordering preserved via the Phase-1 `AuditWriter::with_audit` closure (every catalog mutation writes an audit row before its business row, then enqueues an outbox op per affected entity -- doctor / operator soft-delete cascades through pricings / specialties).
- **Sync server**: `MemorySyncStore` extended with 8 catalog maps and a `upsertLWW` helper that implements the `(version, updated_at, origin_device_id)` tiebreak from §7.17. `SyncPushService` dispatches each catalog entity through superadmin-gated branches with cross-row invariant guards (check-type XOR, subtype-required, dye-supported, cut-kind bounds). Pull `changesSince` returns catalog rows. 5 new tests in `test/routes/catalog-sync.test.ts` cover push success, XOR rejection, role gate, pull round-trip, and parent-mismatch rejection.
- **Frontend**: New `features/catalog/queries.ts` with full TanStack React Query hooks (list / detail / mutations) per entity. Eight new admin pages under `/admin/check-types/{,:id}`, `/admin/doctors/{,:id}`, `/admin/operators/{,:id}`, `/admin/inventory/{,:id}` -- all guarded by the existing `<AdminGate>` (superadmin only, §7.36). Sidebar nav extended with check-types, doctors, operators, inventory links (visible only to superadmins). Reusable `<AdminHeader>` + `<FieldLabel>` + `<EmptyRow>` + `<ErrorBanner>` primitives factored out into `components/admin/admin-panel.tsx`. `resolveLocaleName` helper at `src/lib/format/locale-name.ts` (§7.16).
- **Deferred**: Subtype picker in `<DoctorPricingEditor>` (MVP form supports flat-only; subtype-aware row will land alongside the visit form in phase-05). `<InventoryAdminTable>` audit-log join column (§7.13). `set_active` IPC for operators is wired in the Rust service but the operator detail page uses an inline button rather than a dedicated `operators::set_active` IPC entry (the existing `operators_update` covers it for now -- a follow-up will register `operators::set_active` directly when phase-04 needs it).
- **Verification**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (15 unit + 7 catalog integration + 6 sync integration = 28 Rust tests), sync-server `pnpm build:ts` and `pnpm test` (22/22, including 5 new catalog tests), `pnpm lint` (0 errors), `pnpm build` (frontend) all green. Sync round-trip verified end-to-end via Fastify `app.inject()` in the sync-server tests (functionally equivalent to curl through the HTTP layer; same auth, validation, store).

### Phase 04 completion notes (2026-05-12)

- **Local schema**: migration `004_operator_shifts.sql` adds `operator_shifts` with the partial unique index `operator_shifts_open` (single open shift per operator), the `(entity_id, check_in_at)` history index (§7.2), and an `operator_id` index. FK declarations on `check_in_by_user_id` / `check_out_by_user_id` are `ON DELETE RESTRICT` (§7.14) to document the no-hard-delete-user contract.
- **Server schema**: `OperatorShift` Prisma model added with the `pulledAt` column (§7.13); back-relations `ShiftCheckIn` / `ShiftCheckOut` wired on `User` and `shifts` on `Operator` (closes the Pass-3 CRITICAL §7.29 / §7.30 gaps for shifts).
- **Tauri / Rust**: new `shifts` bounded context (`src-tauri/src/domains/shifts/`) with the `OperatorShift` entity (open/close/edit_times/soft_deleted state transitions with future-time, ordering and double-close guards), `OperatorShiftRepo` trait, sqlx `SqliteOperatorShiftRepo`, and `ShiftService` exposing `clock_in` / `clock_out` / `edit` / `soft_delete` / `list_open` / `history_today` / `list_overlaps`. All mutators go through the Phase-1 `AuditWriter::with_audit` so the audit row precedes the business write (PRD §4.3). Audit-action enum reused the existing `ClockIn` / `ClockOut` variants from phase-01 §7.8 (already in `AuditAction`). 7 new IPC commands registered in `lib.rs::generate_handler!`; `AppState` gained a `shift_service` slot wired in `bootstrap()`.
- **Sync server**: `MemorySyncStore` gained an `operatorShifts` map and a `upsertOperatorShift` LWW helper. `SyncPushService` dispatches `operator_shifts` with a `validateOperatorShift` guard (required fields, `check_out_at >= check_in_at`, `check_out_by_user_id` paired with `check_out_at`, note length). Role gate accepts `receptionist` OR `superadmin` (clock-in path requires only receptionist). `changesSince` includes shifts including soft-deleted rows so the tombstone propagates under additive policy (§7.9).
- **Frontend**: new `features/shifts/queries.ts` with TanStack Query hooks (`useOpenShifts`, `useShiftHistoryToday`, `useShiftOverlaps`, `useShiftClockIn`, `useShiftClockOut`, `useShiftEdit`, `useShiftSoftDelete`); Zod schemas in `lib/schemas/shift.ts`; reception components `<ClockInDialog>`, `<OnShiftTable>`, `<ShiftHistoryToday>`, `<RetroactiveShiftEditor>`, `<OpenShiftConflictBanner>`, `<ResolveOverlappingShifts>`; the `/reception/shifts` page wired in `src/routes/index.tsx` under a `<RequireRole roles={['receptionist','superadmin']}>` wrapper (§7.16). Sidebar nav got a Shifts link visible to receptionist + superadmin. New `reception` i18n namespace shipped in EN + AR.
- **Deferred**: lines-run column is rendered as `0` placeholder pending phase-05's `shifts::lines_run_today` IPC (cross-referenced in §7.7). Server `pulledAt` is declared on the model but not yet stamped by the pull service (will be wired alongside Prisma migration in a later sync-engine slice -- mirrors the catalog phase-03 status).
- **Verification**:
  - `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` clean.
  - `cargo test`: 20 + 7 + 13 + 6 = **46 Rust tests pass** (13 new in `tests/shifts_phase04.rs` cover clock_in success, double-clock-in rejection, inactive-operator rejection, clock-out, role-gated edit (reject + accept), edit overlap rejection, soft-delete idempotency, audit-row counts for each action, outbox enqueue, overlap detection across simulated concurrent rows, history-today window, and migration-applied assertion).
  - `pnpm lint` clean (0 errors, 6 pre-existing warnings in unrelated files); `pnpm build` succeeds.
  - Sync-server `pnpm build:ts` clean; `pnpm test`: **28 tests pass** (6 new in `test/routes/shifts-sync.test.ts` cover push acceptance for receptionist, validation rejection on `check_out_at < check_in_at`, role rejection for accountant, idempotent op_id replay, tombstone pull including `deleted_at`, and LWW resolution to higher version).
  - **Live curl round-trip** (server booted on `:3161`, JWT minted via `/auth/login`): clock_in push -> `applied`; replay -> `duplicate`; bad ordering payload -> `422 VALIDATION_ERROR`; accountant role -> `403`; pull from new device returns the row; clock_out push at v2 -> `applied`; second pull returns v2 with `check_out_at`; soft-delete at v3 -> `applied`; third pull returns the tombstone (`deleted_at` set); missing `operator_id` -> `422`; cross-tenant `entity_id` -> `403`.

### Phase 02 completion notes (2026-05-11)

- **Local schema**: migration `002_users_settings.sql` adds `users` and `settings` with v1 seed (10 required keys). The `audit_log` FK to `users(id)` is deferred (documented in the migration header); application-enforced through `with_audit`.
- **Server schema**: `User`, `Setting`, `RefreshToken` models with enums `UserRole` / `SettingType`. `pulledAt` added to `AuditLog`, `User`, `Setting` (§7.17, §7.32). All existing models now include the FK back-reference `AuditActor`.
- **Tauri / Rust**: auth domain (Argon2id hashing, online + offline login, lock/unlock), users service (CRUD + reset-password with audit-first ordering), settings service (per-key validation, superadmin gate, manual conflict policy). 15 new IPC commands; AppState gained `auth_service`, `user_service`, `settings_service`, `user_repo`, `locked` fields.
- **Sync server**: in-memory `MemoryUserStore` (user + refresh token repository), `AuthService` (Argon2id via `@node-rs/argon2`, sliding 15m/30d JWT pair, sha256-hashed refresh tokens, rotate-on-refresh), routes `/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password`, `/auth/bootstrap-superadmin`. Push service extended to dispatch `users` (LWW) and `settings` (manual conflict detection with `ConflictParked` envelope).
- **Frontend**: `AuthBootstrap` mounts at root and syncs Tauri IPC -> `useAuthStore`. New stores: `useAuthStore`, `useIdleStore`. New components: `RequireAuth` / `RequireRole`, `IdleWatcher`, `UserMenu`. New pages: `/login`, `/no-access`, `/lock`, `/setup/first-run`, `/admin/users`, `/admin/users/:id`, `/admin/settings`. Root `/` performs role-based redirect.
- **Deferred for Phase 8**: per-PRD-§5.5 stronghold-backed offline-creds cache (currently offline login verifies against locally-synced `users.password_hash` -- sufficient since the row is locally present after first online sync). JWT public-key pinning + JWKS endpoint (§7.10). OS suspend/blur lock-on-suspend hook (§7.26). The `tracing` PII-redaction layer remains a Phase 1 §7.14 follow-up.
- **Verification**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (7 unit + 6 integration = 13 Rust), `pnpm lint` (0 errors), `pnpm build`, sync-server `pnpm test` (17/17, including 6 new auth route tests covering login, refresh rotation/reuse rejection, change-password + token revocation, logout, bootstrap idempotency) all green.

Parallel-track notes:
- Within each phase, the three surfaces (Frontend / Tauri-Rust / Sync Server) work as parallel tracks once the migration files land.
- Phase 03 is the heaviest (XL) due to eight reference-data entities; recommend splitting the implementation into three sub-trains during execution: types+subtypes, doctors+pricing, operators+specialties+inventory-catalog.
- Phase 05 is the second XL; reception workspace + lock workflow + receipts. The lock-transaction integrity is the highest-risk piece; integration tests must run against a real SQLite WAL pool.
- Phase 08 includes the soak harness which requires an 8h simulated run; budget CI time accordingly or run the soak overnight outside the standard PR pipeline.
