# Phase 4: Operator Shifts

**Goal:** Ship clock-in / clock-out for operators so visit-lock eligibility in Phase 5 has the data it needs. Includes the `/reception/shifts` page and superadmin retroactive edits.

**Surfaces:** Frontend, Tauri/Rust, Sync Server
**Dependencies:** Phase 03
**Complexity:** S

## §1 Local Schema Changes (Tauri SQLite)

Migration file: `src-tauri/migrations/004_operator_shifts.sql`.

### operator_shifts (PRD §6.1.8)

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

### Modified tables

None.

### New enums

None (action enum extended below in audit).

## §2 Server Schema Changes (Prisma / Postgres)

### OperatorShift (PRD §6.1.8)

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

### New enums

None.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/reception/shifts` | `src/pages/reception/shifts.tsx` | On-shift table + today's history table. |

Components:

| Component | File | Purpose |
|-|-|-|
| `<OnShiftTable>` | `src/components/reception/on-shift-table.tsx` | Operator, specialties, since, clock-out button. |
| `<ShiftHistoryToday>` | `src/components/reception/shift-history-today.tsx` | Operator, in, out, duration, lines run (lines-run column is `0` until Phase 5; column rendered with placeholder). |
| `<ClockInDialog>` | `src/components/reception/clock-in-dialog.tsx` | Operator picker + optional note. |
| `<RetroactiveShiftEditor>` | `src/components/reception/retroactive-shift-editor.tsx` | Superadmin-only; edits `check_in_at` / `check_out_at`. |

Zustand stores: none new.

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useOpenShifts` | `['shifts','open']` | Currently clocked in. |
| `useShiftHistoryToday` | `['shifts','today']` | Today's shifts, both open and closed. |
| `useShiftClockIn` | mutation | IPC `shifts::clock_in`. |
| `useShiftClockOut` | mutation | IPC `shifts::clock_out`. |
| `useShiftEdit` | mutation | IPC `shifts::edit`. |

Zod schemas:

| Schema | File |
|-|-|
| `ShiftSchema` | `src/lib/schemas/shift.ts` |
| `ClockInInputSchema` | `src/lib/schemas/shift.ts` |
| `ClockOutInputSchema` | `src/lib/schemas/shift.ts` |
| `ShiftEditSchema` | `src/lib/schemas/shift.ts` |

### Tauri / Rust

Domain entity (in `src-tauri/src/domains/shifts/`):

```rust
pub struct OperatorShift {
  pub id: Uuid,
  pub operator_id: Uuid,
  pub check_in_at: DateTime<Utc>,
  pub check_out_at: Option<DateTime<Utc>>,
  pub check_in_by_user_id: Uuid,
  pub check_out_by_user_id: Option<Uuid>,
  pub note: Option<String>,
  pub entity_id: String,
}
impl OperatorShift {
  pub fn open(operator_id: Uuid, by_user_id: Uuid, note: Option<String>) -> Result<Self, AppError> { ... }
  pub fn close(self, by_user_id: Uuid, at: DateTime<Utc>) -> Result<Self, AppError> {
    /* checks self.check_out_at is None and at >= check_in_at */
  }
  pub fn edit_times(self, in_at: DateTime<Utc>, out_at: Option<DateTime<Utc>>) -> Result<Self, AppError> { ... }
}
```

Repository trait:

```rust
#[async_trait]
pub trait OperatorShiftRepo {
  async fn open(&self, tx: &mut Tx, shift: OperatorShift) -> Result<(), AppError>;
  async fn close(&self, tx: &mut Tx, shift: OperatorShift) -> Result<(), AppError>;
  async fn edit(&self, tx: &mut Tx, shift: OperatorShift) -> Result<(), AppError>;
  async fn list_open(&self) -> Result<Vec<OperatorShift>, AppError>;
  async fn history_today(&self) -> Result<Vec<OperatorShift>, AppError>;
  async fn has_open_for_operator(&self, operator_id: Uuid) -> Result<bool, AppError>;
}
```

SQLite repo notes: the partial unique index `operator_shifts_open` plus a pre-write check enforces single-open-per-operator. The pre-write check is necessary because the partial index does not by itself stop an upsert from another column path.

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `shifts::clock_in` | `{ operatorId, note? }` | `OperatorShift` | Audit `clock_in`. |
| `shifts::clock_out` | `{ shiftId }` | `OperatorShift` | Audit `clock_out`. |
| `shifts::list_open` | none | `OperatorShift[]` | With joined operator name + specialties. |
| `shifts::history_today` | none | `OperatorShift[]` | Today's open + closed. |
| `shifts::edit` | `{ shiftId, checkInAt, checkOutAt? }` | `OperatorShift` | Superadmin only; audit `update` with delta. |

Register in `src-tauri/src/lib.rs::generate_handler!`. Additionally, audit action enum expands to include `clock_in` and `clock_out` (already in the PRD §6.1.15 enum list).

### Sync Server (Fastify)

Entity class:

```ts
class OperatorShift {
  static create(input): OperatorShift { /* validate check_out_at >= check_in_at */ }
  toResponse(): OperatorShiftResponse { ... }
}
```

Repository interface:

```ts
interface OperatorShiftRepository {
  upsert(shift: OperatorShift): Promise<OperatorShift>;
  listForTenant(params: ShiftListParams): Promise<OperatorShift[]>;
}
```

Prisma repo notes: `additive-only` policy implementation means the server accepts every push and chronologically orders by `created_at`; if a shift with the same `id` is pushed twice, the second push is a no-op (already in `ProcessedOp`).

TypeBox schemas:

| Schema | Purpose |
|-|-|
| `OperatorShiftResponseSchema` | Response shape. |
| `OperatorShiftPushSchema` | Push payload (full row). |

Route table:

| Method | Path | Description |
|-|-|-|
| (no new routes) | n/a | Shifts flow through `/sync/push` and `/sync/pull`. |

## §4 Business Logic

### Frontend

`<ClockInDialog>` flow:

1. Operator combobox lists active operators NOT currently on an open shift.
2. Optional note.
3. Submit dispatches `shifts::clock_in`.
4. On success, optimistically updates `['shifts','open']` and `['shifts','today']` query caches.

`<OnShiftTable>` clock-out button:

1. Calls `shifts::clock_out` with the shift id.
2. Updates the same two caches.

### Tauri / Rust

`ShiftService::clock_in(operator_id, by_user_id, note)`:

1. Validate operator is `is_active = 1`.
2. Validate via `OperatorShiftRepo::has_open_for_operator` that no open shift exists.
3. `with_audit(action='clock_in', entity='operator_shifts', entity_id=new_id)` inserting the row; outbox enqueued.

`ShiftService::clock_out(shift_id, by_user_id)`:

1. Load shift; reject if `check_out_at IS NOT NULL` or `deleted_at IS NOT NULL`.
2. Compute `at = now`.
3. `with_audit(action='clock_out', entity='operator_shifts', entity_id=shift_id)` updating the row.

`ShiftService::edit(shift_id, new_in, new_out, by_user_id)`:

1. Caller role must be `superadmin`; else `Forbidden`.
2. Validate `new_out IS NULL OR new_out >= new_in`.
3. `with_audit(action='update', ...)` updating the row; delta covers `check_in_at` and `check_out_at`.

### Sync Server

`OperatorShift` insertion/upsert: pure additive policy; no conflict response.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `operator_shifts` | `additive-only` | `op_id` | Distinct shift rows from distinct devices never collide because each device emits a fresh `id`. Retroactive edits are sync-pushed as updates and applied LWW within the additive frame (per `additive-only` definition: both writes survive; ordering by `created_at`; updates favor higher `version`). |

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

```ts
export const TENANT_MODELS = [
  'audit_log', 'users', 'settings',
  'check_types', 'check_subtypes',
  'doctors', 'doctor_check_pricing',
  'operators', 'operator_specialties',
  'inventory_items', 'inventory_consumption_map',
  'operator_shifts',
] as const;
```

### Audit trigger additions

None.

### Local SQLite indexes

- `operator_shifts_open` (partial, `WHERE check_out_at IS NULL AND deleted_at IS NULL`).

### Tauri capabilities

No new scopes.

### Plugin registrations

None new.

### What this phase does NOT touch

- No new sync contracts beyond `additive-only` for `operator_shifts`.
- No visit/patient/inventory adjustment entities (Phase 5).
- No reports.

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover clock-in, double-clock-in rejection, clock-out, edit by non-superadmin rejection, edit by superadmin acceptance.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`: navigate to `/reception/shifts`; clock an operator in; assert they appear in the on-shift list; clock them out; assert they move to today's history.
5. `cd sync-server && pnpm test`: shift push acceptance and pull echo.
6. Sync round-trip: clock in on device A; reconnect; assert device B's pull surfaces the same open shift; clock the operator out from device B; reconnect device A; pull; assert the closed time syncs.
7. Conflict (additive ordering): generate two shifts for the same operator with overlapping times on two devices; assert both rows survive (additive); local UI surfaces the overlap to admin without auto-resolution.
8. Idempotency: replay the same clock-in op via `/sync/push`; assert no duplicate row.
9. Audit: every clock-in writes a `clock_in` audit row; every clock-out writes a `clock_out` audit row; superadmin edit writes an `update` row with delta.
10. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 7 gaps incorporated below._

### 7.1 Concurrent open-shift surfacing across devices
- **Gap:** HIGH | Incomplete Coverage | PRD §8.3 + §9.3
- The local partial-unique-index enforces single-open-shift per operator, but the additive-only sync policy accepts both rows when two devices push concurrent clock-in events. Phase-04 §6 verification step 7 says "local UI surfaces the overlap to admin without auto-resolution" but there is no actual UI element or detector.
- **Resolution:** Add to §3.Frontend:
  - Detector hook `useOverlappingShifts()` that returns rows where `(operator_id, check_in_at..check_out_at)` overlaps another row for the same operator.
  - `<OpenShiftConflictBanner>` rendered above `<ShiftsPage>` and `<NewVisitForm>` whenever the hook returns a non-empty set, with a "Resolve" action that routes to a new modal `<ResolveOverlappingShifts>` (admin-only). The modal lets an admin pick which shift to close (sets `check_out_at` on the earlier row, soft-deletes the orphan).
  - Add IPC `shifts::list_overlaps() -> Vec<OverlapPair>`.

### 7.2 `(entity_id, check_in_at)` index for history-today
- **Gap:** LOW | Missing Index | research.md
- §1 declares `operator_shifts_open` (partial) and `operator_shifts_operator`; neither covers the `history_today()` query (`WHERE entity_id = ? AND date(check_in_at) = ?`).
- **Resolution:** Append to §1 migration:
  ```sql
  CREATE INDEX operator_shifts_today ON operator_shifts(entity_id, check_in_at);
  ```

### 7.3 `clock_in` / `clock_out` audit-action enum extensions
- **Gap:** MEDIUM | Missing Audit Trigger | PRD §6.1.15
- §5 says "No new triggers needed" but does not confirm that `audit_log.action` accepts `clock_in` and `clock_out`. Phase-01 §7.8 documents the full union; phase-04 should re-state.
- **Resolution:** Add §5 note: "Audit action union extended to include `clock_in` and `clock_out` (per phase-01 §7.8). Both values are written by `ShiftService::clock_in` and `ShiftService::clock_out` via `with_audit`." No DB CHECK change required.

### 7.4 `ShiftService::edit` overlap validation
- **Gap:** LOW | Missing Validation | PRD §8.3
- `ShiftService::edit` validates `new_out >= new_in` but does not prevent edits that would re-open a shift (`new_out := NULL`) while another shift is already open for the same operator.
- **Resolution:** Append to `ShiftService::edit` step list:
  - Before applying the edit, if `new_check_out_at IS NULL`, run `SELECT count(*) FROM operator_shifts WHERE operator_id = ? AND check_out_at IS NULL AND deleted_at IS NULL AND id != ?`; if > 0 → return `ShiftError::OpenShiftExists`.
  - Write the audit `update` row with the old/new times delta.

### 7.5 `<ShiftsPage>` empty/error/loading states
- **Gap:** HIGH | Missing UI Element | frontend-summary §Conventions
- Frontend-summary mandates explicit Skeleton/Error/Empty states on every list page; §3.Frontend `<ShiftsPage>` description omits them.
- **Resolution:** Extend `<ShiftsPage>` row in §3.Frontend table:
  > States: `<Skeleton>` while `useOpenShifts` is loading; `<Empty action="Clock in">` when no open shifts and no history rows for today; `<ErrorState>` with a "Retry" button on query failure. Each state honors RTL and i18n via `reception:shifts.*` keys (namespace added in this phase).

### 7.6 Cross-device clock-out race resolution
- **Gap:** MEDIUM | Missing Concurrency Guard | PRD §8.3
- Two devices clocking out the same operator: both increment `version` locally and push; server uses LWW within additive (later `check_out_at` wins). Phase-04 §4 sync semantics line doesn't specify this nuance.
- **Resolution:** Append to §4 sync semantics for `operator_shifts`: "Updates to an existing row (clock-out, retroactive edit) reuse the row's `id`; conflict resolution is LWW within the additive policy: server keeps the higher `updated_at` (`origin_device_id` lex tiebreak). Pure inserts (new clock-in) are kept additively without conflict." Add to server `ShiftService::accept_push` a step that distinguishes insert vs update by row presence.

### 7.7 Lines-run column wiring deferred to phase-05
- **Gap:** LOW | Incomplete Coverage | PRD §7.1.5
- §3.Frontend `<ShiftHistoryToday>` shows "Lines run" column as `0` placeholder. Phase-05 owns the visit-count query but never wires it back.
- **Resolution:** Append to §3.Frontend table: "Lines-run column is a placeholder of `0` in phase-04; phase-05 §7 adds the IPC `shifts::lines_run_today(operator_id) -> u32` and updates `<ShiftHistoryToday>` to call it." Cross-referenced in phase-05 §7.

### 7.8 Shift retroactive edit illegal-transition guards
- **Gap:** HIGH | Missing State Transition | PRD §6.1.8
- §4 `ShiftService::edit` allows `(check_in_at, check_out_at)` rewrites but does NOT block (a) editing a soft-deleted shift, (b) setting `check_out_at < check_in_at`, (c) editing a shift the caller did not author when the caller is not superadmin.
- **Resolution:** Add to `ShiftService::edit` step list:
  ```
  0. Load shift; if shift.deleted_at IS NOT NULL → Err(ShiftError::Deleted).
  1. require_role(&ctx, &[Role::Superadmin]) → Err(ShiftError::Forbidden) otherwise.
  2. If new_check_in_at provided AND new_check_in_at > now → Err(ShiftError::CheckInInFuture).
  3. If new_check_out_at provided AND new_check_out_at < (new_check_in_at OR shift.check_in_at) → Err(ShiftError::CheckOutBeforeCheckIn).
  4. If new_check_in_at provided: check no overlap with other non-deleted shifts of the same operator.
  5. with_audit('update','operator_shifts', shift.id, delta) wrapping the UPDATE.
  ```
  Mirror in `OperatorShift::edit_times` domain method; mirror in server `ShiftService::acceptPush` for the update branch.

### 7.9 Operator-shift soft-delete propagation under additive policy
- **Gap:** MEDIUM | Missing Sync Rule | PRD §6.1.8
- §4 Sync Semantics declares `additive-only` but doesn't say how `deleted_at` propagates. A superadmin soft-deleting a stray shift on device A then device B re-syncing creates ambiguity.
- **Resolution:** Append to §4 sync semantics row for `operator_shifts`: "Soft-delete is a permitted update under additive-only. Server applies the latest `deleted_at` (LWW within additive frame: higher `updated_at` wins, `origin_device_id` lex tiebreak). A hard delete is never emitted; a deleted shift remains in the audit graph. Idempotency: `op_id` covers both insert and update including soft-delete events; replays of the same `op_id` return the cached `ProcessedOp` response regardless of operation kind."

### 7.10 `shifts::soft_delete` IPC
- **Gap:** MEDIUM | Missing IPC | PRD §6.1.8
- §7.1 (Pass-1) introduces `<ResolveOverlappingShifts>` UI but the IPC needed to soft-delete an orphan shift is not in §3 commands.
- **Resolution:** Add to §3 Tauri commands: `shifts::soft_delete | { shift_id: Uuid, reason: String } | () | Superadmin only; sets deleted_at, bumps version+dirty, writes audit 'soft_delete' with the reason captured in delta. Used by <ResolveOverlappingShifts> from §7.1.` Service: 1) require_role superadmin, 2) load shift; reject if already deleted, 3) `with_audit('soft_delete','operator_shifts', shift.id, delta={reason})` wrapping UPDATE; 4) enqueue outbox under additive contract.

### 7.11 Audit-first ordering for clock_in / clock_out
- **Gap:** HIGH | Wrong Order | PRD §4.3, phase-01 §7.7
- Phase-01 §7.7 fixed `with_audit` to be audit-first (audit row inserted BEFORE the business write). Phase-04 §4 `ShiftService::clock_in` / `clock_out` describe `with_audit(...)` as wrapping the insert/update but do not explicitly invoke the two-pass closure.
- **Resolution:** Append to §4 ShiftService step lists: "Each of `clock_in`, `clock_out`, `edit`, `soft_delete` invokes the two-pass `with_audit` from phase-01 §7.7: (1) INSERT audit_log row, (2) execute the OperatorShiftRepo write closure, (3) enqueue outbox row(s), (4) commit the tx. The wrapper guarantees audit-first ordering even on partial failure." Add §6 verification: a forced failure of step 2 must leave the audit row absent (rolled back) and the shift row unchanged.

### 7.12 Overlap-shifts verification step
- **Gap:** MEDIUM | Missing Verification | §6
- §7.1 adds `shifts::list_overlaps`, `<OpenShiftConflictBanner>`, `<ResolveOverlappingShifts>`. §6 step 7 only asserts "local UI surfaces the overlap" in general prose.
- **Resolution:** Add §6 verification steps:
  > 10. Concurrent open-shift detection: push two `clock_in` rows for the same operator from two devices (use `mcp__curl__curl_post /sync/push` with crafted op_ids). After sync, assert `shifts::list_overlaps(operator_id)` returns the two rows; assert `<OpenShiftConflictBanner>` renders inside `<ShiftsPage>`.
  > 11. Overlap resolution: open `<ResolveOverlappingShifts>`, choose to close one shift now and soft-delete the other. Assert exactly one shift remains open in `shifts::list_open()`; assert one audit row `clock_out` and one `soft_delete` are written; assert the corresponding outbox rows enqueue under additive contract.

### 7.13 `OperatorShift` server `pulledAt` column
- **Gap:** MEDIUM | Missing Field | Pass-3 GAP-A-6 part 1; PRD line 302
- PRD line 302 mandates `pulledAt` on every server Prisma model. §2 `model OperatorShift` lacks the field; no §7.x retrofit exists. Diagnostics in phase-08 §7.17 are blind to last-pull timestamps.
- **Resolution:** Add to §2 `model OperatorShift`:
  ```prisma
  pulledAt DateTime? @map("pulled_at") @db.Timestamptz
  ```
  Set by `SyncPullService` after each successful pull batch ship. Mirrors phase-03 §7.19 pattern.

### 7.14 `operator_shifts` FK ON DELETE policy
- **Gap:** LOW | Missing FK Policy | Pass-3 GAP-B-5
- §1 declares `check_in_by_user_id TEXT NOT NULL REFERENCES users(id)` and `check_out_by_user_id TEXT NULL REFERENCES users(id)` with no `ON DELETE` clause. SQLite default is `NO ACTION`. Hard-delete of a user is not anticipated (PRD §6.1.1 inv 4 mandates soft-delete only) but the DB-level guarantee is absent.
- **Resolution:** Update both FK declarations in §1 to `ON DELETE RESTRICT`:
  ```sql
  check_in_by_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  check_out_by_user_id  TEXT NULL REFERENCES users(id) ON DELETE RESTRICT,
  ```
  Mirror in §2 Prisma `onDelete: Restrict`. Documents the explicit intent that user hard-delete is forbidden.

### 7.15 `<ShiftsPageHeader>` and `<EditShiftRowAction>` declared
- **Gap:** MEDIUM | Missing UI Element | PRD §7.1.5 lines 1711, 1730-1732; Pass-3 GAP-E-5
- PRD §7.1.5 ASCII shows top-right `[+ Clock in operator]` action and Actions table includes "Edit shift retroactively" with inline trigger. §3 declares `<ClockInDialog>` and `<RetroactiveShiftEditor>` but no header action button or row-edit trigger.
- **Resolution:** Add to §3 Frontend components:
  - `<ShiftsPageHeader>` -- top-right `[+ Clock in operator]` button; opens `<ClockInDialog>`. i18n key `reception.shifts.actions.clock_in_operator`.
  - `<EditShiftRowAction>` -- icon-button on each `<ShiftHistoryToday>` row, gated `useCurrentUser().role === 'superadmin'`; opens `<RetroactiveShiftEditor>` for that row. i18n key `reception.shifts.actions.edit_shift`.
  Both wired into `<ShiftsPage>` layout per the PRD ASCII.

### 7.16 `/reception/*` route role gate (cross-reference)
- **Gap:** HIGH | Missing Role Guard | PRD navigation tree §3.1; Pass-3 GAP-E-7
- PRD restricts `/reception/*` to `receptionist, superadmin`. No phase declared the wrapper.
- **Resolution:** Owned by phase-05 §7.58 (which declares the `<RequireRole roles={['receptionist','superadmin']}>` wrapper around the `/reception/*` outlet). `/reception/shifts` (this phase) inherits the wrapper. Cross-reference receipt only.
