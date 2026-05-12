# IDC Frontend Summary

Cross-team handoff. Updated after EACH phase completion per [planning.md](../../.claude/rules/planning.md). Initial version covers conventions and route inventory; per-phase "Completed in this phase" sections are appended as phases ship.

## Project Intro

The IDC frontend is a React 19 SPA running inside Tauri v2's webview. It is offline-first: every read hits local SQLite via Tauri IPC, every write commits locally first and is queued for sync by the Rust backend. Network failures never produce phantom error toasts; status is reflected via a sync pill in the app shell.

Default locale `ar` with RTL. English `en` available behind a toggle. Zero literal user-facing strings outside `src/i18n/locales/`.

## Tech Stack

| Concern | Tool |
|-|-|
| Framework | React 19 |
| Build | Vite |
| Router | React Router v7 (`createBrowserRouter`) |
| Server state | TanStack React Query v5 |
| Client state | Zustand v5 |
| Validation | Zod v4 |
| Styling | Tailwind v4 (logical properties) + shadcn/ui |
| Animation | framer-motion |
| i18n | react-i18next (ar + en) |
| HTTP | axios (only for auth pre-login; all other IO via Tauri IPC) |
| Path alias | `@/` -> `src/` |

## Routing Matrix (29 PRD pages + system routes)

Owning phase indicates the phase that creates the page file. Subsequent phases may add tabs or actions.

| Path | File | Module | Roles | Owning Phase |
|-|-|-|-|-|
| `/login` | `src/pages/auth/login.tsx` | Auth | (unauth) | 02 |
| `/no-access` | `src/pages/auth/no-access.tsx` | Auth | (any) | 02 |
| `/lock` | `src/pages/auth/lock.tsx` | Auth | (any) | 02 |
| `/` | `src/pages/index/redirect.tsx` | system | (auth) | 02 (redirect logic) |
| `/reception` | `src/pages/reception/checks-grid.tsx` | Reception | receptionist, superadmin | 05 |
| `/reception/checks/:slug` | `src/pages/reception/check-workspace.tsx` | Reception | receptionist, superadmin | 05 |
| `/reception/checks/:slug/new` | `src/pages/reception/new-visit.tsx` | Reception | receptionist, superadmin | 05 |
| `/reception/visits/:id` | `src/pages/reception/visit-detail.tsx` | Reception | receptionist, superadmin | 05 |
| `/reception/shifts` | `src/pages/reception/shifts.tsx` | Reception | receptionist, superadmin | 04 |
| `/accounting` | `src/pages/accounting/dashboard.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/visits` | `src/pages/accounting/visits.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/visits/:id` | `src/pages/accounting/visit-drill.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/doctors` | `src/pages/accounting/doctors.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/doctors/:id` | `src/pages/accounting/doctor-detail.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/operators` | `src/pages/accounting/operators.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/operators/:id` | `src/pages/accounting/operator-detail.tsx` | Accounting | accountant, superadmin | 07 |
| `/accounting/daily-close` | `src/pages/accounting/daily-close.tsx` | Accounting | accountant, superadmin | 07 |
| `/inventory` | `src/pages/inventory/list.tsx` | Inventory | receptionist (limited), accountant (read), superadmin | 06 |
| `/inventory/items/:id` | `src/pages/inventory/detail.tsx` | Inventory | same | 06 |
| `/inventory/adjust` | `src/pages/inventory/adjust.tsx` | Inventory | receptionist (receive/writeoff), superadmin | 06 |
| `/admin/users` | `src/pages/admin/users/list.tsx` | Admin | superadmin | 02 |
| `/admin/users/:id` | `src/pages/admin/users/detail.tsx` | Admin | superadmin | 02 |
| `/admin/check-types` | `src/pages/admin/check-types/list.tsx` | Admin | superadmin | 03 |
| `/admin/check-types/:id` | `src/pages/admin/check-types/detail.tsx` | Admin | superadmin | 03 |
| `/admin/doctors` | `src/pages/admin/doctors/list.tsx` | Admin | superadmin | 03 |
| `/admin/doctors/:id` | `src/pages/admin/doctors/detail.tsx` | Admin | superadmin | 03 |
| `/admin/operators` | `src/pages/admin/operators/list.tsx` | Admin | superadmin | 03 |
| `/admin/operators/:id` | `src/pages/admin/operators/detail.tsx` | Admin | superadmin | 03 |
| `/admin/inventory` | `src/pages/admin/inventory/list.tsx` | Admin | superadmin | 03 |
| `/admin/inventory/:id` | `src/pages/admin/inventory/detail.tsx` | Admin | superadmin | 03 |
| `/admin/settings` | `src/pages/admin/settings.tsx` | Admin | superadmin | 02 |
| `/audit` | `src/pages/audit/index.tsx` | Audit | superadmin | 08 |
| `/sync/conflicts` | `src/pages/sync/conflicts.tsx` | system | superadmin | 08 |

PRD §3.2 enumerates 29 module pages. The matrix above also includes 4 system routes (`/login`, `/no-access`, `/lock`, `/sync/conflicts`) and the `/` redirect, for 34 entries.

## React Query Key Registry

Conventions:

- Keys are arrays; the first element is the module namespace.
- Filter / id segments are stable JSON.
- Mutations invalidate the narrowest applicable list keys.

| Namespace | Keys (seed) |
|-|-|
| `sync` | `['sync','status']`, `['sync','conflicts']` |
| `device` | `['device','info']` |
| `auth` | `['auth','currentUser']` |
| `users` | `['users','list']`, `['users','detail', id]` |
| `settings` | `['settings','all']` |
| `catalog` | `['catalog','checkTypes','list']`, `['catalog','checkSubtypes', typeId]`, `['catalog','doctors','list']`, `['catalog','doctors', id]`, `['catalog','operators','list']`, `['catalog','operators', id]`, `['catalog','inventory','list']`, `['catalog','inventory', id]` |
| `shifts` | `['shifts','open']`, `['shifts','today']` |
| `patients` | `['patients','search', query]`, `['patients','detail', id]` |
| `visits` | `['visits','byCheck', typeId, 'today']`, `['visits','detail', id]`, `['visits','audit', id]`, `['visits','receipts', id]` |
| `operators` | `['operators','qualified', checkTypeId]` |
| `inventory` | `['inventory','items','list', filter]`, `['inventory','items','detail', id]`, `['inventory','adjustments', itemId]`, `['inventory','audit', itemId]` |
| `reports` | `['reports','dashboard', range]`, `['reports','visits', filters]`, `['reports','doctors', range]`, `['reports','doctor', id, range]`, `['reports','operators', range]`, `['reports','operator', id, range]`, `['reports','dailyClose', date]` |
| `audit` | `['audit','query', filters]` |

## Zustand Store Registry

| Store | File | Purpose | Persisted? | Synced? |
|-|-|-|-|-|
| `useSyncStatusStore` | `src/stores/sync-status-store.ts` | Sync pill state, conflicts list | No (in-memory) | No |
| `useDeviceStore` | `src/stores/device-store.ts` | Device id, app version | Yes (`tauri-plugin-store`) | No |
| `useAuthStore` | `src/stores/auth-store.ts` | Current user, tokens, locked flag | Tokens via stronghold; user via store | No |
| `useIdleStore` | `src/stores/idle-store.ts` | Last activity timestamp | No (in-memory) | No |
| `useAdminNavStore` | `src/stores/admin-nav-store.ts` | Active admin sub-page | Yes (per device) | No |
| `useDraftVisitStore` | `src/stores/draft-visit-store.ts` | Per-route draft cache | Yes (per device) | No |
| `useAccountingFiltersStore` | `src/stores/accounting-filters-store.ts` | Current filter window | Yes (per device) | No |

## Conventions Baseline

- **i18n.** Every visible string resolves via `useTranslation('<namespace>')`. Namespaces: `common`, `auth`, `reception`, `accounting`, `inventory`, `admin`, `audit`, `errors`, `receipts`.
- **Tailwind.** Use logical properties only: `ps-*`, `pe-*`, `ms-*`, `me-*`, `text-start`, `text-end`. No `pl-*`, `pr-*`, `ml-*`, `mr-*`, `text-left`, `text-right` in feature code. Chevrons use `rtl:rotate-180`.
- **Zod.** Schemas live in `src/lib/schemas/<entity>.ts`. Mutations parse inputs via Zod before dispatching IPC.
- **IPC.** All Tauri IPC routed through `src/lib/ipc.ts` (typed wrapper). Frontend never calls `invoke()` directly outside that file.
- **Errors.** Domain errors come back from Tauri as `AppError` with a code + i18n key. UI maps codes to `errors.*` translations.
- **Routing.** `createBrowserRouter` only; no file-based routing. Role gates implemented via a `<RequireRole roles={...}>` wrapper around the route element.
- **Loading / error / empty.** Every list and detail page implements explicit Skeleton, Error, and Empty components. No raw "loading..." text.

## Completed in This Phase

_None yet. This section is appended after each phase ships with the screens, hooks, stores, and conventions delivered in that phase._

### Phase 01

_pending_

### Phase 02

_pending_

### Phase 03

_pending_

### Phase 04

Shipped 2026-05-12.

**Route surface**
- `/reception/shifts` -- new page guarded by `<RequireRole roles={['receptionist','superadmin']}>` wrapper around the `/reception/*` outlet (lays the groundwork for phase-05).
- Sidebar: new Shifts link under the Operations group; visible to receptionist + superadmin, hidden for accountant.

**Pages / components**
- `src/pages/reception/shifts.tsx` -- header (eyebrow + refresh + crimson primary "Clock in operator"), conflict banner slot, two stacked tables.
- `src/components/reception/clock-in-dialog.tsx` -- operator combobox filtered to active operators NOT on an open shift; optional note; submit dispatches `shifts::clock_in`. Inner-body component pattern (wrapper returns `null` when closed) avoids `react-hooks/set-state-in-effect`.
- `src/components/reception/on-shift-table.tsx` -- Operator / phone / since (with pulsing live status pill) / clock-out button.
- `src/components/reception/shift-history-today.tsx` -- Operator / in / out / duration / lines-run (placeholder `0`) / edit (superadmin only).
- `src/components/reception/retroactive-shift-editor.tsx` -- superadmin form for `(check_in_at, check_out_at, note)` with `<input type="datetime-local">` paired to RFC3339 round-trip.
- `src/components/reception/open-shift-conflict-banner.tsx` + `resolve-overlapping-shifts.tsx` -- render only when `useShiftOverlaps()` returns rows; resolver lists the involved shifts and lets an admin close-now or soft-delete duplicates.

**State + IPC**
- `src/features/shifts/queries.ts` -- `useOpenShifts`, `useShiftHistoryToday`, `useShiftOverlaps`, `useShiftClockIn`, `useShiftClockOut`, `useShiftEdit`, `useShiftSoftDelete`. Mutations invalidate the shared `['shifts']` key on success.
- `src/lib/schemas/shift.ts` -- Zod schemas for clock-in / clock-out / edit / soft-delete inputs (used by future React Hook Form integrations and also as runtime guards before IPC).
- `src/lib/format/duration.ts` -- `formatTime`, `formatDuration`, `formatSince` helpers (mono Geist with `tnum`).
- IPC type map gained `shifts_clock_in` / `shifts_clock_out` / `shifts_list_open` / `shifts_history_today` / `shifts_edit` / `shifts_soft_delete` / `shifts_list_overlaps` plus shared `ShiftRecord` / `ShiftWithMetaRecord` / `ShiftOverlapPair` shapes.

**i18n**
- New `reception` namespace (en + ar) with `reception.shifts.*` keys for the page title, table headers, dialog copy, and the overlap banner / resolver.
- Added missing `admin.close` / `admin.refresh` / `admin.loading` keys to keep the editorial chrome consistent.
- Sidebar `nav.shifts` added in en + ar.

**Notes for downstream phases**
- The lines-run column is a `0` placeholder. Phase-05 must add `shifts::lines_run_today(operator_id) -> u32` and update `<ShiftHistoryToday>` (§7.7).
- Visit lock eligibility in phase-05 should use `useOpenShifts()` to filter operators available to assign on a visit.
- The conflict resolver currently soft-deletes orphans; phase-08 may add an audit-log filter to surface the resolution trail in the global audit view.

### Phase 05

_pending_

### Phase 06

_pending_

### Phase 07

_pending_

### Phase 08

_pending_
