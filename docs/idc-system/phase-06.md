# Phase 6: Inventory Operations

**Goal:** Ship the operational layer over `inventory_adjustments` (table introduced in Phase 5 to support visit-lock consumption). Build the Inventory list, item detail with four tabs, and the Adjust form. Cover receive / writeoff / count-correction operations with the correct role gates.

**Surfaces:** All
**Dependencies:** Phase 05
**Complexity:** M

## §1 Local Schema Changes (Tauri SQLite)

No new tables. `inventory_adjustments` was created in Phase 5 (PRD §6.1.14) to enable visit-lock consumption.

### Modified tables

None.

### New enums

None (reasons enum `receive | writeoff | count_correction | consume_visit` already in place).

Migration file: `src-tauri/migrations/006_inventory_ops.sql` (no DDL; reserved for any operational indexes that surface during implementation; ships as a no-op placeholder unless needed).

## §2 Server Schema Changes (Prisma / Postgres)

No new models. `InventoryAdjustment` already in place from Phase 5.

### Modified models

None.

### New enums

None.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/inventory` | `src/pages/inventory/list.tsx` | Items list with status pills. |
| `/inventory/items/:id` | `src/pages/inventory/detail.tsx` | Overview / Consumption Map / Adjustments / Audit tabs. |
| `/inventory/adjust` | `src/pages/inventory/adjust.tsx` | Form to create one adjustment. |

Components:

| Component | File | Purpose |
|-|-|-|
| `<InventoryItemsTable>` | `src/components/inventory/items-table.tsx` | Name / Unit / On hand / Threshold / Status pill / Last adjusted. |
| `<StockStatusPill>` | `src/components/inventory/stock-status-pill.tsx` | OK / Low / Negative; color and text. |
| `<ItemOverview>` | `src/components/inventory/item-overview.tsx` | On-hand + threshold + badges. |
| `<ItemConsumptionMapTable>` | `src/components/inventory/item-consumption-map-table.tsx` | Read-only; redirect to admin for edit. |
| `<ItemAdjustmentsList>` | `src/components/inventory/item-adjustments-list.tsx` | Chronological adjustments; voided-visit reversals render as positive offsets. |
| `<ItemAuditTab>` | `src/components/inventory/item-audit-tab.tsx` | Filtered audit on `entity='inventory_items'`. |
| `<AdjustForm>` | `src/components/inventory/adjust-form.tsx` | Item picker, reason radio, delta, note. |

Zustand stores: none new.

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useInventoryItems(filter)` | `['inventory','items','list', filter]` | Items with computed status. |
| `useInventoryItem(id)` | `['inventory','items','detail', id]` | Item + joined consumption map. |
| `useInventoryAdjustments(itemId)` | `['inventory','adjustments', itemId]` | Adjustments for one item. |
| `useInventoryAdjustmentCreate` | mutation | `inventory::create_adjustment`. |
| `useInventoryItemAuditLog(itemId)` | `['inventory','audit', itemId]` | Audit filtered. |

Zod schemas:

| Schema | File |
|-|-|
| `AdjustmentInputSchema` (refines `reason` + `delta` sign rules) | `src/lib/schemas/inventory.ts` |

### Tauri / Rust

Domain entity (refined in `src-tauri/src/domains/inventory/`):

```rust
pub struct InventoryAdjustment {
  pub id: Uuid,
  pub item_id: Uuid,
  pub delta: i64,
  pub reason: AdjustmentReason,
  pub visit_id: Option<Uuid>,
  pub note: Option<String>,
  pub by_user_id: Uuid,
  pub created_at: DateTime<Utc>,
  pub entity_id: String,
}
impl InventoryAdjustment {
  pub fn try_receive(item_id: Uuid, qty: i64, by: Uuid, note: Option<String>) -> Result<Self, AppError> {
    /* qty > 0 */
  }
  pub fn try_writeoff(item_id: Uuid, qty: i64, by: Uuid, note: Option<String>) -> Result<Self, AppError> {
    /* qty > 0 stored as negative delta */
  }
  pub fn try_count_correction(item_id: Uuid, signed_delta: i64, by: Uuid, note: Option<String>) -> Result<Self, AppError> {
    /* delta != 0 */
  }
  pub fn try_consume_visit(item_id: Uuid, qty: i64, visit_id: Uuid, by: Uuid) -> Result<Self, AppError> {
    /* qty > 0 stored as negative delta */
  }
}
```

Repository trait extended:

```rust
#[async_trait]
pub trait InventoryAdjustmentRepo {
  async fn append(&self, tx: &mut Tx, adj: InventoryAdjustment) -> Result<(), AppError>;
  async fn list_for_item(&self, item_id: Uuid, page: Page) -> Result<Vec<InventoryAdjustment>, AppError>;
  async fn recompute_on_hand(&self, tx: &mut Tx, item_id: Uuid) -> Result<i64, AppError>;
}
```

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `inventory::list_items` | `{ status?: 'ok'|'low'|'neg', includeInactive?: bool }` | `InventoryItemWithStatus[]` | Reads materialized `quantity_on_hand`; computes status against `low_stock_threshold`. |
| `inventory::get_item` | `{ id }` | `{ item, consumption_map, recent_adjustments }` | Item detail. |
| `inventory::list_adjustments` | `{ itemId, limit, offset }` | `InventoryAdjustment[]` | Chronological. |
| `inventory::create_adjustment` | `AdjustmentInput` | `InventoryAdjustment` | Writes adjustment + recomputes on-hand in one tx. |
| `inventory::recompute_on_hand` | `{ itemId }` | `{ newOnHand }` | Admin/debug command; recomputes without writing an adjustment. |

Register in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity class: `InventoryAdjustment` already defined in Phase 5; no changes.

Repository interface: already defined.

Prisma repo notes: server treats all adjustment pushes as `additive-only`. The server also recomputes `inventory_items.quantityOnHand` server-side on every adjustment apply in the same transaction so that server-side reports do not require a separate recompute job.

TypeBox schemas: `InventoryAdjustmentPushSchema` already from Phase 5.

Route table:

| Method | Path | Description |
|-|-|-|
| (no new routes) | n/a | Adjustments flow through `/sync/push` and `/sync/pull`. |

## §4 Business Logic

### Frontend

`<AdjustForm>` flow per PRD §7.3.3:

1. Pick item (combobox).
2. Pick reason: radio `receive` / `writeoff` / `count_correction`.
3. Enter delta:
   - For `receive`: positive integer.
   - For `writeoff`: positive integer (UI shows it as "decrease by"; service writes negative delta).
   - For `count_correction`: signed integer; must be non-zero.
4. Optional note.
5. Submit dispatches `inventory::create_adjustment`.

Permission gates:

| Reason | Roles |
|-|-|
| `receive` | receptionist, superadmin |
| `writeoff` | receptionist, superadmin |
| `count_correction` | superadmin only |
| `consume_visit` | not selectable via UI; only the lock workflow emits these |

The UI hides the count-correction radio for non-superadmin users.

### Tauri / Rust

`InventoryAdjustmentService::create(input, current_user)`:

1. Validate role per reason (count_correction requires superadmin).
2. Construct `InventoryAdjustment` via the appropriate constructor (`try_receive` / `try_writeoff` / `try_count_correction`); reject on constructor error.
3. Open SQLite transaction:
   1. Append adjustment via `InventoryAdjustmentRepo::append`.
   2. Recompute `inventory_items.quantity_on_hand` for the affected item; bump `version`; mark `dirty=1`.
   3. Write audit rows: `create` on adjustment; `update` on item (delta covers `quantity_on_hand`).
   4. Enqueue outbox rows.
   5. Commit.
4. Return the persisted adjustment with updated item context.

### Sync Server

No new server logic; reuses Phase 5 acceptance path.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `inventory_adjustments` | `additive-only` | `op_id` | Same as Phase 5; this phase only widens the set of operations that emit adjustments. |

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

No changes.

### Audit trigger additions

None.

### Local SQLite indexes

None new (Phase 5 indexes suffice).

### Tauri capabilities

No new scopes.

### Plugin registrations

None new.

### What this phase does NOT touch

- No accounting reports (Phase 7).
- No new entities.
- No new sync contracts.
- No conflict resolver UI (Phase 8).

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover sign rules per reason, role-gated count_correction, recompute correctness, voided-visit offset rendering.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`:
   1. Navigate to `/inventory`; see items list with status pills.
   2. Open an item detail; verify Overview, Consumption Map, Adjustments, Audit tabs render.
   3. Go to `/inventory/adjust`; pick item; reason=receive; enter qty=5; submit; assert `quantity_on_hand` increases by 5 in the list.
   4. As receptionist, attempt count_correction; assert the radio is hidden / submission rejected by the IPC.
   5. As superadmin, perform count_correction; assert audit row carries the actor and delta.
   6. Verify a voided visit's offset adjustments render as positive rows in the item's Adjustments tab.
5. `cd sync-server && pnpm test`: adjustment push echo (additive replay test reused).
6. Sync round-trip: create a `receive` adjustment offline; reconnect; assert the row arrives on the server; pull verifies the server-side `quantityOnHand` matches local.
7. Concurrent adjustments: device A receives 10, device B writes off 3 of the same item simultaneously; reconnect both; assert both rows survive and `quantity_on_hand` matches the sum.
8. Negative on-hand: simulate over-consumption via lock; assert UI surfaces "NEG" pill but does not block the operation.
9. Audit: every adjustment writes one `create` audit row and one `update` audit row on the item.
10. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 8 gaps incorporated below._

### 7.1 Per-reason delta sign CHECK on `inventory_adjustments`
- **Gap:** HIGH | Missing Constraint | PRD §6.1.14 inv 2-4
- Schema enforces `consume_visit ⇒ visit_id NOT NULL` but NOT the delta-sign-by-reason rules (`receive` → `delta > 0`; `writeoff` → `delta < 0`; `count_correction` → `delta != 0`). A sync apply from a malicious or buggy device can write inconsistent rows.
- **Resolution:** Extend the local CHECK on `inventory_adjustments` (added in phase-05 §1):
  ```sql
  CHECK (
      (reason = 'receive'           AND delta > 0)
   OR (reason = 'writeoff'          AND delta < 0)
   OR (reason = 'count_correction'  AND delta != 0)
   OR (reason = 'consume_visit')
  )
  ```
  Server Prisma adds a `@@check` (raw SQL) migration with the same predicate. Phase-05 §7 referenced; phase-06 takes ownership because the operational reasons land here.

### 7.2 Quantity-recompute SQL quoted verbatim
- **Gap:** MEDIUM | Missing Service Method | research.md
- §4 says "recompute on_hand" without quoting the exact SUM-with-tombstone-filter SQL that research.md publishes.
- **Resolution:** Add to §4 `QuantityRecomputer::recompute(item_ids: &[Uuid])`:
  ```sql
  UPDATE inventory_items
  SET quantity_on_hand = (
        SELECT COALESCE(SUM(delta), 0)
        FROM inventory_adjustments
        WHERE item_id = inventory_items.id
          AND deleted_at IS NULL
      ),
      updated_at = :now,
      version = version + 1,
      dirty = 1
  WHERE id IN (:item_ids);
  ```
  Runs inside the same SQLite tx as any adjustment write.

### 7.3 Server-side recompute on adjustment push
- **Gap:** HIGH | Missing Service Method | PRD §6.1.12 + PRD §6.1.14
- §3.Server mentions server recompute in one sentence; no Prisma transaction structure, no method signature, no audit row.
- **Resolution:** Add to §3.Server services:
  ```ts
  class InventoryAdjustmentService {
    async acceptPush(rows: AdjustmentPushRow[]): Promise<void> {
      await this.prisma.$transaction(async (tx) => {
        for (const row of rows) {
          await tx.inventoryAdjustment.upsert({ where: { id: row.id }, ... });
          const sum = await tx.inventoryAdjustment.aggregate({
            where: { itemId: row.itemId, deletedAt: null },
            _sum: { delta: true },
          });
          await tx.inventoryItem.update({
            where: { id: row.itemId },
            data: { quantityOnHand: sum._sum.delta ?? 0, version: { increment: 1 } },
          });
          await this.auditService.write(tx, { entity: 'inventory_items', op: 'update', ... });
        }
      });
    }
  }
  ```

### 7.4 `inventory::recompute_on_hand` audit row
- **Gap:** MEDIUM | Missing Audit Trigger | PRD §10.4
- The debug command `inventory::recompute_on_hand` does not write an audit row.
- **Resolution:** Wrap the command body in `with_audit(action = 'update', entity = 'inventory_items', actor = superadmin_id)` writing the before/after `quantity_on_hand` per item. Restricted to superadmin via role check at the IPC layer.

### 7.5 Active/Inactive filter chip on Inventory list
- **Gap:** MEDIUM | Missing UI Element | PRD §7.3.1
- §3.Frontend list page has status pills + `includeInactive` flag but no UI filter control.
- **Resolution:** Extend `<InventoryItemsTable>` row in §3.Frontend table:
  > Filter row: status chips (`OK | LOW | NEG`), active toggle (`Active only | All`), free-text query input (debounced 250ms, min 2 chars - reuses `src/lib/search.ts` from phase-03 §7.14).

### 7.6 AdjustForm role enforcement in TypeBox / IPC
- **Gap:** MEDIUM | Missing Validation | PRD §6.1.14
- UI hides the `count_correction` radio for non-superadmin; the IPC handler trusts the request.
- **Resolution:** Add to §3.Tauri `inventory::create_adjustment` command (the IPC declared in §3 is `inventory::create_adjustment`, NOT `inventory::adjust` — Pass-1 had the wrong name):
  ```rust
  if reason == AdjustmentReason::CountCorrection
      && current_user.role != Role::Superadmin {
      return Err(AdjustmentError::Forbidden);
  }
  ```
  Server `acceptPush` also rejects `count_correction` rows whose authoring user (`byUserId`) is not a superadmin.

### 7.7 `count_correction` non-zero delta DB CHECK
- **Gap:** MEDIUM | Missing Constraint | PRD §6.1.14 inv 4
- Covered by §7.1 unified CHECK. Listed here as the cross-reference for verification: write a sync-apply test that rejects `{ reason: 'count_correction', delta: 0 }`.

### 7.8 Max-delta sanity cap (documentation)
- **Gap:** LOW | Missing Validation | PRD §6.1.14
- The plan doesn't say whether deltas have an upper bound.
- **Resolution:** Document in §4: "There is no upper bound on `delta` per row by design (medical/surgical emergencies). The UI warns when `|delta| > 1000` with 'Unusually large adjustment - confirm' but does not block."

### 7.9 Pull-time quantity recompute hook
- **Gap:** HIGH | Missing Sync Rule | PRD §6.1.12 inv 1
- Phase-03 §7.25 declares pull-time recompute as the contract; this phase owns the hook implementation.
- **Resolution:** Add to §4 a `SyncEngine::on_pull_applied_inventory` callback registered in phase-01 SyncPullService. After applying any `inventory_items` or `inventory_adjustments` rows in a pull batch, run `QuantityRecomputer::recompute(affected_item_ids)` in the SAME tx as the pull apply. Any pulled `quantity_on_hand` value is OVERWRITTEN by the local recompute (per PRD §6.1.12 sync policy). Server value is treated as informational only. Add §6 verification: pull a batch containing `inventory_adjustments` for item I with delta -5 and an `inventory_items` row with `quantity_on_hand = 999`; after apply, local row has the locally-computed sum, NOT 999.

### 7.10 Low-stock / negative-stock partial indexes
- **Gap:** MEDIUM | Missing Logic | PRD §7.3.1
- PRD §7.3.1 status pill needs LOW/NEG filter served fast. No expression-based index exists for the comparison `quantity_on_hand <= low_stock_threshold`.
- **Resolution:** Override the §1 "no DDL" note and add to a `migrations/006_inventory_indexes.sql`:
  ```sql
  CREATE INDEX inventory_items_low_stock
    ON inventory_items(entity_id)
    WHERE deleted_at IS NULL AND quantity_on_hand <= low_stock_threshold;
  CREATE INDEX inventory_items_negative
    ON inventory_items(entity_id)
    WHERE deleted_at IS NULL AND quantity_on_hand < 0;
  ```
  Inventory list status filters consume these via partial-index lookups. Mirror on server with raw SQL CREATE INDEX statements via Prisma raw migration.

### 7.11 Audit-first ordering for adjustment workflows
- **Gap:** HIGH | Wrong Order | PRD §4.3
- §4 `InventoryAdjustmentService::create` step list writes audit rows AFTER the append and recompute. Phase-01 §7.7 mandates audit-first.
- **Resolution:** Restructure §4 `InventoryAdjustmentService::create` step 3:
  ```
  3.1 with_audit start: INSERT audit_log row action='create' entity='inventory_adjustments' entity_id=new.id.
  3.2 INSERT audit_log row action='update' entity='inventory_items' entity_id=item_id, delta={ before: q_before, after: q_after, reason }.
  3.3 INSERT inventory_adjustments row.
  3.4 UPDATE inventory_items.quantity_on_hand via QuantityRecomputer (single-statement SUM).
  3.5 Enqueue outbox row(s).
  3.6 Commit tx.
  ```
  Server `acceptPush` follows the same order. Audit rows are always written first; on failure of any subsequent step the entire tx rolls back, leaving NO audit row.

### 7.12 `<InventoryItemsTable>` Pending-sync column receipt
- **Gap:** MEDIUM | Missing Handshake | phase-05 §7.29
- Phase-05 §7.29 says `<InventoryItemsTable>` (this phase) adds a Pending-sync column rendering `<DirtyDot dirty={row.dirty === 1} />`. Phase-06 §7 had no receipt.
- **Resolution:** Amend §3.Frontend `<InventoryItemsTable>` row to include the Pending-sync column. Update `inventory::list_items` response to include `dirty: boolean` on every row. Column header i18n key `inventory.columns.pending_sync`. Column is sortable but not filterable. Receipt confirms the shared `<DirtyDot>` component from phase-05 §7.29 is consumed here.

### 7.13 `/inventory/*` route role gate
- **Gap:** HIGH | Missing Role Guard | PRD navigation tree §3.1; Pass-3 GAP-E-7
- PRD restricts `/inventory/*` to `receptionist, superadmin`. No phase declared the wrapper.
- **Resolution:** Append to §3 Frontend routing block: "The `/inventory/*` outlet is wrapped in `<RequireRole roles={['receptionist','superadmin']}>` (component from phase-02 §7.8). Non-matching role redirects to `/no-access`. `<UserMenu>` hides the Inventory link based on the same role check."

### 7.14 `inventory_adjustments` per-reason CHECK as concrete server raw migration
- **Gap:** HIGH | Missing Constraint | Pass-3 GAP-C-4; §7.1
- §7.1 added the per-reason delta-sign CHECK to the LOCAL SQLite migration and stated "Server Prisma adds a `@@check` (raw SQL) migration with the same predicate" -- but `@@check` does not exist in Prisma 5/6 and no concrete raw-SQL migration was named. Without it a malicious push of `{reason:'receive', delta:-5}` lands on the server.
- **Resolution:** Add concrete raw-SQL migration `sync-server/prisma/migrations/<ts>_inventory_adjustments_delta_sign/migration.sql`:
  ```sql
  ALTER TABLE inventory_adjustments
    ADD CONSTRAINT inventory_adjustments_delta_sign CHECK (
          (reason = 'receive'          AND delta > 0)
       OR (reason = 'writeoff'         AND delta < 0)
       OR (reason = 'count_correction' AND delta != 0)
       OR (reason = 'consume_visit')
    );
  ```
  Created via `prisma migrate dev --create-only`. Server `InventoryAdjustmentService::acceptPush` additionally TypeBox-validates each branch before insert (defense in depth; the CHECK is the source-of-truth invariant). Cross-reference phase-05 §7.51 raw-SQL ordering rule.

### 7.15 `<ItemDetailTabs>` container + Adjust form action bar + reversal badge
- **Gap:** LOW | Missing UI Element | PRD §7.3.2 lines 1834-1839, §7.3.3 lines 1841-1845; Pass-3 GAP-E-13, E-14
- PRD §7.3.2 declares 4 tabs (Overview, Consumption Map, Adjustments, Audit); §3 declares 4 components but no container or per-tab i18n keys. PRD §7.3.3 Adjust form lacks explicit submit/cancel buttons; voided-visit reversal rendering convention is unspecified.
- **Resolution:** Add to §3 Frontend components:
  - `<ItemDetailTabs>` -- container orchestrating tab switching with i18n keys `inventory.item.tabs.{overview,consumption_map,adjustments,audit}`.
  - `<ItemAdjustmentsList>` extension: voided-visit reversal rows render with `<Badge variant="reversal">` and tooltip linking back to the voided visit at `/reception/visits/:id` (read-only mode per phase-05 §7.24).
  - `<AdjustForm>` extension: action bar with `[Save adjustment]` (primary, dispatches IPC) and `[Cancel]` (back to `/inventory`). For `count_correction` reason: a single signed `<Input type="number">` allowing negatives; helper text `inventory.adjust.helper.count_correction_signed` ("Negative values reduce stock").
