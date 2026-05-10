# Phase 4: Operator Shifts

**Goal:** Land the `operator_shifts` entity and the receptionist-driven clock-in / clock-out workflow so the lock workflow in Phase 5 can pick from the set of currently-clocked-in operators with matching specialty.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 3.
**Complexity:** M
**PRD references:** §6.1.8 (operator_shifts), §7.1.5 (Operator Shifts page), §8.3 (Clock In / Out workflow).
**Decisions consumed:** D-002 (operator picked from clocked-in set), D-003 (operators have no login), D-016 (shifts = additive-only sync).

---

## Section 1: Local Schema Changes (Tauri SQLite)

### Migration `012_operator_shifts.sql`

```sql
CREATE TABLE IF NOT EXISTS operator_shifts (
  id                       TEXT PRIMARY KEY,
  operator_id              TEXT NOT NULL REFERENCES operators(id),
  check_in_at              TEXT NOT NULL,                   -- RFC3339 UTC
  check_out_at             TEXT NULL,                       -- null => on shift
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

-- One open shift per operator at a time.
CREATE UNIQUE INDEX operator_shifts_open_unique
  ON operator_shifts(operator_id)
  WHERE check_out_at IS NULL AND deleted_at IS NULL;

CREATE INDEX operator_shifts_operator_at
  ON operator_shifts(operator_id, check_in_at);
```

### What this phase does NOT touch
No `visits` (P5), no inventory (P6), no FTS5 changes.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

### `OperatorShift`

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

  operator       Operator @relation(fields: [operatorId], references: [id])
  checkInByUser  User     @relation("ShiftCheckIn",  fields: [checkInByUserId],  references: [id])
  checkOutByUser User?    @relation("ShiftCheckOut", fields: [checkOutByUserId], references: [id])

  @@index([entityId, operatorId, checkInAt])
  @@map("operator_shifts")
}
```

A partial-unique constraint mirroring the SQLite invariant is enforced at the **service** level (Postgres doesn't support partial-unique-on-NULL natively across all versions; we filter on `check_out_at IS NULL` in service-side validation pre-insert).

---

## Section 3: DDD Implementation

### Frontend (React)

#### New pages / routes

| Path | File | Description |
|-|-|-|
| `/reception` | `src/pages/reception/index.tsx` | Placeholder Checks Grid; populated in P5. The Operator Shifts link in the sidebar lives in this page's header. |
| `/reception/shifts` | `src/pages/reception/shifts.tsx` | Open shifts table + clock-in dialog + today's history. |

The full Reception module surface lands in P5; this phase wires the shifts page only.

#### React Query hooks

| Hook | Key |
|-|-|
| `useOpenShifts()` | `['shifts', 'open']` |
| `useShiftHistory(date)` | `['shifts', 'history', date]` |
| `useClockIn()` | mutation; invalidates `['shifts', 'open']` |
| `useClockOut(shiftId)` | mutation; invalidates `['shifts', 'open']` and the relevant history page |
| `useEditShiftRetro(shiftId, patch)` | mutation; superadmin only |

#### Zod schemas
`shift.ts` — `OperatorShiftSchema`, `ClockInPayloadSchema`, `ClockOutPayloadSchema`, `ShiftRetroEditSchema`.

#### i18n
`reception.json` namespace gets the shift-page strings (~30 keys).

### Tauri/Rust

#### Domain entity
`src-tauri/src/domains/shifts/domain/shift.rs`. Invariants in factories:
- `clock_in(operator_id, by_user_id, now)` constructor — emits `ShiftService::clock_in` events.
- `clock_out(now, by_user_id)` method — sets `check_out_at` once; idempotent via service-level open-shift check.

#### Repository trait
```rust
#[async_trait]
pub trait ShiftRepository: Send + Sync {
    async fn find_open_for_operator(&self, operator_id: Uuid) -> Result<Option<OperatorShift>, AppError>;
    async fn find_open_all(&self) -> Result<Vec<OperatorShift>, AppError>;
    async fn list_for_date(&self, date: NaiveDate) -> Result<Vec<OperatorShift>, AppError>;
    async fn upsert(&self, tx: &mut sqlx::Transaction<'_, Sqlite>, shift: &OperatorShift) -> Result<(), AppError>;
}
```

#### Tauri commands

| Command | Args | Returns | Description |
|-|-|-|-|
| `shifts_list_open` | `()` | `Vec<ShiftWithOperator>` | Currently-clocked-in operators. |
| `shifts_list_history` | `{ date: String }` | `Vec<ShiftWithOperator>` | All shifts (open + closed) for a date. |
| `shifts_clock_in` | `{ operator_id: Uuid, note?: String }` | `Uuid` | Inserts shift; rejects if open shift exists. |
| `shifts_clock_out` | `{ shift_id: Uuid, note?: String }` | `()` | Sets `check_out_at = now`. |
| `shifts_edit_retro` | `{ shift_id: Uuid, check_in_at?: String, check_out_at?: String, note?: String }` | `()` | Superadmin-only retroactive edit. |

5 IPC commands.

### Sync Server (Fastify)

#### Domain `shifts/`

Routes:
| Method | Path | Description |
|-|-|-|
| `GET` | `/shifts/open` | currently-clocked-in shifts |
| `GET` | `/shifts/history` | `?date=YYYY-MM-DD` |
| `POST` | `/shifts/clock-in` | server-side clock-in (admin tooling). |
| `PATCH` | `/shifts/:id/clock-out` | server-side clock-out. |
| `PATCH` | `/shifts/:id` | retroactive edit (superadmin only). |

Direct REST exists for parity with the desktop. The desktop pushes via the sync engine in steady state.

---

## Section 4: Business Logic

### `ShiftService`
File: `src-tauri/src/domains/shifts/services/shift_service.rs`.

Clock-in step sequence (PRD §8.3):
1. Validate `operator_id` references an active operator.
2. Open transaction.
3. Query `find_open_for_operator(operator_id)` — if Some, reject `ShiftError::AlreadyOpen`.
4. Construct `OperatorShift::clock_in(operator_id, current_user_id, now)`.
5. `with_audit(action='clock_in', entity='operator_shifts', entity_id=shift.id, delta={ check_in_at: { from: null, to: now }, operator_id: { from: null, to: operator_id } }) { repo.upsert(tx, &shift) }`.
6. Commit. Outbox enqueue.

Clock-out step sequence:
1. Read shift; reject if `check_out_at IS NOT NULL`.
2. Set `check_out_at = now`, `check_out_by_user_id = current_user`.
3. `with_audit(action='clock_out', entity='operator_shifts', entity_id=shift.id, delta={ check_out_at: { from: null, to: now } }) { repo.upsert(tx, &shift) }`.
4. Commit. Outbox enqueue.

Idempotency on double-click: clock-in's open-shift check + clock-out's `check_out_at` check both reject re-entries; UI buttons additionally disable while the mutation is in flight.

Retroactive edit:
1. Permission check: `current_user.role == Superadmin`.
2. Read shift.
3. Apply patch; validate `check_out_at >= check_in_at`.
4. `with_audit(action='update', entity='operator_shifts', delta=...) { repo.upsert(tx, &shift) }`.

### Sync semantics

`operator_shifts` is **additive-only** (D-016). Push order: by `created_at` (server reads `originDeviceId` for tiebreak). Conflicts impossible by policy — every push is appended; updates are server-applied per row's incoming `version`.

The "additive-only" label on a model with an Update operation is reconciled this way: the server applies the latest `version` that arrives, and audit log captures every change. Two devices editing the same shift simultaneously is rare in practice (only one receptionist per device) and falls back to LWW within the additive policy.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
Append `OperatorShift`. **TENANT_MODELS at end of P4 = 11.**

### Audit triggers
None.

### Local SQLite indexes
`operator_shifts_open_unique`, `operator_shifts_operator_at`.

### Capabilities / plugins
No additions.

---

## Section 6: Verification

1. Rust + frontend lint/build/test pass.
2. Migration applies cleanly; partial-unique index in place.
3. **Clock-in / out live test.**
   - Clock in operator A → row in `operator_shifts`, audit `clock_in`.
   - Clock in operator A again → rejects (`ShiftError::AlreadyOpen`).
   - Clock out operator A → `check_out_at` set; audit `clock_out`.
   - Clock in operator A again → succeeds (new row, prior is closed).
4. **Round-trip.** Clock-in on device A; observe row arrive on the server and on device B within 10s.
5. **Retroactive edit.** Superadmin edits a shift's `check_out_at` to 1h earlier; audit row records `from / to` of the changed field.
6. **Receptionist cannot edit retro.** As receptionist, the edit button is hidden; direct IPC call returns `Forbidden`.
7. **Eligibility query (preview).** A helper IPC `shifts_eligibility_for_check(check_type_id) -> Vec<OperatorOption>` is shipped and unit-tested even though the Reception form lands in P5; this guarantees P5 has a stable API.
8. **i18n + RTL** verified for the shifts page.
9. **Pre-push composite** as in P2.

### What this phase does NOT verify
- Visit creation / lock attribution (P5).
- Reception Checks Grid + Workspace (P5).
- Shift impact on operator-earnings reporting (P7).

### Summary update
Bump `status.md` row 4 to `Completed`. Add `useOpenShifts`, `useShiftHistory`, `useClockIn`, `useClockOut`, `useEditShiftRetro` to `frontend-summary.md` Section 3 and the `/reception/shifts` route to Section 1.

---

## Section 7: PRD Gap Additions

### 7.0 Operator Shifts page — Columns + States (Pass-V+) — LOW
**Gap:** PRD-writing rule §6 mandates a "States" subsection per page (empty / error / loading) plus an explicit columns list. P4 §3 mentions the page exists but doesn't enumerate.
**Category:** Incomplete Coverage.
**Remediation:** Add to P4 §3 the following per `/reception/shifts`:
- **Columns (open shifts table):** `Operator | Specialties | Since | Action`. Per PRD §7.1.5 ASCII.
- **Columns (today's history):** `Operator | In | Out | Duration | Visits run`.
- **Empty state:** "No operators on shift" with a single-button CTA "Clock in operator".
- **Loading state:** skeleton rows.
- **Error state:** inline retry banner.

### 7.1 Operator soft-delete blocked while open shift exists — MEDIUM
**Gap:** PRD §6.1.6 invariant 3 mandates that operator soft-delete is blocked while any open shift exists. Phase 3 ships `OperatorService::soft_delete` but the open-shift check requires Phase 4's `operator_shifts` table — i.e. P4 needs to extend the P3 service.
**Category:** Missing Logic (cross-phase).
**Remediation:** In Phase 4, extend `OperatorService::soft_delete`:
1. Read open-shift count: `SELECT COUNT(*) FROM operator_shifts WHERE operator_id = ? AND check_out_at IS NULL AND deleted_at IS NULL`.
2. If > 0, reject with `OperatorError::HasOpenShift { shift_id }`.
3. UI surfaces: "Cannot delete operator — currently clocked in. Clock out first, then retry."
4. Add unit test in P4: clock in operator A → try soft-delete from admin → rejected; clock out → soft-delete succeeds.

The P3 `OperatorService` either ships a `try_soft_delete` that delegates to a `ShiftCheck` trait introduced in P4, or P4 lands the override; the cleaner route is a trait + dependency-injection so P3's tests don't need to know about shifts.
