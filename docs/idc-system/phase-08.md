# Phase 8: Void Workflow

**Goal:** Land the superadmin-only void action: confirms with reason, soft-deletes the visit, writes offsetting `inventory_adjustments` rows, refreshes materialized stock counts, audits the reversal, and re-renders a "VOIDED" receipt.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 6 (inventory rollback), Phase 7 (accounting reflects voids — already does, but verified here).
**Complexity:** M
**PRD references:** §6.1.10 invariant 8 (superadmin-only voiding), §8.2 (Void workflow).
**Decisions consumed:** D-009 (roles), D-010 (void = soft-delete + reversal, no separate refund record), D-011 (receipt prints), Q-005 (voided receipt watermark).

---

## Section 1: Local Schema Changes (Tauri SQLite)

**No new tables.** Void writes existing `visits`, `inventory_adjustments`, `audit_log`.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

**No new models.**

The `Visit` model's `voided_at`, `voided_by_user_id`, `void_reason` columns already exist from Phase 5; this phase fills them.

---

## Section 3: DDD Implementation

### Frontend (React)

#### Affected pages

`/reception/visits/:id` and `/accounting/visits/:id` (the latter redirects to the former in P7) gain a "Void" button visible only when `useAuthStore().role === 'superadmin'` AND `visit.status === 'locked'`.

#### Components

`src/components/reception/VoidVisitDialog.tsx`:
- Confirms intent (modal).
- `Reason` text area; Zod `z.string().trim().min(5).max(500)`.
- Cancel + "Void this visit" buttons. The action button is destructive (red).

#### React Query hooks
- `useVoidVisit()` — mutation; on success invalidates the visit detail key, the workspace list key, the dashboard KPI key, and the affected inventory items.

### Tauri/Rust

#### Tauri command

| Command | Args | Returns | Description |
|-|-|-|-|
| `visits_void` | `{ visit_id: Uuid, reason: String }` | `LockResult` (re-rendered receipt paths) | Runs the void workflow (Section 4). Returns the same `LockResult` shape so the UI can offer a re-print. |

1 IPC command.

### Sync Server (Fastify)

`PATCH /visits/:id/void` — previously a 501 stub from Phase 5 — now implements server-side void parity. Body: `{ reason: string }`. Same step sequence as the Tauri-side service.

---

## Section 4: Business Logic

### `VisitService::void`

File: `src-tauri/src/domains/visits/services/visit_service.rs` (extends P5).

Step sequence (PRD §8.2):

1. **Permission check.** `current_user.role == Superadmin`. Else `VoidError::Forbidden`.
2. **Validate.** `visit.status == 'locked'`. Else `VoidError::NotLocked`.
3. **Validate reason.** `reason.trim().len() >= 5`. Else `VoidError::ReasonTooShort`.
4. **Begin transaction.**
5. **Mutate visit.** Set `status='voided'`, `voided_at = now`, `voided_by_user_id = current_user`, `void_reason = reason`. Bump `version`, `updated_at`, `dirty=1`. `repo.upsert(tx, &visit)`.
6. **Reverse inventory.** Read all `inventory_adjustments` where `visit_id = visit.id AND reason = 'consume_visit' AND deleted_at IS NULL`. For each, create an offsetting row:
   ```rust
   InventoryAdjustment {
       id: uuid_v7(),
       item_id: original.item_id,
       delta: -original.delta,                   // sign-flipped (positive)
       reason: AdjustmentReason::ConsumeVisit,    // same reason; sign distinguishes
       visit_id: Some(visit.id),
       note: Some(format!("void reversal of {}", original.id)),
       by_user_id: current_user_id,
       ...
   }
   ```
   `repo.upsert(tx, &offset)` and recompute `quantity_on_hand` for each affected item.
7. **Audit.** One `audit_log` row for the `void` action with delta `{ status: { from: "locked", to: "voided" }, void_reason: { from: null, to: <reason> }, voided_at: { from: null, to: <now> } }`. Plus one `create` row per offsetting adjustment.
8. **Re-render receipt.** `ReceiptRenderer::render_voided(visit, locale)` — same visit data, but with a top banner `ملغي / VOIDED` and a watermark behind the line items. Persisted alongside the original receipt with a `-voided` suffix on the filename.
9. **Commit.** Outbox enqueue: visit row + N adjustment rows + N item rows + audit rows.
10. **Return `LockResult`** (so the UI can re-print).

### Sync semantics

`visits` is **manual** policy. A void from device A and an edit from device B (on the same locked visit) is a real conflict that the resolver UI (P9) surfaces. In practice this is rare; the typical case is "superadmin voids on the same device that locked".

`inventory_adjustments` is additive-only — the offsetting rows are appended; the originals stay (audit trail).

### Frontend flow

1. Click Void on Visit Detail.
2. Modal opens; reason input.
3. On confirm, `visits_void({ visit_id, reason })`.
4. On success, the page reloads to the voided state; the system print dialog offers the re-rendered "VOIDED" receipt.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
None (no new models).

### Audit triggers
None.

### Local SQLite indexes
None new (existing indexes cover the void path).

### Capabilities / plugins
None.

---

## Section 6: Verification

1. Lint / build / test pass.
2. **Permission gate.** Receptionist sees no Void button on a locked visit; direct IPC call returns `Forbidden`.
3. **Reason validation.** Empty reason rejected. Reason of "abc" rejected (<5 chars). "wrong patient" accepted.
4. **Void rollback.** Lock a "مفراس" visit with `dye=1` consuming 1 unit of contrast. Stock = 18. Void it. Stock returns to 19. Two `consume_visit` rows exist (one negative, one positive); audit log shows both writes.
5. **Voided visit appears in reports.** Accounting visits report filtered to `status=voided` lists the row; dashboard "voided today" KPI increments.
6. **Voided visit excluded from active aggregates.** Doctor earnings + operator earnings exclude voided rows by default.
7. **Receipt re-print.** Voided receipt PDF has the banner + watermark in both locales.
8. **Sync round-trip.** Void on device A; observe row + audit + offsetting adjustments on device B within 10s.
9. **Re-void prevented.** Voiding an already-voided visit returns `NotLocked`.
10. **i18n + RTL** verified for the void dialog and voided receipt.
11. **Pre-push composite.**

### What this phase does NOT verify
- Conflict-resolver UI (P9) — the engine logs conflicts but resolution is in P9.
- Audit page UI (P9).
- Backup workflow (P10).

### Summary update
Bump `status.md` row 8 to `Completed`. Add `useVoidVisit` to `frontend-summary.md` Section 3. Note `VoidVisitDialog` under shadcn-using components.
