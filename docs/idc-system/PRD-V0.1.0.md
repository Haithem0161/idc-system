# IDC System — Product Requirements Document V0.1.1

المجمع العراقي التخصصي — Operational Software Platform.

## §0 Document History

| Version | Date | Author | Status | Notes |
|-|-|-|-|-|
| 0.1.0 | 2026-05-07 | IDC Engineering | Superseded | Initial PRD covering Reception, Accounting, Inventory, Admin modules; offline-first Tauri desktop + Fastify sync/backup server. Bilingual (ar default + en). |
| 0.1.1 | 2026-05-10 | IDC Engineering | Draft | Per-check Reception workflow lands. Data-model tightening: `visit_lines` removed; check fields (`check_type_id`, `check_subtype_id`, `doctor_id`, `operator_id`, `dye`, `report`, all `*_snapshot_iqd` columns) inlined onto `visits`. A visit is now exactly one check. Cascading updates to §4, §6, §7, §8, §9, §12. |

### Precedent Documents

- `.claude/rules/prd-writing.md` — Section template authority and quality bar.
- `.claude/rules/offline-first.md` — Standard sync columns, conflict-resolution policies, outbox shape.
- `.claude/rules/auth.md` — JWT model, offline-login token caching.
- `.claude/rules/ddd.md` — Domain layering across surfaces.
- `.claude/rules/sync-server.md` — Fastify plugin layout, TypeBox schemas, tenant model.
- `.claude/rules/tauri.md`, `.claude/rules/rust.md`, `.claude/rules/frontend.md` — Surface conventions.

---

## §1 Executive Summary

### §1.1 Overview

The IDC System is the operational software for a single-site Iraqi medical imaging center. It is a Tauri v2 desktop application paired with a Fastify sync/backup server. The desktop app runs at every workstation in the center — minimum two: the reception desk and the accountant's office — and acts as the source of truth for every operational record produced during a working day.

The desktop app covers four surfaces: a receptionist workflow that captures patient visits and locks them with a printed receipt, an accountant workflow that reads back the day's records as detailed financial reports, a simple inventory module that auto-decrements consumables when a check is locked, and a superadmin module that manages all reference data (users, check types, doctors, operators, items, settings).

Network connectivity is opportunistic. Every read goes to local SQLite. Every write commits to local SQLite first. The sync engine batches changes and ships them to the Fastify server when the network is available. A network outage of any duration never blocks the staff. When connectivity returns, the engine drains the outbox and pulls peer changes; conflicts on financial records surface as an explicit resolver screen rather than being merged silently.

The system is bilingual. The default locale is Arabic with right-to-left layout. English is available behind a toggle. There is no hard-coded user-facing string anywhere in the codebase; every label, error, and button resolves through the i18n layer. Domain-data names that the staff configure (check types, subtypes, inventory items) carry both an Arabic and an optional English name. People-names (doctors, patients, operators) are free-form single-string fields entered in whichever script the staff types.

### §1.2 Key Objectives

1. Lock a typical visit (no special routing) in under 30 seconds from check-card click to printed receipt.
2. Attribute every doctor and operator cut deterministically — no silent fallbacks, no ambiguous splits, no after-the-fact reassignment.
3. Audit every business mutation with actor, timestamp, device, and field-level delta; deletes are tombstones, never row removals.
4. Survive any network outage indefinitely; the full feature surface works offline.
5. Replicate changes between two devices in the same center within 5 seconds (p95) of reconnect.
6. Ship Arabic-default bilingual UX with full RTL fidelity, including printed artifacts.
7. Provide a single source of truth across reception and accounting — the accountant sees what the receptionist locked, with no manual reconciliation.
8. Enforce zero hard-coded UI strings; 100% of user-facing text resolves through `react-i18next`.

### §1.3 Success Metrics

| Metric | Target | Measurement Method |
|-|-|-|
| Visit lock time | p95 < 30s | Front-end timing instrumentation, "New Visit" click → receipt-print event. |
| Sync replication | p95 < 5s after reconnect | Server-side timestamp diff: origin `updated_at` vs apply timestamp on second device. |
| Audit coverage | 100% of business mutations | Per-release: count of `audit_log` rows divided by count of repository writes; ratio must equal 1.0. |
| Reconciliation diff | 0 IQD | Daily-close report: sum of locked visit totals minus sum of derived cuts must reconcile to the dinar against the prior-day close. |
| i18n coverage | 100% | Static lint pass: zero literal Arabic or English strings in JSX/TSX outside `src/i18n/locales/`. |
| RTL regression count | 0 | Visual regression tests render every screen at `dir=rtl` and `dir=ltr`. |
| Offline endurance | 8 hours single-device | Soak test: simulated 8-hour shift offline; outbox replays on reconnect with zero data loss. |
| Sync conflict false-merge | 0 | Test suite asserts that `manual`-policy entities never auto-merge. |
| Receipt print success | > 99% | Telemetry: count of lock events with successful `tauri-plugin-dialog` save divided by total lock events. |

### §1.4 Scope Boundaries

| In Scope | Out of Scope | Rationale |
|-|-|-|
| Reception (visit entry, lock, void, operator clock-in) | Appointment scheduling | Center operates walk-in; scheduling deferred to Horizon 1. |
| Accounting (read-only reports, daily close) | Tax filing / VAT integration | No mandatory e-invoicing in the target jurisdiction. |
| Simple inventory with auto-decrement on locks | Multi-warehouse, batch tracking, expiry dates | Single physical stockroom; consumption simple at v1. |
| Admin CRUD on reference data | Bulk import wizards | Manual entry acceptable at v1 catalog sizes (~50 doctors, ~10 check types). |
| Audit log over every business write | DICOM/PACS imaging, clinical reporting | Out of operational scope; clinical workflow lives outside this app. |
| Bilingual UI (ar default, en) | Other locales | Iraqi center; staff cover ar + en only. |
| Offline-first with cross-device sync within one center | Multi-branch / multi-tenant in real sense | Single center; tenant column kept for forward-compat. |
| RBAC: Superadmin, Receptionist, Accountant | Operator self-login | Operators are tracked records, not system users. |
| Receipts (A5 + thermal) printed at lock | Patient SMS / email notifications | Deferred to Horizon 1; needs comms infrastructure. |
| Refunds via void | Refund as a separate ledger record | Voiding the visit reverses everything; standalone refund records deferred to Horizon 1. |
| Visit financial reports | Insurance claims | No insurance integration in v1. |
| Cross-device sync within one center | Offline mobile companion | Desktop only in v1. |

### §1.5 Target Users & Personas

Primary:

| Persona | Key Needs | Use Case |
|-|-|-|
| Maha (Receptionist) | Enter a patient and 1-3 checks in under a minute; never lose a visit to a network blip; print a receipt while the patient waits; clock operators in/out. | "I create the visit, type the doctor's name (or leave it empty), tick صبغة if asked, lock, print, hand to the patient." |
| Karrar (Accountant) | End-of-day numbers that match the cash drawer; per-doctor and per-operator earnings broken down by check type and time range; ability to drill from any aggregate to the source visit. | "At 7pm I run the daily close. If the diff is anything other than zero, I want to see exactly which visit caused it." |
| Dr. Sami (Superadmin / Owner) | Configure pricing and roles; audit anything anyone did; void erroneous visits; see the audit log; manage inventory. | "Last Wednesday someone changed the dye cost. I want to see who, when, and what the old value was." |

Secondary:

| Persona | Key Needs | Use Case |
|-|-|-|
| Operator (radiology technician) | Get paid the correct cut at the end of the month. Not a system user. | "I clock in with Maha at 8am, run the machine, clock out at 4pm. My earnings show on Karrar's report." |

### §1.6 Technology Stack

| Layer | Tooling | Rule reference |
|-|-|-|
| Desktop runtime | Tauri v2 | `.claude/rules/tauri.md` |
| Native code | Rust + sqlx + tokio + tracing + thiserror | `.claude/rules/rust.md` |
| Local database | SQLite via `tauri-plugin-sql` | `.claude/rules/offline-first.md` |
| Frontend | React 19 + Vite + TypeScript | `.claude/rules/frontend.md` |
| Routing | React Router v7 | `REACT-ROUTER.md` |
| Server state | TanStack Query v5 | `REACT-QUERY.md` |
| Client state | Zustand v5 | `ZUSTAND.md` |
| Validation | Zod v4 | `ZOD.md` |
| Styling | Tailwind v4 + shadcn/ui | `TAILWIND.md`, `SHADCN.md` |
| Animation | framer-motion | `FRAMER-MOTION.md` |
| i18n | react-i18next, ar (default) + en | `I18N.md`, this PRD §10.6 |
| HTTP (frontend) | axios typed instance | `AXIOS.md` |
| Sync server | Fastify + Prisma + Postgres 16 | `.claude/rules/sync-server.md` |
| Server validation | TypeBox + Swagger | `.claude/rules/sync-server.md` |
| Auth | RS256 JWT, offline-cached creds via Tauri stronghold | `.claude/rules/auth.md` |
| Sync engine | Custom Tokio task with outbox | `.claude/rules/offline-first.md` |
| Background jobs (server) | BullMQ | `.claude/rules/sync-server.md` |

IDC-specific deviations from the platform stack: none in v1. The app embraces the platform stack as-is.

---

## §2 Module Packaging & Entitlements

### §2.1 Package Definitions

Single bundled IDC desktop app, single license per center. No paid/free tiers, no module marketplace, no per-seat licensing in v1. The sync server is a single instance per center, self-hosted by the operator.

### §2.2 Entitlement Behavior

Every authenticated user has access to surfaces gated by their role assignment. A user with `is_active = false` cannot log in. A user whose role enum value is missing or unknown lands on a `/no-access` stub with a "contact your administrator" message. There is no in-app upsell, no paywall, no feature flag for paid/free splits.

### §2.3 Surface Summary

The app shell renders a sidebar (right-aligned in RTL Arabic, left-aligned in LTR English) with a role-gated navigation tree:

```
Sidebar
  Reception        receptionist, superadmin
  Accounting       accountant, superadmin
  Inventory        receptionist (limited), accountant (read), superadmin (full)
  Admin            superadmin only
  Audit            superadmin only
```

Status bar (bottom of every page): sync status pill, current user + role, language toggle (ar/en), lock-screen action.

---

## §3 Application Architecture

### §3.1 Navigation Tree

```
/login
/                                redirect by role: superadmin -> /reception
                                                   receptionist -> /reception
                                                   accountant -> /accounting
/reception                       receptionist, superadmin   (Checks Grid)
  /reception/checks/:slug                                       (Check Workspace)
  /reception/checks/:slug/new                                   (New Visit)
  /reception/visits/:id                                         (Visit Detail)
  /reception/shifts                                             (Operator Shifts)
/accounting                      accountant, superadmin
  /accounting                    dashboard
  /accounting/visits
  /accounting/visits/:id
  /accounting/doctors
  /accounting/doctors/:id
  /accounting/operators
  /accounting/operators/:id
  /accounting/daily-close
/inventory                       receptionist (read+adjust), accountant (read), superadmin
  /inventory
  /inventory/items/:id
  /inventory/adjust
/admin                           superadmin only
  /admin/users
  /admin/users/:id
  /admin/check-types
  /admin/check-types/:id
  /admin/doctors
  /admin/doctors/:id
  /admin/operators
  /admin/operators/:id
  /admin/inventory
  /admin/inventory/:id
  /admin/settings
/audit                           superadmin only
/no-access                       fallback for unknown role
```

### §3.2 Page Count Summary

| Module | Pages | Notes |
|-|-|-|
| Reception | 5 | Checks grid, check workspace, new visit, visit detail, operator shifts. |
| Accounting | 7 | Dashboard, visits list + detail, doctors list + detail, operators list + detail, daily close. |
| Inventory | 3 | List, item detail, adjust. |
| Admin | 11 | Users list + detail, check-types list + detail, doctors list + detail, operators list + detail, inventory list + detail, settings. |
| Audit | 1 | Search/filter page. |
| Auth/system | 2 | Login, no-access. |
| **Total** | **29** | Drives milestone sizing. |

### §3.3 Navigation Pattern

- Persistent sidebar (mirrored in RTL), top app-bar with user menu and sync pill, main pane.
- Lists in module roots; detail accessed via dedicated sub-routes (not modals) so refresh + deep-link works.
- Admin uses a macOS System-Settings nesting: a secondary sub-sidebar inside `/admin/*` lists the eleven admin areas; the main pane shows the active sub-page.
- Tabs are reserved for in-page sectioning (e.g., visit detail: "Lines | Audit | Receipts"); they never replace routes.

---

## §4 Core Architectural Patterns

### §4.1 Lock-Then-Snapshot Pricing

A `visits` row is `draft` until the receptionist clicks Lock. At lock time the system writes the price snapshots onto the visit row itself: `price_snapshot_iqd`, `dye_cost_snapshot_iqd`, `report_cost_snapshot_iqd`, `doctor_cut_snapshot_iqd`, `operator_cut_snapshot_iqd`, `internal_pct_snapshot`, `total_amount_iqd_snapshot`. Subsequent admin changes to `check_types`, `check_subtypes`, `doctor_check_pricing`, or `settings` do NOT mutate locked visits. Accounting reports always read the snapshots, never the live prices. See §6.1.10 (visits) and §8.1 (lock workflow).

### §4.2 Operator Attribution at Lock

A draft visit may be saved with no operator. Lock requires the visit to have an `operator_id` chosen from the set of operators who are currently clocked in (`operator_shifts.check_in_at <= now AND check_out_at IS NULL`) and whose specialties (`operator_specialties`) cover the visit's `check_type_id`. If the qualifying set is empty, lock fails with the domain error `OperatorAttribution::NoQualifiedOperator` and the UI shows "No qualified operator on shift for <check type>." Lock NEVER auto-assigns silently.

### §4.3 Audit-First Writes

Every domain service performs writes through a transactional `with_audit(actor, action, entity, entity_id) { … }` helper. The helper opens a SQLite transaction, captures the `before` snapshot, runs the mutation, computes the field-level delta `{ field: { from, to } }`, writes the `audit_log` row, and commits. Any error rolls back both the mutation and the audit row. Bare repository writes outside this helper are a code-review reject.

### §4.4 Inventory Consumption Ledger

Stock counts are derived. `inventory_items.quantity_on_hand` is materialized for fast reads, but every change is recorded as an `inventory_adjustments` row with reason in `{receive, writeoff, count_correction, consume_visit}`. On visit lock, the system iterates the visit's matching `inventory_consumption_map` rows and writes one negative-delta `inventory_adjustments` row per consumed item, all in the same transaction as the lock. Voiding a visit writes offsetting positive-delta rows referencing the same `visit_id`. The materialized `quantity_on_hand` is recomputed in the same transaction.

### §4.5 Bilingual-by-Construction

Domain entities that surface in lists and forms — `check_types`, `check_subtypes`, `inventory_items` — carry both `name_ar` (NOT NULL) and `name_en` (nullable). The frontend resolves display strings via `i18n.language`; if the active locale is `en` and `name_en` is null, the system falls back to `name_ar` so nothing renders blank. Free-form names (people: doctors, patients, operators) use a single `name` column in whichever script the staff types — they are not translated.

---

## §5 Surface Integration

### §5.1 Tauri / Rust

| Capability | Cover |
|-|-|
| IPC commands | One module of commands per feature area (`auth`, `users`, `check_types`, `doctors`, `operators`, `shifts`, `patients`, `visits`, `inventory`, `settings`, `audit`, `sync`, `reports`). Approximate count at v1: 55 commands. Detailed tables in §7 per module. |
| Plugins | `tauri-plugin-sql` (SQLite), `tauri-plugin-store` (sync cursor + per-device UI prefs), `tauri-plugin-dialog` (save-as-PDF receipts, pick-file for backup restore), `tauri-plugin-log` (rolling file logs), `tauri-plugin-stronghold` (refresh + creds cache), `tauri-plugin-os` (device id, locale detection on first launch). |
| Capabilities | `fs:scope: $APPDATA/idc-system/receipts/**`, `fs:scope: $APPDATA/idc-system/logs/**`, `dialog:save`, `dialog:open`, `store:default`, `stronghold:default`, `os:default`. No bare `http` capability — sync HTTP goes through the in-process sync engine using `reqwest` linked into the Rust backend. |
| Runtime state | `AppState { db_pool, sync_engine_handle, user_context, settings_cache, device_id }` in `src-tauri/src/state.rs`. |
| Logging | `tracing` with a JSON file appender; PII-bearing fields (patient name, password) are redacted via a custom `tracing` layer. |

Boundary: the Rust layer NEVER calls the sync server directly from a command handler. Mutations enqueue to the local outbox; the sync engine ships them. Reads always hit local SQLite. The server is reachable only from the sync engine task.

### §5.2 Sync Server

| Method | Path | Description |
|-|-|-|
| `POST` | `/auth/login` | Email + password → `{ accessToken, refreshToken, user, role, publicKey }`. |
| `POST` | `/auth/refresh` | Refresh-token rotation. Old refresh token is revoked atomically. |
| `POST` | `/auth/logout` | Server-side refresh-token revocation. |
| `POST` | `/auth/change-password` | Online-only; updates server hash and triggers client cache refresh on next login. |
| `POST` | `/sync/push` | Outbox batch up to 50 ops; idempotent by `op_id`; per-entity validation; conflict response on financial entities. |
| `GET` | `/sync/pull` | `?since=<cursor>` returns changes after cursor; new cursor in response body. |
| `POST` | `/sync/conflicts/:opId/resolve` | Submits a manual resolution for a conflict surfaced by `/sync/push`. |
| `GET` | `/audit/query` | Server-side audit search (admin-only); paged; full-text on entity + actor. |
| `GET` | `/reports/visits` | Server-side rollup if local query exceeds threshold; aggregate rows. |
| `GET` | `/reports/daily-close/:date` | Authoritative daily totals, signed by server with the same RS256 key. |
| `GET` | `/healthz` | Liveness; no auth. |

All routes carry full TypeBox schemas and Swagger metadata per `.claude/rules/sync-server.md`. JWT enforced via Fastify auth plugin on every non-`/healthz` route. `request.tenantId` injected from JWT claim; every Prisma query filters by it.

Boundary: the server does NOT generate IDs for syncable entities — all IDs are client-supplied UUID v7. The server validates the ID format and rejects collisions across tenants. Server-only IDs (e.g., refresh-token rows) live in their own non-syncable tables.

### §5.3 Reserved

Reserved. (Former content removed; the IDC system ships as a standalone Tauri app.)

### §5.4 Document Center / Storage

Receipts persist to disk under `$APPDATA/idc-system/receipts/<YYYY>/<MM>/<visit-id>.pdf` plus a thermal-print version at `$APPDATA/idc-system/receipts/thermal/<visit-id>.txt`. No upload to a Document Center service in v1; backup is via the sync server's nightly database snapshot (Horizon 1 introduces a centralized receipt archive).

### §5.5 Auth

- RS256 JWT issued by the sync server. Public key fetched at app start and pinned in stronghold.
- Access token lifetime: 15 minutes. Refresh token lifetime: 30 days.
- Offline login: on every successful online login, the app caches an Argon2id-hashed copy of the user's password in stronghold (per `.claude/rules/auth.md`). Subsequent offline logins compare against the cached hash. Cache invalidates on any successful online password change.
- Role on JWT claim (`role`) and server-side enforced. Rust commands also enforce — claims alone are not trusted by the server because the local user context is established post-login.
- Lock screen: ten minutes of input idleness locks the app to a re-auth prompt; works offline.

---

## §6 Data Model

Every syncable table includes the standard sync columns from `.claude/rules/offline-first.md`:

```sql
id                TEXT PRIMARY KEY,             -- UUID v7, client-generated
created_at        TEXT NOT NULL,                -- RFC3339 UTC
updated_at        TEXT NOT NULL,                -- RFC3339 UTC, bumped on every mutation
deleted_at        TEXT NULL,                    -- tombstone marker
version           INTEGER NOT NULL DEFAULT 0,   -- per-row monotonic
dirty             INTEGER NOT NULL DEFAULT 1,   -- 1 = needs push
last_synced_at    TEXT NULL,
origin_device_id  TEXT NULL,
entity_id         TEXT NOT NULL                 -- tenant scope; v1 single-tenant
```

Server Prisma models include the same columns (`createdAt`, `updatedAt`, `deletedAt`, `version`, `lastSyncedAt`, `originDeviceId`, `entityId`) plus a server-only `pulledAt` timestamp.

The `outbox` table is shared infrastructure (specified in `.claude/rules/offline-first.md`) and is not redefined here.

### §6.1 Entity Definitions

#### §6.1.1 users

A user is an authenticated subject. Three roles exist; assignment determines which surfaces the app shell exposes.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| email | TEXT UNIQUE | yes | yes | login identifier; lowercase normalized at write. |
| name | TEXT | yes | yes | display name. |
| password_hash | TEXT | yes | no | Argon2id; server is canonical, client mirror for offline auth. |
| role | TEXT | yes | yes | one of `superadmin`, `receptionist`, `accountant`. |
| is_active | INTEGER | yes | no | 0 = disabled, cannot log in. |
| last_login_at | TEXT | no | no | informational. |

**Local Schema (SQLite)**

```sql
CREATE TABLE users (
  id                TEXT PRIMARY KEY,
  email             TEXT NOT NULL,
  name              TEXT NOT NULL,
  password_hash     TEXT NOT NULL,
  role              TEXT NOT NULL CHECK (role IN ('superadmin','receptionist','accountant')),
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  last_login_at     TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX users_email_unique ON users(entity_id, email) WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model User {
  id              String    @id
  email           String
  name            String
  passwordHash    String    @map("password_hash")
  role            UserRole
  isActive        Boolean   @default(true) @map("is_active")
  lastLoginAt     DateTime? @map("last_login_at") @db.Timestamptz
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  @@unique([entityId, email], name: "user_email_unique")
  @@map("users")
}

enum UserRole {
  superadmin
  receptionist
  accountant
}
```

**Invariants**

1. `email` is unique within `entity_id` among non-deleted rows.
2. `role` is one of the three enum values; no nulls.
3. A user with `is_active = 0` cannot authenticate.
4. Soft-delete sets `deleted_at` and `is_active = 0` atomically.

**Sync Policy:** `last-write-wins`. Rationale: user records are edited rarely and from one place at a time (admin screen). Field-merge is overkill; manual is too friction-heavy.

#### §6.1.2 check_types

A type of diagnostic check (e.g., سونار, مفراس). Either has a flat price OR has subtypes that carry the price; never both.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| name_ar | TEXT | yes | yes | Arabic display name. |
| name_en | TEXT | no | yes | English display name; falls back to `name_ar`. |
| has_subtypes | INTEGER | yes | no | 0 or 1; if 1, `base_price_iqd` MUST be NULL. |
| base_price_iqd | INTEGER | conditional | no | flat price if `has_subtypes = 0`, else NULL. IQD whole units. |
| dye_supported | INTEGER | yes | no | 1 = receptionist may toggle dye on lines of this type. |
| report_supported | INTEGER | yes | no | 1 = receptionist may toggle report on lines of this type. |
| sort_order | INTEGER | yes | no | for stable list ordering; default 0. |

**Local Schema (SQLite)**

```sql
CREATE TABLE check_types (
  id                TEXT PRIMARY KEY,
  name_ar           TEXT NOT NULL,
  name_en           TEXT NULL,
  has_subtypes      INTEGER NOT NULL CHECK (has_subtypes IN (0,1)),
  base_price_iqd    INTEGER NULL,
  dye_supported     INTEGER NOT NULL DEFAULT 0 CHECK (dye_supported IN (0,1)),
  report_supported  INTEGER NOT NULL DEFAULT 0 CHECK (report_supported IN (0,1)),
  sort_order        INTEGER NOT NULL DEFAULT 0,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL,
  CHECK (
    (has_subtypes = 1 AND base_price_iqd IS NULL) OR
    (has_subtypes = 0 AND base_price_iqd IS NOT NULL AND base_price_iqd >= 0)
  )
);
CREATE INDEX check_types_sort ON check_types(entity_id, sort_order) WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model CheckType {
  id              String    @id
  nameAr          String    @map("name_ar")
  nameEn          String?   @map("name_en")
  hasSubtypes     Boolean   @map("has_subtypes")
  basePriceIqd    Int?      @map("base_price_iqd")
  dyeSupported    Boolean   @default(false) @map("dye_supported")
  reportSupported Boolean   @default(false) @map("report_supported")
  sortOrder       Int       @default(0) @map("sort_order")
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  subtypes              CheckSubtype[]
  doctorPricings        DoctorCheckPricing[]
  inventoryConsumption  InventoryConsumptionMap[]
  operatorSpecialties   OperatorSpecialty[]
  visits                Visit[]

  @@map("check_types")
}
```

**Invariants**

1. XOR rule: exactly one of (`has_subtypes = 1` with no `base_price_iqd`) or (`has_subtypes = 0` with a non-negative `base_price_iqd`).
2. Toggling `has_subtypes` from `0` to `1` requires `base_price_iqd = NULL` first; UI enforces atomically.
3. Toggling `has_subtypes` from `1` to `0` is blocked if any non-deleted `check_subtypes` rows reference this type.
4. `dye_supported` and `report_supported` are independent toggles.

**Sync Policy:** `last-write-wins`. Rationale: low edit frequency; admin-only.

#### §6.1.3 check_subtypes

A subtype of a check type that has subtypes. Carries its own price.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| check_type_id | TEXT FK | yes | no | parent type. |
| name_ar | TEXT | yes | yes | Arabic display name. |
| name_en | TEXT | no | yes | English; falls back. |
| price_iqd | INTEGER | yes | no | non-negative. |
| sort_order | INTEGER | yes | no | default 0. |

**Local Schema (SQLite)**

```sql
CREATE TABLE check_subtypes (
  id                TEXT PRIMARY KEY,
  check_type_id     TEXT NOT NULL REFERENCES check_types(id),
  name_ar           TEXT NOT NULL,
  name_en           TEXT NULL,
  price_iqd         INTEGER NOT NULL CHECK (price_iqd >= 0),
  sort_order        INTEGER NOT NULL DEFAULT 0,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE INDEX check_subtypes_type ON check_subtypes(check_type_id) WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model CheckSubtype {
  id              String    @id
  checkTypeId     String    @map("check_type_id")
  nameAr          String    @map("name_ar")
  nameEn          String?   @map("name_en")
  priceIqd        Int       @map("price_iqd")
  sortOrder       Int       @default(0) @map("sort_order")
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  checkType             CheckType                 @relation(fields: [checkTypeId], references: [id])
  doctorPricings        DoctorCheckPricing[]
  inventoryConsumption  InventoryConsumptionMap[]
  visits                Visit[]

  @@map("check_subtypes")
}
```

**Invariants**

1. Parent `check_types.has_subtypes` must equal `1` at write time (enforced in service layer).
2. `price_iqd` is non-negative.
3. Soft-delete is allowed if no non-deleted `visits` reference the subtype with `status != voided`.

**Sync Policy:** `last-write-wins`.

#### §6.1.4 doctors

An external referring doctor. The "house"/internal case is NOT a row — it is the absence of a `doctor_id` on a visit.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| name | TEXT | yes | yes | free-form, single string. |
| specialty | TEXT | no | yes | free-form. |
| phone | TEXT | no | yes | optional contact. |
| is_active | INTEGER | yes | no | inactive doctors do not appear in receptionist autocomplete. |
| notes | TEXT | no | no | free-form. |

**Local Schema (SQLite)**

```sql
CREATE TABLE doctors (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  specialty         TEXT NULL,
  phone             TEXT NULL,
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  notes             TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE INDEX doctors_name ON doctors(entity_id, name) WHERE deleted_at IS NULL;
CREATE VIRTUAL TABLE doctors_fts USING fts5(name, specialty, content='doctors', content_rowid='rowid');
```

**Server Schema (Prisma)**

```prisma
model Doctor {
  id              String    @id
  name            String
  specialty       String?
  phone           String?
  isActive        Boolean   @default(true) @map("is_active")
  notes           String?
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  pricings    DoctorCheckPricing[]
  visits      Visit[]

  @@index([entityId, name])
  @@map("doctors")
}
```

**Invariants**

1. `name` must be non-empty after trim.
2. Inactive doctors remain selectable on existing draft visits but do not appear in new-line autocompletion.
3. Soft-delete cascades soft-deletes all `doctor_check_pricing` rows for this doctor.

**Sync Policy:** `last-write-wins`.

#### §6.1.5 doctor_check_pricing

The doctor's price and cut for a specific check type (or specific subtype within a typed-with-subtypes check type). One row per (doctor, check_type, check_subtype?).

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| doctor_id | TEXT FK | yes | no |  |
| check_type_id | TEXT FK | yes | no |  |
| check_subtype_id | TEXT FK | conditional | no | NULL when type has no subtypes; required when type has subtypes. |
| price_override_iqd | INTEGER | no | no | optional override of `check_types.base_price_iqd` or `check_subtypes.price_iqd` for visits booked through this doctor. |
| cut_kind | TEXT | yes | no | `pct` or `fixed`. |
| cut_value | INTEGER | yes | no | percent (0-100) if `pct`; IQD if `fixed`. |

**Local Schema (SQLite)**

```sql
CREATE TABLE doctor_check_pricing (
  id                  TEXT PRIMARY KEY,
  doctor_id           TEXT NOT NULL REFERENCES doctors(id),
  check_type_id       TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id    TEXT NULL REFERENCES check_subtypes(id),
  price_override_iqd  INTEGER NULL,
  cut_kind            TEXT NOT NULL CHECK (cut_kind IN ('pct','fixed')),
  cut_value           INTEGER NOT NULL CHECK (cut_value >= 0),
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  deleted_at          TEXT NULL,
  version             INTEGER NOT NULL DEFAULT 0,
  dirty               INTEGER NOT NULL DEFAULT 1,
  last_synced_at      TEXT NULL,
  origin_device_id    TEXT NULL,
  entity_id           TEXT NOT NULL,
  CHECK (cut_kind != 'pct' OR cut_value <= 100),
  CHECK (price_override_iqd IS NULL OR price_override_iqd >= 0)
);
CREATE UNIQUE INDEX doctor_check_pricing_unique
  ON doctor_check_pricing(doctor_id, check_type_id, IFNULL(check_subtype_id,''))
  WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model DoctorCheckPricing {
  id                String    @id
  doctorId          String    @map("doctor_id")
  checkTypeId       String    @map("check_type_id")
  checkSubtypeId    String?   @map("check_subtype_id")
  priceOverrideIqd  Int?      @map("price_override_iqd")
  cutKind           CutKind   @map("cut_kind")
  cutValue          Int       @map("cut_value")
  createdAt         DateTime  @map("created_at") @db.Timestamptz
  updatedAt         DateTime  @map("updated_at") @db.Timestamptz
  deletedAt         DateTime? @map("deleted_at") @db.Timestamptz
  version           Int       @default(0)
  lastSyncedAt      DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId    String?   @map("origin_device_id")
  entityId          String    @map("entity_id")

  doctor        Doctor        @relation(fields: [doctorId], references: [id])
  checkType     CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype  CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])

  @@unique([doctorId, checkTypeId, checkSubtypeId])
  @@map("doctor_check_pricing")
}

enum CutKind {
  pct
  fixed
}
```

**Invariants**

1. Uniqueness on `(doctor_id, check_type_id, check_subtype_id)` among non-deleted rows.
2. If the parent `check_types.has_subtypes = 1`, then `check_subtype_id` must be non-null.
3. If the parent `check_types.has_subtypes = 0`, then `check_subtype_id` must be null.
4. `cut_kind = 'pct'` constrains `cut_value` to `[0, 100]`.
5. `price_override_iqd`, when present, replaces the type/subtype price ONLY for visits booked under this doctor; type/subtype default applies otherwise.

**Sync Policy:** `last-write-wins`.

#### §6.1.6 operators

A radiology technician. Tracked for payroll. Not a system user.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| name | TEXT | yes | yes | free-form. |
| phone | TEXT | no | yes | optional. |
| base_cut_per_check_iqd | INTEGER | yes | no | flat IQD per check; doubled when dye is on the line. |
| is_active | INTEGER | yes | no | inactive operators do not appear in clock-in lists. |
| notes | TEXT | no | no |  |

**Local Schema (SQLite)**

```sql
CREATE TABLE operators (
  id                       TEXT PRIMARY KEY,
  name                     TEXT NOT NULL,
  phone                    TEXT NULL,
  base_cut_per_check_iqd   INTEGER NOT NULL CHECK (base_cut_per_check_iqd >= 0),
  is_active                INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  notes                    TEXT NULL,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  deleted_at               TEXT NULL,
  version                  INTEGER NOT NULL DEFAULT 0,
  dirty                    INTEGER NOT NULL DEFAULT 1,
  last_synced_at           TEXT NULL,
  origin_device_id         TEXT NULL,
  entity_id                TEXT NOT NULL
);
```

**Server Schema (Prisma)**

```prisma
model Operator {
  id                    String    @id
  name                  String
  phone                 String?
  baseCutPerCheckIqd    Int       @map("base_cut_per_check_iqd")
  isActive              Boolean   @default(true) @map("is_active")
  notes                 String?
  createdAt             DateTime  @map("created_at") @db.Timestamptz
  updatedAt             DateTime  @map("updated_at") @db.Timestamptz
  deletedAt             DateTime? @map("deleted_at") @db.Timestamptz
  version               Int       @default(0)
  lastSyncedAt          DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId        String?   @map("origin_device_id")
  entityId              String    @map("entity_id")

  specialties OperatorSpecialty[]
  shifts      OperatorShift[]
  visits      Visit[]

  @@map("operators")
}
```

**Invariants**

1. `name` non-empty after trim.
2. `base_cut_per_check_iqd` non-negative.
3. Soft-delete blocks while any open shift (`check_out_at IS NULL`) exists; admin must clock out first.

**Sync Policy:** `last-write-wins`.

#### §6.1.7 operator_specialties

Junction: which check types an operator is qualified to run.

**Local Schema (SQLite)**

```sql
CREATE TABLE operator_specialties (
  id                TEXT PRIMARY KEY,
  operator_id       TEXT NOT NULL REFERENCES operators(id),
  check_type_id    TEXT NOT NULL REFERENCES check_types(id),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX operator_specialties_unique
  ON operator_specialties(operator_id, check_type_id)
  WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model OperatorSpecialty {
  id              String    @id
  operatorId      String    @map("operator_id")
  checkTypeId     String    @map("check_type_id")
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  operator    Operator   @relation(fields: [operatorId], references: [id])
  checkType   CheckType  @relation(fields: [checkTypeId], references: [id])

  @@unique([operatorId, checkTypeId])
  @@map("operator_specialties")
}
```

**Invariants**

1. Unique on `(operator_id, check_type_id)` among non-deleted rows.

**Sync Policy:** `last-write-wins`.

#### §6.1.8 operator_shifts

A clock-in/out span for an operator.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| operator_id | TEXT FK | yes | no |  |
| check_in_at | TEXT | yes | yes | RFC3339 UTC. |
| check_out_at | TEXT | no | yes | NULL = on shift. |
| check_in_by_user_id | TEXT FK | yes | no | the receptionist or superadmin who clocked them in. |
| check_out_by_user_id | TEXT FK | no | no |  |
| note | TEXT | no | no |  |

**Local Schema (SQLite)**

```sql
CREATE TABLE operator_shifts (
  id                       TEXT PRIMARY KEY,
  operator_id              TEXT NOT NULL REFERENCES operators(id),
  check_in_at              TEXT NOT NULL,
  check_out_at             TEXT NULL,
  check_in_by_user_id      TEXT NOT NULL REFERENCES users(id),
  check_out_by_user_id     TEXT NULL REFERENCES users(id),
  note                     TEXT NULL,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  deleted_at               TEXT NULL,
  version                  INTEGER NOT NULL DEFAULT 0,
  dirty                    INTEGER NOT NULL DEFAULT 1,
  last_synced_at           TEXT NULL,
  origin_device_id         TEXT NULL,
  entity_id                TEXT NOT NULL,
  CHECK (check_out_at IS NULL OR check_out_at >= check_in_at)
);
CREATE INDEX operator_shifts_open
  ON operator_shifts(operator_id)
  WHERE check_out_at IS NULL AND deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model OperatorShift {
  id                  String    @id
  operatorId          String    @map("operator_id")
  checkInAt           DateTime  @map("check_in_at") @db.Timestamptz
  checkOutAt          DateTime? @map("check_out_at") @db.Timestamptz
  checkInByUserId     String    @map("check_in_by_user_id")
  checkOutByUserId    String?   @map("check_out_by_user_id")
  note                String?
  createdAt           DateTime  @map("created_at") @db.Timestamptz
  updatedAt           DateTime  @map("updated_at") @db.Timestamptz
  deletedAt           DateTime? @map("deleted_at") @db.Timestamptz
  version             Int       @default(0)
  lastSyncedAt        DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId      String?   @map("origin_device_id")
  entityId            String    @map("entity_id")

  operator         Operator @relation(fields: [operatorId], references: [id])
  checkInByUser    User     @relation("ShiftCheckIn", fields: [checkInByUserId], references: [id])
  checkOutByUser   User?    @relation("ShiftCheckOut", fields: [checkOutByUserId], references: [id])

  @@map("operator_shifts")
}
```

**Invariants**

1. `check_out_at >= check_in_at` if both present.
2. At most one open shift per `operator_id` at a time. Enforced by the partial index above plus a service-layer pre-write check.
3. Retroactive edits to `check_in_at`/`check_out_at` are allowed only for superadmins and emit an audit event.

**State Machine**

```
+-------+   clock-in    +------+   clock-out   +--------+
| (n/a) | ------------> | open | ------------> | closed |
+-------+               +------+               +--------+
                           |                       ^
                           +-- superadmin edit ----+
```

**Sync Policy:** `additive-only`. Rationale: shifts are append-mostly; both devices recording independently never conflict because each device clocks-in via a unique action. Retroactive edits are rare and audited.

#### §6.1.9 patients

A patient record. v1 holds only the quadripartite name. No identity dedupe.

**Local Schema (SQLite)**

```sql
CREATE TABLE patients (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE VIRTUAL TABLE patients_fts USING fts5(name, content='patients', content_rowid='rowid');
```

**Server Schema (Prisma)**

```prisma
model Patient {
  id              String    @id
  name            String
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityId        String    @map("entity_id")

  visits Visit[]

  @@map("patients")
}
```

**Invariants**

1. `name` non-empty after trim.
2. No uniqueness — a returning patient yields a new row in v1.

**Sync Policy:** `last-write-wins`.

#### §6.1.10 visits

A patient encounter for exactly one check. Carries the patient header, the chosen check, the dye/report flags, the doctor and operator, and all financial snapshots taken at lock. v1 is single-check by design; multi-check bookings are Horizon-1 (see §11.1).

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| patient_id | TEXT FK | yes | no |  |
| status | TEXT | yes | yes | `draft`, `locked`, `voided`. |
| receptionist_user_id | TEXT FK | yes | no | who created the visit. |
| check_type_id | TEXT FK | yes | yes | the check this visit is for. |
| check_subtype_id | TEXT FK | conditional | yes | required when `check_types.has_subtypes = 1`. |
| doctor_id | TEXT FK | no | yes | NULL = house (in-house). |
| operator_id | TEXT FK | conditional | no | required at lock; chosen from clocked-in operators with matching specialty. |
| dye | INTEGER | yes | yes | 0 or 1; gated by `check_types.dye_supported`. |
| report | INTEGER | yes | yes | 0 or 1; gated by `check_types.report_supported`. |
| locked_at | TEXT | no | yes | RFC3339; set on `draft -> locked`. |
| voided_at | TEXT | no | yes | RFC3339; set on `locked -> voided`. |
| voided_by_user_id | TEXT FK | conditional | no | required when voided. |
| void_reason | TEXT | conditional | no | required when voided. |
| price_snapshot_iqd | INTEGER | conditional | no | set at lock; NULL while draft. |
| dye_cost_snapshot_iqd | INTEGER | conditional | no | set at lock; 0 when `dye = 0`. |
| report_cost_snapshot_iqd | INTEGER | conditional | no | set at lock; 0 when `report = 0`. |
| doctor_cut_snapshot_iqd | INTEGER | conditional | no | set at lock. |
| operator_cut_snapshot_iqd | INTEGER | conditional | no | set at lock. |
| internal_pct_snapshot | INTEGER | conditional | no | set at lock when `doctor_id IS NULL`; captures `settings.internal_doctor_pct`. |
| total_amount_iqd_snapshot | INTEGER | conditional | no | `price + dye_cost + report_cost` at lock. |

**Local Schema (SQLite)**

```sql
CREATE TABLE visits (
  id                          TEXT PRIMARY KEY,
  patient_id                  TEXT NOT NULL REFERENCES patients(id),
  status                      TEXT NOT NULL CHECK (status IN ('draft','locked','voided')),
  receptionist_user_id        TEXT NOT NULL REFERENCES users(id),
  check_type_id               TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id            TEXT NULL REFERENCES check_subtypes(id),
  doctor_id                   TEXT NULL REFERENCES doctors(id),
  operator_id                 TEXT NULL REFERENCES operators(id),
  dye                         INTEGER NOT NULL DEFAULT 0 CHECK (dye IN (0,1)),
  report                      INTEGER NOT NULL DEFAULT 0 CHECK (report IN (0,1)),
  locked_at                   TEXT NULL,
  voided_at                   TEXT NULL,
  voided_by_user_id           TEXT NULL REFERENCES users(id),
  void_reason                 TEXT NULL,
  price_snapshot_iqd          INTEGER NULL,
  dye_cost_snapshot_iqd       INTEGER NULL,
  report_cost_snapshot_iqd    INTEGER NULL,
  doctor_cut_snapshot_iqd     INTEGER NULL,
  operator_cut_snapshot_iqd   INTEGER NULL,
  internal_pct_snapshot       INTEGER NULL,
  total_amount_iqd_snapshot   INTEGER NULL,
  created_at                  TEXT NOT NULL,
  updated_at                  TEXT NOT NULL,
  deleted_at                  TEXT NULL,
  version                     INTEGER NOT NULL DEFAULT 0,
  dirty                       INTEGER NOT NULL DEFAULT 1,
  last_synced_at              TEXT NULL,
  origin_device_id            TEXT NULL,
  entity_id                   TEXT NOT NULL,
  CHECK (
    (status = 'draft'  AND locked_at IS NULL AND voided_at IS NULL
                       AND price_snapshot_iqd IS NULL
                       AND total_amount_iqd_snapshot IS NULL) OR
    (status = 'locked' AND locked_at IS NOT NULL AND voided_at IS NULL
                       AND operator_id IS NOT NULL
                       AND price_snapshot_iqd IS NOT NULL
                       AND dye_cost_snapshot_iqd IS NOT NULL
                       AND report_cost_snapshot_iqd IS NOT NULL
                       AND doctor_cut_snapshot_iqd IS NOT NULL
                       AND operator_cut_snapshot_iqd IS NOT NULL
                       AND total_amount_iqd_snapshot IS NOT NULL) OR
    (status = 'voided' AND locked_at IS NOT NULL AND voided_at IS NOT NULL
                       AND voided_by_user_id IS NOT NULL AND void_reason IS NOT NULL)
  )
);
CREATE INDEX visits_status_date    ON visits(entity_id, status, locked_at);
CREATE INDEX visits_check_type     ON visits(entity_id, check_type_id, locked_at) WHERE deleted_at IS NULL;
CREATE INDEX visits_doctor         ON visits(entity_id, doctor_id, locked_at)     WHERE deleted_at IS NULL AND doctor_id IS NOT NULL;
CREATE INDEX visits_operator       ON visits(entity_id, operator_id, locked_at)   WHERE deleted_at IS NULL AND operator_id IS NOT NULL;
```

**Server Schema (Prisma)**

```prisma
model Visit {
  id                        String       @id
  patientId                 String       @map("patient_id")
  status                    VisitStatus
  receptionistUserId        String       @map("receptionist_user_id")
  checkTypeId               String       @map("check_type_id")
  checkSubtypeId            String?      @map("check_subtype_id")
  doctorId                  String?      @map("doctor_id")
  operatorId                String?      @map("operator_id")
  dye                       Boolean      @default(false)
  report                    Boolean      @default(false)
  lockedAt                  DateTime?    @map("locked_at") @db.Timestamptz
  voidedAt                  DateTime?    @map("voided_at") @db.Timestamptz
  voidedByUserId            String?      @map("voided_by_user_id")
  voidReason                String?      @map("void_reason")
  priceSnapshotIqd          Int?         @map("price_snapshot_iqd")
  dyeCostSnapshotIqd        Int?         @map("dye_cost_snapshot_iqd")
  reportCostSnapshotIqd     Int?         @map("report_cost_snapshot_iqd")
  doctorCutSnapshotIqd      Int?         @map("doctor_cut_snapshot_iqd")
  operatorCutSnapshotIqd    Int?         @map("operator_cut_snapshot_iqd")
  internalPctSnapshot       Int?         @map("internal_pct_snapshot")
  totalAmountIqdSnapshot    Int?         @map("total_amount_iqd_snapshot")
  createdAt                 DateTime     @map("created_at") @db.Timestamptz
  updatedAt                 DateTime     @map("updated_at") @db.Timestamptz
  deletedAt                 DateTime?    @map("deleted_at") @db.Timestamptz
  version                   Int          @default(0)
  lastSyncedAt              DateTime?    @map("last_synced_at") @db.Timestamptz
  originDeviceId            String?      @map("origin_device_id")
  entityId                  String       @map("entity_id")

  patient        Patient       @relation(fields: [patientId], references: [id])
  receptionist   User          @relation("VisitReceptionist", fields: [receptionistUserId], references: [id])
  voidedBy       User?         @relation("VisitVoider",      fields: [voidedByUserId], references: [id])
  checkType      CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype   CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])
  doctor         Doctor?       @relation(fields: [doctorId], references: [id])
  operator       Operator?     @relation(fields: [operatorId], references: [id])
  inventoryAdjustments InventoryAdjustment[]

  @@index([entityId, checkTypeId, lockedAt])
  @@index([entityId, doctorId, lockedAt])
  @@index([entityId, operatorId, lockedAt])
  @@map("visits")
}

enum VisitStatus {
  draft
  locked
  voided
}
```

**Invariants**

1. Status transitions follow the state machine below; no other transitions allowed.
2. If parent type has subtypes (`check_types.has_subtypes = 1`), `check_subtype_id` must be non-null.
3. If parent type has no subtypes, `check_subtype_id` must be null.
4. `dye = 1` requires `check_types.dye_supported = 1`.
5. `report = 1` requires `check_types.report_supported = 1`.
6. At lock: `operator_id` non-null; all `*_snapshot_iqd` fields non-null; `internal_pct_snapshot` non-null iff `doctor_id IS NULL`; `total_amount_iqd_snapshot = price_snapshot_iqd + dye_cost_snapshot_iqd + report_cost_snapshot_iqd`.
7. While draft, all snapshot fields are null.
8. Voiding requires `voided_by_user_id` to have role `superadmin`.

**State Machine**

```
       create                lock                   void
+-----+   ->    +-------+    ->    +--------+   ->    +--------+
| n/a |         | draft |          | locked |         | voided |
+-----+         +-------+          +--------+         +--------+
                   |                                     ^
                   +-- delete -- (soft) --- (no path back from voided)
```

| From | To | Trigger | Side Effects |
|-|-|-|-|
| n/a | draft | receptionist creates visit on a check workspace | row inserted with `check_type_id` set; audit `create`. |
| draft | draft | receptionist edits subtype/doctor/dye/report | row update; audit `update` with delta. |
| draft | (deleted) | receptionist discards | soft-delete; audit `soft_delete`. |
| draft | locked | receptionist clicks Lock and validation passes | snapshots written; inventory consumed; receipt generated; audit `lock`. |
| locked | voided | superadmin voids | offsetting inventory adjustments; audit `void` with reason. |

**Money Math (applied at lock)**

```
total_amount = price + (dye ? dye_cost : 0) + (report ? report_cost : 0)
where:
  price = doctor_check_pricing.price_override_iqd  if doctor_id is set and override exists
        else check_subtypes.price_iqd              if subtype visit
        else check_types.base_price_iqd            otherwise
  dye_cost    = settings.dye_cost_iqd
  report_cost = settings.report_cost_iqd

doctor_cut_basis = price                     -- excludes dye and report
doctor_cut       = case doctor_id:
                     null:    floor(price * settings.internal_doctor_pct / 100)   -- house
                     non-null:case doctor_check_pricing.cut_kind:
                               'pct':   floor(price * cut_value / 100)
                               'fixed': cut_value

operator_cut     = operators.base_cut_per_check_iqd * (dye ? 2 : 1)
```

`internal_pct_snapshot` is set ONLY when `doctor_id IS NULL` at lock; it captures `settings.internal_doctor_pct` at that moment.

**Sync Policy:** `manual`. Rationale: financial-critical. Two devices editing the same draft, or two devices voiding the same locked visit, are real risks that must surface to a human resolver rather than auto-merging.

#### §6.1.11 settings

A singleton key-value table for global tunables.

**Local Schema (SQLite)**

```sql
CREATE TABLE settings (
  id                TEXT PRIMARY KEY,
  key               TEXT NOT NULL,
  value             TEXT NOT NULL,
  value_type        TEXT NOT NULL CHECK (value_type IN ('int','decimal','text','bool')),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX settings_key ON settings(entity_id, key) WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model Setting {
  id              String        @id
  key             String
  value           String
  valueType       SettingType   @map("value_type")
  createdAt       DateTime      @map("created_at") @db.Timestamptz
  updatedAt       DateTime      @map("updated_at") @db.Timestamptz
  deletedAt       DateTime?     @map("deleted_at") @db.Timestamptz
  version         Int           @default(0)
  lastSyncedAt    DateTime?     @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?       @map("origin_device_id")
  entityId        String        @map("entity_id")

  @@unique([entityId, key])
  @@map("settings")
}

enum SettingType {
  int
  decimal
  text
  bool
}
```

**Required keys at v1**

| Key | Type | Notes |
|-|-|-|
| `dye_cost_iqd` | int | non-negative IQD added to a visit when `dye=1`. |
| `report_cost_iqd` | int | non-negative IQD added to a visit when `report=1`. |
| `internal_doctor_pct` | int | percent in `[0,100]`. Applied to house lines only. |
| `idle_lock_minutes` | int | default 10. |
| `arabic_numerals` | bool | render Eastern-Arabic digits in Arabic locale; default `false`. |
| `clinic_display_name_ar` | text | header on receipts. |
| `clinic_display_name_en` | text | optional. |
| `currency_symbol` | text | default `د.ع`. |

**Invariants**

1. Key uniqueness within tenant among non-deleted rows.
2. Value parses against `value_type`.
3. Soft-deleting a required key is blocked.

**Sync Policy:** `manual`. Rationale: settings flips have business consequences; concurrent edits must be resolved by an admin.

#### §6.1.12 inventory_items

A consumable or supply item.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| name_ar | TEXT | yes | yes |  |
| name_en | TEXT | no | yes |  |
| unit | TEXT | yes | no | free-form: ml, vial, box, pcs, etc. |
| quantity_on_hand | INTEGER | yes | no | derived; recomputed in same tx as adjustments. |
| low_stock_threshold | INTEGER | yes | no | non-negative; UI flags when on-hand <= threshold. |
| is_active | INTEGER | yes | no |  |

**Local Schema (SQLite)**

```sql
CREATE TABLE inventory_items (
  id                    TEXT PRIMARY KEY,
  name_ar               TEXT NOT NULL,
  name_en               TEXT NULL,
  unit                  TEXT NOT NULL,
  quantity_on_hand      INTEGER NOT NULL DEFAULT 0,
  low_stock_threshold   INTEGER NOT NULL DEFAULT 0 CHECK (low_stock_threshold >= 0),
  is_active             INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  deleted_at            TEXT NULL,
  version               INTEGER NOT NULL DEFAULT 0,
  dirty                 INTEGER NOT NULL DEFAULT 1,
  last_synced_at        TEXT NULL,
  origin_device_id      TEXT NULL,
  entity_id             TEXT NOT NULL
);
```

**Server Schema (Prisma)**

```prisma
model InventoryItem {
  id                  String    @id
  nameAr              String    @map("name_ar")
  nameEn              String?   @map("name_en")
  unit                String
  quantityOnHand      Int       @default(0) @map("quantity_on_hand")
  lowStockThreshold   Int       @default(0) @map("low_stock_threshold")
  isActive            Boolean   @default(true) @map("is_active")
  createdAt           DateTime  @map("created_at") @db.Timestamptz
  updatedAt           DateTime  @map("updated_at") @db.Timestamptz
  deletedAt           DateTime? @map("deleted_at") @db.Timestamptz
  version             Int       @default(0)
  lastSyncedAt        DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId      String?   @map("origin_device_id")
  entityId            String    @map("entity_id")

  consumptionMap   InventoryConsumptionMap[]
  adjustments      InventoryAdjustment[]

  @@map("inventory_items")
}
```

**Invariants**

1. `quantity_on_hand` equals the sum of `inventory_adjustments.delta` for non-deleted adjustments of this item. The materialized value is recomputed in the same transaction as any adjustment write.
2. `quantity_on_hand` may go negative; the UI surfaces a warning, but does not block (surgical/dye items can over-consume during emergencies; admin reconciles via `count_correction`).

**Sync Policy:** `last-write-wins` for the item metadata fields. The `quantity_on_hand` is recomputed locally from `inventory_adjustments` on every pull, so its sync value is informational only.

#### §6.1.13 inventory_consumption_map

Maps a check type or specific subtype to the items consumed when a line of that type locks.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| check_type_id | TEXT FK | yes | no |  |
| check_subtype_id | TEXT FK | conditional | no | required when type has subtypes; null otherwise. |
| item_id | TEXT FK | yes | no |  |
| quantity_per_check | INTEGER | yes | no | positive. |
| on_dye_only | INTEGER | yes | no | 1 = consume only when the line has `dye=1`. |

**Local Schema (SQLite)**

```sql
CREATE TABLE inventory_consumption_map (
  id                  TEXT PRIMARY KEY,
  check_type_id       TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id    TEXT NULL REFERENCES check_subtypes(id),
  item_id             TEXT NOT NULL REFERENCES inventory_items(id),
  quantity_per_check  INTEGER NOT NULL CHECK (quantity_per_check > 0),
  on_dye_only         INTEGER NOT NULL DEFAULT 0 CHECK (on_dye_only IN (0,1)),
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  deleted_at          TEXT NULL,
  version             INTEGER NOT NULL DEFAULT 0,
  dirty               INTEGER NOT NULL DEFAULT 1,
  last_synced_at      TEXT NULL,
  origin_device_id    TEXT NULL,
  entity_id           TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_consumption_unique
  ON inventory_consumption_map(check_type_id, IFNULL(check_subtype_id,''), item_id, on_dye_only)
  WHERE deleted_at IS NULL;
```

**Server Schema (Prisma)**

```prisma
model InventoryConsumptionMap {
  id                  String    @id
  checkTypeId         String    @map("check_type_id")
  checkSubtypeId      String?   @map("check_subtype_id")
  itemId              String    @map("item_id")
  quantityPerCheck    Int       @map("quantity_per_check")
  onDyeOnly           Boolean   @default(false) @map("on_dye_only")
  createdAt           DateTime  @map("created_at") @db.Timestamptz
  updatedAt           DateTime  @map("updated_at") @db.Timestamptz
  deletedAt           DateTime? @map("deleted_at") @db.Timestamptz
  version             Int       @default(0)
  lastSyncedAt        DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId      String?   @map("origin_device_id")
  entityId            String    @map("entity_id")

  checkType     CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype  CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])
  item          InventoryItem @relation(fields: [itemId], references: [id])

  @@unique([checkTypeId, checkSubtypeId, itemId, onDyeOnly])
  @@map("inventory_consumption_map")
}
```

**Invariants**

1. If parent type has subtypes, `check_subtype_id` must be non-null.
2. If parent type has no subtypes, `check_subtype_id` must be null.
3. `quantity_per_check` is strictly positive.
4. Uniqueness on `(check_type_id, check_subtype_id, item_id, on_dye_only)` among non-deleted rows.

**Sync Policy:** `last-write-wins`.

#### §6.1.14 inventory_adjustments

Append-only ledger of every change to stock counts.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| item_id | TEXT FK | yes | no |  |
| delta | INTEGER | yes | no | signed; positive on receive/positive correction, negative on consume/writeoff. |
| reason | TEXT | yes | yes | `receive`, `writeoff`, `count_correction`, `consume_visit`. |
| visit_id | TEXT FK | conditional | no | required for `consume_visit`. |
| note | TEXT | no | no |  |
| by_user_id | TEXT FK | yes | no |  |

**Local Schema (SQLite)**

```sql
CREATE TABLE inventory_adjustments (
  id                TEXT PRIMARY KEY,
  item_id           TEXT NOT NULL REFERENCES inventory_items(id),
  delta             INTEGER NOT NULL,
  reason            TEXT NOT NULL CHECK (reason IN ('receive','writeoff','count_correction','consume_visit')),
  visit_id          TEXT NULL REFERENCES visits(id),
  note              TEXT NULL,
  by_user_id        TEXT NOT NULL REFERENCES users(id),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL,
  CHECK (reason != 'consume_visit' OR visit_id IS NOT NULL)
);
CREATE INDEX inventory_adjustments_item  ON inventory_adjustments(item_id, created_at) WHERE deleted_at IS NULL;
CREATE INDEX inventory_adjustments_visit ON inventory_adjustments(visit_id) WHERE visit_id IS NOT NULL;
```

**Server Schema (Prisma)**

```prisma
model InventoryAdjustment {
  id              String              @id
  itemId          String              @map("item_id")
  delta           Int
  reason          AdjustmentReason
  visitId         String?             @map("visit_id")
  note            String?
  byUserId        String              @map("by_user_id")
  createdAt       DateTime            @map("created_at") @db.Timestamptz
  updatedAt       DateTime            @map("updated_at") @db.Timestamptz
  deletedAt       DateTime?           @map("deleted_at") @db.Timestamptz
  version         Int                 @default(0)
  lastSyncedAt    DateTime?           @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?             @map("origin_device_id")
  entityId        String              @map("entity_id")

  item       InventoryItem @relation(fields: [itemId], references: [id])
  visit      Visit?        @relation(fields: [visitId], references: [id])
  byUser     User          @relation(fields: [byUserId], references: [id])

  @@map("inventory_adjustments")
}

enum AdjustmentReason {
  receive
  writeoff
  count_correction
  consume_visit
}
```

**Invariants**

1. Adjustments are never edited or hard-deleted. Voiding a visit writes new offsetting rows referencing the same `visit_id`.
2. `reason = 'consume_visit'` requires `visit_id` non-null and `delta` non-positive.
3. `reason = 'receive'` requires `delta > 0`.
4. `reason = 'writeoff'` requires `delta < 0`.
5. `reason = 'count_correction'` allows any non-zero delta.

**Sync Policy:** `additive-only`.

#### §6.1.15 audit_log

Universal append-only log of every business mutation.

**Core Fields**

| Field | Type | Required | Searchable | Notes |
|-|-|-|-|-|
| actor_user_id | TEXT FK | yes | yes |  |
| action | TEXT | yes | yes | `create`, `update`, `soft_delete`, `lock`, `void`, `clock_in`, `clock_out`, `password_change`. |
| entity | TEXT | yes | yes | table name, e.g., `visits`, `doctors`. |
| entity_id | TEXT | yes | yes |  |
| delta | TEXT | yes | no | JSON `{ field: { from, to } }`; for `create`, all `from` are null; for `soft_delete`, single `deleted_at` delta. |
| ip | TEXT | no | yes | client IP if known (server tags it on push). |
| device_id | TEXT | yes | yes |  |
| at | TEXT | yes | yes | RFC3339 UTC. |

**Local Schema (SQLite)**

```sql
CREATE TABLE audit_log (
  id                TEXT PRIMARY KEY,
  actor_user_id     TEXT NOT NULL REFERENCES users(id),
  action            TEXT NOT NULL,
  entity            TEXT NOT NULL,
  entity_id         TEXT NOT NULL,
  delta             TEXT NOT NULL,
  ip                TEXT NULL,
  device_id         TEXT NOT NULL,
  at                TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id_tenant  TEXT NOT NULL
);
CREATE INDEX audit_log_entity ON audit_log(entity, entity_id, at);
CREATE INDEX audit_log_actor  ON audit_log(actor_user_id, at);
CREATE INDEX audit_log_at     ON audit_log(at);
```

Note the column collision avoidance: the tenant scope column on `audit_log` is named `entity_id_tenant` because `entity_id` is already used to mean "id of the audited business row".

**Server Schema (Prisma)**

```prisma
model AuditLog {
  id              String    @id
  actorUserId     String    @map("actor_user_id")
  action          String
  entity          String
  entityId        String    @map("entity_id")
  delta           Json
  ip              String?
  deviceId        String    @map("device_id")
  at              DateTime  @db.Timestamptz
  createdAt       DateTime  @map("created_at") @db.Timestamptz
  updatedAt       DateTime  @map("updated_at") @db.Timestamptz
  deletedAt       DateTime? @map("deleted_at") @db.Timestamptz
  version         Int       @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId  String?   @map("origin_device_id")
  entityIdTenant  String    @map("entity_id_tenant")

  actor User @relation(fields: [actorUserId], references: [id])

  @@index([entity, entityId, at])
  @@index([actorUserId, at])
  @@index([at])
  @@map("audit_log")
}
```

**Invariants**

1. Append-only. The system never updates an audit row except for sync metadata.
2. Local rows are pruned to the most recent 90 days; the server keeps them indefinitely. Pruning is a vacuum job that runs daily and never deletes a row whose `dirty = 1`.
3. Every business write transaction includes one audit row in the same SQLite transaction.

**Sync Policy:** `additive-only`.

### §6.2 Cross-App / Cross-Surface References

| Entity | Owner | Consumer | Contract |
|-|-|-|-|
| `users.id` | this PRD | sync server, audit log, future apps | actor identifier for audit and authorization across all surfaces. |
| `entity_id` (tenant) | platform (sync server) | every syncable table | server JWT carries the active tenant; client trusts the JWT after offline-cache validation. |
| `device_id` | Tauri runtime | every audit log row, every sync HTTP header | provided by `tauri-plugin-os` at boot, stored in `tauri-plugin-store`. |

### §6.3 Entity Relationship Map

The full graph does not fit in 80 columns; split into four sub-diagrams.

**Reference Data**

```
users -< user_role enum
check_types -< check_subtypes
check_types -< doctor_check_pricing >- doctors
check_subtypes -< doctor_check_pricing
check_types -< inventory_consumption_map >- inventory_items
check_subtypes -< inventory_consumption_map
check_types -< operator_specialties >- operators
settings (singleton kv)
```

**Visit Graph**

```
patients -< visits >- users (receptionist)
visits >- check_types
visits >- check_subtypes (nullable; required when type has subtypes)
visits >- doctors        (nullable; null = house)
visits >- operators      (required at lock)
visits -- voided_by --- users
```

**Operator Graph**

```
operators -< operator_shifts >- users (check_in_by, check_out_by)
operators -< operator_specialties >- check_types
operators -< visits (operator_id)
```

**Inventory Graph**

```
inventory_items -< inventory_adjustments >- users (by_user_id)
inventory_adjustments -- (consume) --- visits
inventory_items -< inventory_consumption_map >- check_types
                                              \- check_subtypes
```

**Audit (cross-cuts everything)**

```
audit_log >- users (actor)
audit_log >- (any entity, entity_id) -- denormalized; no FK, query by string match
```

---

## §7 Module Specifications

### §7.1 Reception

**Purpose:** front-desk operators capture patient visits, manage operator shifts, and lock visits with a printed receipt. The Reception module is **per-check**: the receptionist first picks which check the patient is here for, then works inside that check's workspace. A visit is exactly one check.

#### §7.1.1 Checks Grid (`/reception`)

The Reception landing page. A grid of cards, one per active `check_types` row, each showing the check name (`name_ar` primary, `(name_en)` if locale is `en`), the count of today's locked visits for that check, and a sample subtype list when applicable. Clicking a card enters that check's workspace at `/reception/checks/:check-slug`.

ASCII layout:

```
+--------------------------------------------------------------------------+
| Reception                                              [Operator shifts] |
|                                                                          |
| What is the patient here for?                                            |
| +-------------+  +-------------+  +-------------+  +-------------+       |
| | سونار       |  | مفراس       |  | رنين        |  | صدى القلب   |  ... |
| | 12 today    |  |  8 today    |  |  3 today    |  |  2 today    |       |
| +-------------+  +-------------+  +-------------+  +-------------+       |
+--------------------------------------------------------------------------+
```

#### §7.1.2 Check Workspace (`/reception/checks/:check-slug`)

Per-check workspace. Header shows the active check name and a back link to the grid. Body shows today's visits filtered to this check, plus a "+ New visit" button. Filters apply within the workspace (subtype, doctor, status, date).

Columns: `#`, `Created at`, `Patient`, `Subtype`, `Doctor`, `Operator`, `Dye`, `Report`, `Total IQD`, `Status pill`, `Pending sync indicator`, `Actions`.

Sort: `created_at DESC` default.

Pagination: 50 rows per page; cursor-based scroll on local SQLite.

Bulk actions: none in v1.

#### §7.1.3 New Visit (`/reception/checks/:check-slug/new`)

Single-check form. The check is locked into the URL — the receptionist cannot change the check from inside this form. To switch checks they back out to the grid.

ASCII layout:

```
+--------------------------------------------------------------------------+
| New visit  ·  Check: سونار                                  [< Workspace]|
|                                                                          |
| Patient (اسم رباعي):  [_______________________________________________]  |
|                                                                          |
| Subtype:                                                                 |
|   ( ) بدون صبغة     20,000 IQD                                          |
|   ( ) مع صبغة       30,000 IQD                                          |
|                                                                          |
| Doctor:  [search... or leave empty for house]                            |
|                                                                          |
| Dye    : [ ] supported                                                   |
| Report : [ ] supported (+10,000 IQD)                                     |
|                                                                          |
| Operator (at lock):  picked from clocked-in operators with this check    |
|                                                                          |
| Summary                                                                  |
|   Price                                                  20,000 IQD      |
|   Dye                                                         0 IQD      |
|   Report                                                 10,000 IQD      |
|   Total                                                  30,000 IQD      |
|   Doctor cut (preview)                                    6,000 IQD      |
|   Operator cut (preview)                                  3,000 IQD      |
|                                                                          |
|                  [ Save draft ]   [ Discard ]   [ Lock & print >> ]      |
+--------------------------------------------------------------------------+
```

**Form fields**

| Field | Validation | Notes |
|-|-|-|
| Patient name | Zod: `z.string().trim().min(2).max(120)`. | Single-line; FTS5 search backs autocompletion of recent patients (last 30 days). |
| Check type | locked to the workspace's `check_type_id`. | Not editable in the form. |
| Subtype | required if `check_types.has_subtypes = 1`; otherwise hidden. | Radio cards listing non-deleted subtypes with prices. |
| Doctor | optional autocomplete; null = house. | Live FTS over `doctors_fts`; empty box = house. |
| Dye | checkbox; gated by `check_types.dye_supported`. | Disabled with tooltip if not supported. |
| Report | checkbox; gated by `check_types.report_supported`. | Disabled with tooltip if not supported. |
| Operator picker | shown at lock time. | Loaded from currently-clocked-in operators with `operator_specialties` covering the workspace's check type. |

**Actions**

| Action | Trigger | Permission | Side Effects | Audit Event |
|-|-|-|-|-|
| Create visit | "Save draft" or implicit save on first field commit | receptionist, superadmin | inserts `visits` (status=draft, `check_type_id` set). | `create`. |
| Edit visit | inline / form save | same (only on draft) | updates `visits`. | `update` with delta. |
| Discard visit | "Discard" | same | soft-deletes `visits`. | `soft_delete`. |
| Lock visit | "Lock & print" | same | snapshots written to `visits`; inventory consumption; receipt write. See §8.1. | `lock` with snapshot delta. |

**States**

- **Empty:** "Fill the patient name to start a visit" placeholder.
- **Loading:** skeleton rows in the form when fetching reference data.
- **Error:** inline form error toast for validation; modal for unrecoverable backend errors with retry.
- **Lock validation failure:** inline list of unmet requirements (no operator on shift for this check; subtype missing).

**Mobile/Compact:** v1 deferred. App is desktop-only.

#### §7.1.4 Visit Detail (`/reception/visits/:id`)

Reachable from any workspace row click. Tabs: `Details`, `Audit`, `Receipts`.

- `Details` tab: read-only after lock; one consolidated panel with check, subtype, doctor, operator, dye/report flags, all snapshots, and computed cuts.
- `Audit` tab: filtered `audit_log` on this `visit_id`.
- `Receipts` tab: list of generated receipt PDFs/thermal txts; reprint button.

Superadmin-only action: `Void` button (requires void reason text input).

#### §7.1.5 Operator Shifts (`/reception/shifts`)

ASCII:

```
+--------------------------------------------------------------------------+
| On shift now:                                       [+ Clock in operator]|
| +-------------------------------------------------------+                |
| | Operator     | Specialties      | Since   | Action    |                |
| |--------------|------------------|---------|-----------|                |
| | علي حسن      | سونار, مفراس     | 08:14   | Clock out |                |
| | محمد سعيد    | سونار            | 09:02   | Clock out |                |
| +-------------------------------------------------------+                |
|                                                                          |
| Today's history:                                                         |
| +-------------------------------------------------------+                |
| | Operator | In       | Out      | Duration | Lines run |                |
| ...                                                                      |
+--------------------------------------------------------------------------+
```

Actions:

| Action | Trigger | Permission | Side Effects | Audit Event |
|-|-|-|-|-|
| Clock in | + button | receptionist, superadmin | inserts `operator_shifts` with `check_in_by_user_id`. | `clock_in`. |
| Clock out | row button | same | sets `check_out_at`, `check_out_by_user_id`. | `clock_out`. |
| Edit shift retroactively | inline | superadmin only | updates check_in/out fields. | `update` with delta. |

States: empty (no operator on shift), error (failed to write — local DB unavailable), loading (fetch).

### §7.2 Accounting

**Purpose:** read-only financial reporting with deep filters and drill-down to source visits.

#### §7.2.1 Dashboard (`/accounting`)

KPIs across the active filter window (default: today):
- Revenue (sum of locked visit totals, IQD)
- Doctor cuts (sum)
- Operator cuts (sum)
- Inventory consumption value (sum of `quantity * latest_unit_cost` if cost is tracked; v1 displays `quantity` only)
- Net (revenue - doctor cuts - operator cuts)

Trend cards: today vs yesterday, this week vs last, this month vs last.

Active filters bar:
- Date range picker (today, yesterday, last 7d, this month, last month, custom).
- Status: locked-only is default; toggle to include voided.

#### §7.2.2 Visits Report (`/accounting/visits`)

Detailed table of every locked visit in the date range. Columns:

`Date | Visit # | Patient | Check | Subtype | Doctor | Operator | Dye | Report | Price | Doctor cut | Operator cut | Net`

Filters (all combinable):
- Date range (required).
- Status: `locked`, `voided`, or both.
- Check type (multi-select).
- Subtype (multi-select; filtered by selected check type).
- Doctor (multi-select, includes a "(house)" pseudo-option).
- Operator (multi-select).
- Dye `y/n/all`.
- Report `y/n/all`.

Aggregation footer: total revenue, total doctor cuts, total operator cuts, net.

Export: CSV button generates a UTF-8 BOM CSV file via `tauri-plugin-dialog` save-as.

Drill-down: clicking a row opens `/reception/visits/:id` (read-only for accountant; superadmin sees void button).

#### §7.2.3 Doctor Earnings (`/accounting/doctors`)

Aggregate per doctor across the filter window.

Columns: `Doctor | Specialty | Visits | Revenue | Doctor cut total | Avg cut per visit`.

Includes a row for `(house)` summing all internal-doctor visits.

Drill-down: click a doctor → `/accounting/doctors/:id` shows per-check breakdown and a list of source visits.

#### §7.2.4 Operator Earnings (`/accounting/operators`)

Aggregate per operator.

Columns: `Operator | Visits | Visits with dye | Operator cut total | Hours on shift | Avg cut per hour`.

Drill-down: click an operator → `/accounting/operators/:id` shows shifts in the window plus the visits attributed.

#### §7.2.5 Daily Close (`/accounting/daily-close`)

End-of-day reconciliation.

Layout:

```
Date: [2026-05-07]                                       [ Run close ]

Today                              vs Prior Day
  Revenue:        1,250,000 IQD     1,180,000   (+5.9%)
  Doctor cuts:      375,000 IQD       340,000
  Operator cuts:    150,000 IQD       145,000
  Net:              725,000 IQD       695,000

  Locked visits:        18              17
  Voided today:          1               0
  Voided from prior:     0               0

Pending sync:    0 ops
Reconciliation:  matches             [ Sign and freeze ]
```

When the accountant clicks "Sign and freeze", a `daily_close` row is materialized (Horizon-1 entity; v1 uses an in-memory generated artifact). For v1 this artifact is generated on demand from `visits` and is exportable as PDF.

### §7.3 Inventory

**Purpose:** stock visibility and manual adjustments. Auto-decrement happens via the visit-lock workflow.

#### §7.3.1 Items List (`/inventory`)

Columns: `Name | Unit | On hand | Threshold | Status pill (OK | LOW | NEG) | Last adjusted`.

Filters: search, status (OK / Low / Negative), active/inactive.

Drill-down: row click → `/inventory/items/:id`.

#### §7.3.2 Item Detail (`/inventory/items/:id`)

Tabs: `Overview`, `Consumption Map`, `Adjustments`, `Audit`.

- **Overview:** current on-hand, threshold, badge.
- **Consumption Map:** read-only table of `inventory_consumption_map` entries. Edit redirects to admin.
- **Adjustments:** chronological list of `inventory_adjustments` rows; voided visit consumptions render as positive offsetting rows.
- **Audit:** filtered `audit_log`.

#### §7.3.3 Adjust (`/inventory/adjust`)

Form: pick item, choose reason (`receive` / `writeoff` / `count_correction`), enter delta (positive for receive, negative for writeoff, signed for correction), optional note. Submit writes one `inventory_adjustments` row and recomputes `quantity_on_hand` in the same transaction.

Receptionist permission allows `receive` and `writeoff`; only superadmin may write `count_correction`.

### §7.4 Admin

**Purpose:** superadmin CRUD over all reference data and settings.

Sub-pages, each list+detail:
- §7.4.1 **Users** — fields: email, name, role, active, password reset (issues a one-time admin-set password). Audit on every change.
- §7.4.2 **Check Types** — list, detail with subtypes table inline. Toggling `has_subtypes` follows the invariants in §6.1.2.
- §7.4.3 **Doctors** — list with FTS5 search, detail with `doctor_check_pricing` rows; add/edit pricing per (check type, subtype if applicable).
- §7.4.4 **Operators** — list, detail with `operator_specialties` and `base_cut_per_check_iqd`. Soft-delete blocked if open shifts exist.
- §7.4.5 **Inventory (Catalog)** — items + consumption map editing. Items list shares with §7.3.1 but admin sees additional columns (active flag, last edit author).
- §7.4.6 **Settings** — keyed form for the v1 required keys listed in §6.1.11.
- §7.4.7 **Audit** — global audit search; see §7.5.

Common patterns across admin pages:

| Action | Trigger | Permission | Side Effects | Audit Event |
|-|-|-|-|-|
| Create | "+ Add" | superadmin | row insert; cascade dependent rows where applicable. | `create`. |
| Update | inline edit / save | superadmin | row update. | `update` with delta. |
| Soft-delete | "Delete" | superadmin | sets `deleted_at`. | `soft_delete`. |
| Reset password | row action on Users | superadmin | sets a new `password_hash` and forces sign-out across devices. | `password_change`. |

States: empty (per page), loading (skeleton), error (toast + retry). RTL-aware confirmations on every destructive action.

### §7.5 Audit (`/audit`)

ASCII:

```
+----------------------------------------------------------------------+
| Filters: [Actor v]  [Action v]  [Entity v]  [From..To]  [Search]    |
| +------------------------------------------------------------------+ |
| | At                  | Actor   | Action | Entity     | Entity ID | |
| |---------------------|---------|--------|------------|-----------| |
| | 2026-05-07 14:02:11 | Maha    | lock   | visits     | 0192f...  | |
| | 2026-05-07 14:02:11 | Maha    | update | inventory_adjustments | 0192f...  | |
| | 2026-05-07 13:58:02 | Sami    | update | settings   | 0192e...  | |
| +------------------------------------------------------------------+ |
| Clicking a row expands the JSON delta inline.                        |
+----------------------------------------------------------------------+
```

Filters: actor, action enum, entity table, entity_id substring search, date range, free-text search across `delta`.

Server-backed when the query exceeds local retention (90 days); local-backed otherwise. UI surfaces "querying server" pill when crossing the boundary.

---

## §8 Cross-Module Business Logic

### §8.1 Lock a Visit

| Property | Description |
|-|-|
| Trigger | Receptionist clicks "Lock & print" on `/reception/checks/:check-slug/new` (or on a draft visit detail page). |
| Surfaces involved | Reception (UI), Tauri (`visits::lock_visit` command), domain service (`VisitService::lock`), inventory service (consumption), audit service. |
| Frequency | Per visit, typically 30-100 times per day. |

**Step Sequence**

1. Validate the draft: `check_type_id` set; `check_subtype_id` set iff `check_types.has_subtypes = 1`; `dye` and `report` consistent with type capabilities; patient name non-empty.
2. Build the operator-eligibility set: `qualified_operators(visit) = active_shifts ∩ operators_with_specialty(visit.check_type_id)`. If empty, return `LockError::NoQualifiedOperator(visit_id)` and surface in UI.
3. UI prompts the receptionist to pick an operator from the eligibility set.
4. Receptionist confirms; client posts `visits::lock_visit { visit_id, operator_id }` IPC.
5. Rust handler opens a SQLite transaction.
6. Resolve `price` via the §6.1.10 money math; compute `doctor_cut`, `operator_cut`, `internal_pct_snapshot` as applicable; write all `*_snapshot_iqd` columns onto the visit; set `operator_id`; set `total_amount_iqd_snapshot = price + dye_cost + report_cost`; set `status='locked'`, `locked_at = now`.
7. Iterate `inventory_consumption_map` matching `(check_type_id, check_subtype_id?)` filtered by `on_dye_only ⇒ visit.dye = 1`; write one `inventory_adjustments` row per match with negative delta and `reason='consume_visit'`, `visit_id` set.
8. Recompute `inventory_items.quantity_on_hand` for each affected item (sum of `inventory_adjustments.delta`).
9. Write one `audit_log` row per change in this transaction (visit lock with snapshot delta, each inventory adjustment, each item recomputation).
10. Generate the receipt PDF and thermal text; persist to `$APPDATA/idc-system/receipts/...`.
11. Commit the transaction. Enqueue outbox entries for each affected row.
12. UI fires the print dialog (PDF) and prints the thermal text via the configured printer.

**Business Rules**

- All steps 5-11 run inside a single SQLite transaction. No partial lock state is persistable.
- `now` for timestamps comes from the local clock; the server stamp on push is informational.
- Receipt generation is part of the transaction in the sense that a failure to render the receipt aborts the lock (the receptionist sees a "lock failed — receipt generator unavailable" error and the visit remains a draft).

**Offline Branch**

| Step | Online | Offline |
|-|-|-|
| 1-9 | Same | Same. All local; no network calls. |
| 10 | Same | Same. Receipt is local. |
| 11 | Outbox enqueues; sync engine ships within seconds. | Outbox enqueues; sync engine waits for connectivity. UI shows the visit row with a "pending sync" pill. |
| 12 | Same | Same. Print is local. |

**UI Signals**

- "Pending sync" pill on the visit row in the list.
- Sync status pill in the app shell turns yellow (`pushing`) when the engine drains the outbox.
- An `audit_log` row is also queued and ships with the visit.

### §8.2 Void a Visit

| Property | Description |
|-|-|
| Trigger | Superadmin opens `/reception/visits/:id` or `/accounting/visits/:id` and clicks "Void". |
| Surfaces involved | Reception or Accounting (UI), Tauri (`visits::void_visit` command), domain service, inventory service, audit. |
| Frequency | Rare; <1% of visits. |

**Step Sequence**

1. Confirm-modal asks for `void_reason` (required, ≥5 chars).
2. Client posts `visits::void_visit { visit_id, reason }` IPC.
3. Rust handler opens a transaction.
4. Verify status is `locked`. If not, return `VoidError::NotLocked`.
5. Verify caller's role is `superadmin`. If not, return `VoidError::Forbidden`.
6. Set `status='voided'`, `voided_at = now`, `voided_by_user_id`, `void_reason`.
7. Read all `consume_visit` adjustments for this `visit_id`; write offsetting positive-delta `inventory_adjustments` rows referencing the same `visit_id` with reason `consume_visit` (the system distinguishes consumption vs reversal by sign, not by reason).
8. Recompute `inventory_items.quantity_on_hand` for affected items.
9. Write one `audit_log` row per touched row (`void` on visit; `create` on each offsetting adjustment; `update` on each affected inventory item).
10. Commit. Enqueue outbox.

**Business Rules**

- Void is one-way; voided visits do not return to `locked`. A new corrective visit must be created if needed.
- Voiding a visit does NOT delete the original `inventory_adjustments` rows; the offsetting positive rows are appended.

**Offline Branch**

Same model as lock — fully local, ships when connectivity returns.

### §8.3 Operator Clock In / Out

| Property | Description |
|-|-|
| Trigger | Receptionist or superadmin uses `/reception/shifts`. |
| Surfaces involved | Reception (UI), Tauri (`shifts::clock_in`, `shifts::clock_out`). |
| Frequency | Per operator, typically 1-3 times per day. |

**Step Sequence (Clock In)**

1. Receptionist picks operator.
2. Service rejects if any open shift exists for the operator (partial unique index).
3. Insert `operator_shifts` row with `check_in_at = now` and `check_in_by_user_id = current_user`.
4. Audit `clock_in`.

**Step Sequence (Clock Out)**

1. Receptionist clicks "Clock out" on a row.
2. Service rejects if `check_out_at` is already set.
3. Update the row with `check_out_at = now` and `check_out_by_user_id`.
4. Audit `clock_out` with delta on `check_out_at`.

**Business Rules**

- Idempotent within a millisecond: double-clicks do not create duplicates because the second insert hits the partial-unique-index conflict.
- Retroactive edits: superadmin only. Audit captures old vs new times.

**Offline Branch**

Same model — fully local, ships later.

### §8.4 Daily Close

| Property | Description |
|-|-|
| Trigger | Accountant clicks "Run close" on `/accounting/daily-close`. |
| Surfaces involved | Accounting (UI), Tauri reports service. |
| Frequency | Once per day at end-of-shift. |

**Step Sequence**

1. Aggregate today's locked visits and their cuts directly from `visits` snapshot columns.
2. Aggregate today's voided visits separately.
3. Aggregate inventory consumption today (sum of `consume_visit` adjustments today).
4. Compute deltas vs prior day.
5. Render a printable artifact via the receipt generator.
6. (v1 stops here.) Horizon-1 will add a `daily_close` row signed by the server.

**Offline Branch**

- The aggregation is local SQL; works fully offline.
- The "matches" reconciliation flag is only meaningful when the day's outbox is empty, i.e., no pending sync. UI surfaces this clearly: "0 pending ops" badge alongside the close button.

### §8.5 Pricing Change Propagation

| Property | Description |
|-|-|
| Trigger | Superadmin edits a check type, subtype, or doctor pricing row. |
| Surfaces involved | Admin (UI), domain service. |
| Frequency | Rare; ad-hoc. |

**Step Sequence**

1. Admin saves edit; service writes the change with audit.
2. New visits booked after this point use the new price.
3. Existing `draft` visits do NOT auto-recompute; the receptionist sees a "prices updated — refresh totals?" banner and can refresh on demand. (No silent rewrite.)
4. Existing `locked` visits NEVER change. Their snapshot fields remain authoritative.

**Business Rules**

- Snapshots are never overwritten by background processes.
- A "Recalculate draft" button on the new-visit form re-fetches reference data and recomputes the running total.

**Offline Branch**

- Edits are local; ride the outbox. Other devices pull the change at next sync. Drafts on those devices show the same banner.

### §8.6 Settings Change

| Property | Description |
|-|-|
| Trigger | Superadmin edits a `settings` key. |
| Surfaces involved | Admin → all locks afterward. |
| Frequency | Very rare. |

Same model as §8.5: locked snapshots untouched, drafts get a "settings changed — recompute?" banner. Conflict policy is `manual`: if two admins edit the same key concurrently, the later push triggers a 409 and the resolver screen.

---

## §9 Multi-User & Multi-Tenant Support

### §9.1 Tenant Scoping

Every syncable table carries an `entity_id` (tenant) column. v1 hardcodes a single tenant per deployment but the column and indexes exist to support multi-branch later. The sync server's `TENANT_MODELS` list (per `.claude/rules/sync-server.md`) includes every entity in §6.1: `users`, `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `operator_shifts`, `patients`, `visits`, `settings`, `inventory_items`, `inventory_consumption_map`, `inventory_adjustments`, `audit_log`. Server middleware injects `request.tenantId` from the JWT and every Prisma query filters on it.

### §9.2 Multi-User Local Behavior

A single SQLite file per device. Multiple users share the file with logical scoping by `actor_user_id` on every audited write. Login swap during a working day is supported: the prior user's session ends, the new user authenticates, the in-memory `UserContext` rotates. No per-user DB, no SQLite attach gymnastics. Data integrity is enforced by referential constraints (every audit row carries the actor) and by the audit-first pattern (no untracked writes).

### §9.3 Cross-Device Behavior

Same user on two devices, or different users in the same tenant on two devices:
- Visits, doctors, operators, settings, inventory, audit log: synced live.
- UI prefs (last-opened tab, theme, language toggle): per-device, stored in `tauri-plugin-store`. NOT synced.
- Drafts: synced, so the accountant can see the draft pipeline from the reception PC. Edits to the same draft from two devices are real and surface as a `manual` conflict.
- Open shifts: synced. If an operator is clocked in on the reception PC and the superadmin tries to clock them out from the admin PC, the second device sees the same row.

---

## §10 System Features

### §10.1 Search

Local SQLite FTS5 indexes:

- `patients_fts (name)` — backs the New Visit patient autocomplete.
- `doctors_fts (name, specialty)` — backs the New Visit doctor autocomplete and `/admin/doctors` list filter.

Inventory items, check types, and subtypes use `LIKE`-prefix queries on the `name_ar`/`name_en` columns; their cardinality is small enough that FTS is overkill in v1.

`audit_log` queries are structured (filter chips), not FTS — full-text on `delta` JSON is a Horizon-1 enhancement.

### §10.2 Export / Import

- CSV export from `/accounting/visits`, `/accounting/doctors`, `/accounting/operators`. UTF-8 BOM. Date format `YYYY-MM-DD HH:MM:SS`. Currency rendered as raw integer IQD (column header notes the unit).
- PDF export of the daily-close artifact via `tauri-plugin-dialog` save-as.
- No bulk import in v1.

### §10.3 Printing

- Receipts: A5 PDF and a 58/80mm thermal text alternative. Templates rendered in code (HTML+CSS for PDF, fixed-width text for thermal).
- Daily-close: PDF only.
- Templates support both Arabic and English; layout mirrors for RTL.

### §10.4 Audit

- Every business write emits one `audit_log` row in the same SQLite transaction (see §4.3).
- Delta format: `{ "field_name": { "from": <old>, "to": <new> } }`. Inserts have all `from = null`. Soft-deletes have a single `deleted_at` delta.
- Local retention: 90 days. Daily vacuum job soft-deletes rows older than 90 days that are also `dirty = 0`.
- Server retention: indefinite in v1.
- Server-backed query when the local retention window is insufficient.

### §10.5 Multi-Currency

Not supported in v1. All money fields are IQD whole units (integer). The `settings.currency_symbol` key (default `د.ع`) controls display only; no conversion logic.

### §10.6 Localization

Localization is load-bearing for IDC and is enumerated here exhaustively.

- **Default locale `ar`** on first install and on cleared storage. RTL applied via `<html dir="rtl">` and Tailwind v4 logical-property utilities (`ps-*` / `pe-*` / `ms-*` / `me-*`). The first-launch detector in `src/i18n/index.ts` ignores OS locale on first run and forces `ar`; subsequent launches respect the user's stored choice.
- **English (`en`)** is the only other locale. Toggle in the app shell persists in `tauri-plugin-store` per device. NOT synced.
- **Every UI string is keyed.** No literal Arabic or English text in JSX/TSX outside `src/i18n/locales/`. Lint rule (to be added in a future phase) enforces this.
- **Translation bundles** at `src/i18n/locales/ar/translation.json` and `src/i18n/locales/en/translation.json`. Namespace layout:
  - `common` — buttons, labels, statuses shared everywhere.
  - `auth` — login screen, no-access, lock screen.
  - `reception` — Reception module strings.
  - `accounting` — Accounting module strings.
  - `inventory` — Inventory module strings.
  - `admin` — Admin module strings.
  - `audit` — Audit page strings.
  - `errors` — domain error messages, validation errors.
  - `receipts` — printed-artifact strings.
- **Domain data is bilingual where it makes sense.** `check_types`, `check_subtypes`, `inventory_items` carry `name_ar` (NOT NULL) and `name_en` (NULL allowed). Display resolution: active locale `en` and `name_en` non-null → use `name_en`; else use `name_ar`. People-names (doctors, patients, operators) are user-entered free text — single `name` column, no translation.
- **Date formats:** tables and machine fields use `YYYY-MM-DD` and `YYYY-MM-DD HH:MM:SS`; prose uses locale-formatted dates via `Intl.DateTimeFormat(i18n.language)`.
- **Number formats:** money rendered with thousands separators per locale. `arabic_numerals` setting toggles Eastern-Arabic digits (`٠١٢٣٤٥٦٧٨٩`) in the Arabic locale; default is `false` because Iraqi invoicing convention uses Western digits.
- **Currency display:** integer IQD with the configurable `settings.currency_symbol` suffix (default `د.ع`).
- **Receipts and printed reports** render in the active locale, with mirrored layout for RTL: header right-aligned, totals on the left, columns flipped right-to-left.
- **RTL correctness checklist** applied to every component:
  - `ps-*` / `pe-*` instead of `pl-*` / `pr-*`.
  - `ms-*` / `me-*` instead of `ml-*` / `mr-*`.
  - Chevrons and arrows mirror via Tailwind `rtl:rotate-180` or inline `dir`-aware variants.
  - Table column order flips automatically through CSS logical properties; explicit `text-end`/`text-start` instead of `text-right`/`text-left`.
  - Iconography reviewed: pencils, arrows, trends, sliders all mirror in RTL.

### §10.7 Accessibility (WCAG 2.1 AA)

- Keyboard navigation across every form; visible focus rings via Tailwind `focus-visible` ring utilities.
- Screen-reader labels on all icon-only buttons (`aria-label` keys live in `i18n/common`).
- Color contrast: receipt and print artifacts use #000 on #fff; UI uses Tailwind `text-foreground` / `bg-background` palette which meets AA at v1 defaults.
- No reliance on color alone for status — every status pill has both color and text.

### §10.8 Offline UX (required)

- **Sync status pill** in the app shell, four states:
  - `idle` — gray, no spinner. All synced.
  - `pushing` — yellow, spinner. Outbox draining.
  - `pulling` — blue, spinner. Pulling peer changes.
  - `offline` — gray with strikethrough cloud icon. No connectivity.
  - `error` — red. Click opens the resolver screen.
- **Pending-sync count badge** on the pill: number of unshipped outbox ops.
- **Per-row pending indicator** on visit lists, audit list, inventory list. Small dot with tooltip "Pending sync" on rows where `dirty = 1`.
- **Conflict resolver screen** (`/sync/conflicts`) for `manual`-policy entities. Lists conflicts; each conflict shows local vs server side-by-side; user picks one or merges manually. Submit calls `/sync/conflicts/:opId/resolve`.
- **Offline-only rendering** on every screen during outage: status pill goes `offline`, "Last synced" timestamp visible in the user menu.
- **No phantom error toasts.** Network failures are absorbed by the sync engine; the user sees the pill change, not an error.

---

## §11 Future Enhancements

| Horizon | Definition |
|-|-|
| Horizon 1 | Next minor version. Well-defined; likely 1-2 milestones out. |
| Horizon 2 | Next major version. Requires significant planning. |
| Horizon 3 | Long-term vision. Aspirational. |

### §11.1 Horizon 1 (v0.2)

- **Patient identity dedupe.** Match by name + phone (when phone is added); merge tooling.
- **Appointment scheduling.** Calendar UI; pre-booked visits start in `draft` and become `locked` on arrival.
- **SMS / WhatsApp reminders.** Patient phone capture; reminder job on the sync server.
- **Refund records as a separate ledger.** Splits void semantics: void = mistake (full reversal), refund = partial cash-back (new ledger row).
- **Server-side audit full-text search** over the JSON delta.
- **Daily-close as a signed `daily_close` entity.** Server-signed; cannot be regenerated from data, only re-verified.
- **Inventory unit cost tracking** for COGS reporting.
- **Bulk import wizards** for catalog migration from spreadsheets.

### §11.2 Horizon 2 (v1.0)

- **PACS / DICOM viewer integration** for clinical staff.
- **Insurance claim attachments** on visits.
- **Multi-branch / true multi-tenant.** The `entity_id` infrastructure already exists; this Horizon turns it on.
- **Operator login** with self-clock-in (replaces receptionist-managed shifts).
- **Mobile companion app** for clinicians on rounds.

### §11.3 Horizon 3

- Clinical reporting (radiologist write-up integrated with the visit).
- AI-assisted measurement on imaging.
- Patient portal with check history and bookings.
- Multi-currency support if cross-border operations emerge.

### §11.4 Considered & Rejected

- **Per-internal-doctor named tracking.** Rejected: the user's stated workflow leaves the doctor field empty for in-house cases. Adding a "house doctor" picker would complicate UX without producing reportable per-person value when every internal doctor gets the same percentage.
- **Per-(operator, check_type) base rates.** Rejected: user explicitly chose flat per-operator rates. Re-evaluation possible at Horizon 2 if payroll fairness becomes an issue.
- **Auto-printed receipts on lock.** Rejected: user accepted "Yes, print at lock" but the implementation uses the standard print dialog (manual confirmation) to avoid jammed-printer-driven lock failures.
- **Last-write-wins on visits.** Rejected: financial records cannot silently merge; concurrent edits must surface to a human.
- **Hard-deleting any business row.** Rejected: every delete is a tombstone (soft-delete) for audit and reversibility.
- **Server-generated entity IDs.** Rejected: breaks offline-first; client-generated UUID v7 is canonical.
- **Operator role with logins.** Rejected for v1: matches user spec; revisit at Horizon 2.

---

## §12 Glossary

| Term | Definition |
|-|-|
| Audit Log | The append-only `audit_log` table capturing every business mutation with actor, action, entity, delta, IP, device, timestamp. See §6.1.15. |
| Check | A diagnostic procedure performed on a patient. Realized as a `visits` row referencing a `check_types` (and optionally a `check_subtypes`) row. |
| CheckSubType | A finer-grained variant of a `check_types` row. Carries its own price. Required when the parent type's `has_subtypes = 1`. See §6.1.3. |
| CheckType | A category of check (سونار, مفراس, ...). Either has a flat price or has subtypes that carry the price. See §6.1.2. |
| Cut | The portion of a visit's revenue allocated to a doctor or operator. Computed at lock and stored as a snapshot on the `visits` row. See §6.1.10 money math. |
| Daily Close | An end-of-day reconciliation report (v1 ad-hoc; Horizon-1 signed entity). See §7.2.5. |
| Delta | The JSON `{ field: { from, to } }` payload stored on every `audit_log` row. |
| Doctor | An external referring doctor with per-check pricing and cut. The "house"/internal case is the absence of a doctor on a visit. See §6.1.4. |
| Dye (صبغة) | An indicator that contrast was used on a check, adding a fixed cost (`settings.dye_cost_iqd`) and doubling the operator cut for that visit. |
| House | A visit with no `doctor_id`. The internal-doctor percentage from `settings.internal_doctor_pct` applies. |
| Lock | The receptionist action that finalizes a `draft` visit: snapshots prices, attributes the operator, consumes inventory, generates a receipt. See §8.1. |
| Operator | A radiology technician. Tracked for payroll only; not a system user. See §6.1.6. |
| Operator Shift | A clock-in/out span for an operator. Specialty + open-shift status drives operator eligibility at lock. See §6.1.8. |
| Outbox | Local queue of mutations awaiting server push. Drained by the sync engine. Defined in `.claude/rules/offline-first.md`. |
| Patient | The person receiving a check. v1 stores only the quadripartite name (اسم رباعي). See §6.1.9. |
| Receipt | The printed artifact handed to the patient at lock; A5 PDF and/or thermal text. |
| Report | An optional written radiology report, distinct from this PRD. Adds a fixed cost to the visit; does NOT affect doctor or operator cuts. |
| Settings | Singleton k/v store of global tunables: dye cost, report cost, internal-doctor percentage, etc. See §6.1.11. |
| Sync Engine | The Tokio task in the Tauri app that drains the outbox to the server and pulls peer changes. See `.claude/rules/offline-first.md`. |
| Tombstone | A soft-deleted row, marked by non-null `deleted_at`. Tombstones propagate over sync; rows are never hard-deleted. |
| Visit | A patient encounter for exactly one check, with all snapshots stored at lock. See §6.1.10. |
| Void | The superadmin action that reverses a locked visit: status → `voided`, offsetting inventory adjustments, audit row. See §8.2. |

---

End of PRD V0.1.0.
