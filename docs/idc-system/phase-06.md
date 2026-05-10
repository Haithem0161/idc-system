# Phase 6: Inventory & Auto-Decrement

**Goal:** Land the three inventory tables (`inventory_items`, `inventory_consumption_map`, `inventory_adjustments`), the auto-decrement extension to the Phase-5 lock workflow, and the Inventory module UI (list + detail + manual adjust).

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 5.
**Complexity:** L
**PRD references:** §4.4 (inventory consumption ledger), §6.1.12-§6.1.14 (inventory entities), §7.3 (Inventory module), §8.1 (lock workflow extension).
**Decisions consumed:** D-008 (auto-decrement + ledger), D-016 (sync policies), D-025 (receptionist permissions on adjustments).

---

## Section 1: Local Schema Changes (Tauri SQLite)

### Migration `014_inventory_items.sql` (PRD §6.1.12)

```sql
-- Verbatim from PRD §6.1.12. Bilingual `name_ar` (required) + `name_en` (optional)
-- mirror `check_types`/`check_subtypes` per PRD §10.6. No `notes` column.
CREATE TABLE IF NOT EXISTS inventory_items (
  id                    TEXT PRIMARY KEY,
  name_ar               TEXT NOT NULL,
  name_en               TEXT NULL,
  unit                  TEXT NOT NULL,                    -- 'ml', 'box', 'each', etc.
  quantity_on_hand      INTEGER NOT NULL DEFAULT 0,       -- materialized; recomputed from adjustments
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
CREATE INDEX inventory_items_low_stock
  ON inventory_items(entity_id, is_active)
  WHERE deleted_at IS NULL AND quantity_on_hand <= low_stock_threshold;
```

### Migration `015_inventory_consumption_map.sql` (PRD §6.1.13)

```sql
CREATE TABLE IF NOT EXISTS inventory_consumption_map (
  id                    TEXT PRIMARY KEY,
  check_type_id         TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id      TEXT NULL REFERENCES check_subtypes(id),
  item_id               TEXT NOT NULL REFERENCES inventory_items(id),
  quantity_per_check    INTEGER NOT NULL CHECK (quantity_per_check > 0),
  on_dye_only           INTEGER NOT NULL DEFAULT 0 CHECK (on_dye_only IN (0,1)),
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  deleted_at            TEXT NULL,
  version               INTEGER NOT NULL DEFAULT 0,
  dirty                 INTEGER NOT NULL DEFAULT 1,
  last_synced_at        TEXT NULL,
  origin_device_id      TEXT NULL,
  entity_id             TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_consumption_map_unique
  ON inventory_consumption_map(check_type_id, IFNULL(check_subtype_id,''), item_id, on_dye_only)
  WHERE deleted_at IS NULL;
```

### Migration `016_inventory_adjustments.sql` (PRD §6.1.14)

```sql
CREATE TABLE IF NOT EXISTS inventory_adjustments (
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

---

## Section 2: Server Schema Changes (Prisma / Postgres)

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

  consumptionMap      InventoryConsumptionMap[]
  adjustments         InventoryAdjustment[]

  @@map("inventory_items")
}

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

  checkType    CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])
  item         InventoryItem @relation(fields: [itemId], references: [id])

  @@unique([checkTypeId, checkSubtypeId, itemId, onDyeOnly])
  @@map("inventory_consumption_map")
}

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

  item   InventoryItem @relation(fields: [itemId], references: [id])
  visit  Visit?        @relation(fields: [visitId], references: [id])
  byUser User          @relation(fields: [byUserId], references: [id])

  @@map("inventory_adjustments")
}

enum AdjustmentReason { receive writeoff count_correction consume_visit }
```

### Existing models updated

The following Prisma models from earlier phases gain back-relations in this phase. These are append-only line additions; `prisma db push` validates the schema after they land alongside the three new models above. Apply each addition in the same migration as the new models so validation never fails mid-deploy.

```prisma
// On CheckType (originally at phase-03.md §2):
model CheckType {
  // ... existing fields and relations ...
  consumptionMap InventoryConsumptionMap[]   // ADDED in P6
}

// On CheckSubtype (originally at phase-03.md §2):
model CheckSubtype {
  // ... existing fields and relations ...
  consumptionMap InventoryConsumptionMap[]   // ADDED in P6
}

// On Visit (originally at phase-05.md §2):
model Visit {
  // ... existing fields and relations ...
  inventoryAdjustments InventoryAdjustment[] // ADDED in P6
}

// On User (originally at phase-02.md §2):
model User {
  // ... existing fields and relations ...
  inventoryAdjustments InventoryAdjustment[] // ADDED in P6 (matches `InventoryAdjustment.byUser`)
}
```

These four lines are the only edits to existing models. No fields change; no constraints change; no indexes change.

---

## Section 3: DDD Implementation

### Frontend (React)

#### New routes (`/inventory/*`)

| Path | File | Description |
|-|-|-|
| `/inventory` | `src/pages/inventory/list.tsx` | Items list with status pills (OK / LOW / NEG). |
| `/inventory/items/:id` | `src/pages/inventory/detail.tsx` | Tabs: Overview / Consumption Map / Adjustments / Audit. |
| `/inventory/adjust` | `src/pages/inventory/adjust.tsx` | Form: pick item, reason, delta, note. |

Plus the `/admin/inventory` placeholder from P3 lights up:
- `/admin/inventory/items` — same list as `/inventory` but with edit affordances.
- `/admin/inventory/items/:id` — edit name, unit, threshold.
- `/admin/inventory/consumption-map` — manage rows.

#### React Query hooks
- `useInventoryItems(filter)` — list with status.
- `useInventoryItem(id)` — detail with adjustment history.
- `useConsumptionMap(checkTypeId)` — for admin editor.
- `useCreateAdjustment()`, `useReceiveStock()`, `useWriteoff()`, `useCountCorrection()` (last is superadmin-only).

#### Zod schemas
`inventory-item.ts`, `inventory-consumption-map.ts`, `inventory-adjustment.ts`.

#### i18n
`inventory.json` namespace populated (~80 keys).

#### Sidebar low-stock badge
`src/components/shell/Sidebar.tsx` reads a `useLowStockCount()` query (key `['inventory', 'low-stock-count']`); shows a red dot + count next to the Inventory item when > 0.

### Tauri/Rust

#### Domains added
- `inventory_items/`
- `inventory_consumption_map/`
- `inventory_adjustments/`

Each follows the standard DDD layout from earlier phases.

#### Tauri commands

| Command | Args | Returns | Description |
|-|-|-|-|
| `inventory_items_list` | `{ filter: ItemsFilter }` | `Vec<ItemRow>` | List with status. |
| `inventory_items_get` | `{ id: Uuid }` | `ItemDetail` | Detail with recent adjustments. |
| `inventory_items_upsert` | `{ id?: Uuid, payload: ItemWrite }` | `Uuid` | Create/edit (admin). |
| `inventory_items_delete` | `{ id: Uuid }` | `()` | Soft-delete. Blocked if non-zero `quantity_on_hand`. |
| `inventory_consumption_map_for_check` | `{ check_type_id: Uuid, check_subtype_id?: Uuid }` | `Vec<MapRow>` | For lock workflow. |
| `inventory_consumption_map_upsert` | `{ id?: Uuid, payload: MapWrite }` | `Uuid` | Admin editor. |
| `inventory_consumption_map_delete` | `{ id: Uuid }` | `()` | Soft-delete. |
| `inventory_adjustments_create` | `{ item_id: Uuid, reason: Reason, delta: i64, note?: String }` | `Uuid` | Receive / writeoff / count_correction. |
| `inventory_adjustments_list_for_item` | `{ item_id: Uuid, limit: i64 }` | `Vec<AdjustmentRow>` | Item detail tab. |
| `inventory_low_stock_count` | `()` | `i64` | Sidebar badge. |

10 IPC commands.

### Sync Server (Fastify)

Domain `inventory/`. Routes:

| Method | Path | Description |
|-|-|-|
| `GET` | `/inventory/items` | List. |
| `GET` | `/inventory/items/:id` | Detail. |
| `POST` | `/inventory/items` | Create. |
| `PATCH` | `/inventory/items/:id` | Update. |
| `DELETE` | `/inventory/items/:id` | Soft-delete. |
| `GET` | `/inventory/consumption-map` | List rows. |
| `POST` | `/inventory/consumption-map` | Create. |
| `PATCH` | `/inventory/consumption-map/:id` | Update. |
| `DELETE` | `/inventory/consumption-map/:id` | Soft-delete. |
| `POST` | `/inventory/adjustments` | Append (receive/writeoff/count_correction; consume_visit only via sync push). |
| `GET` | `/inventory/adjustments` | List with filters. |

11 routes.

`/sync/push` registry adds `inventory_items` (LWW), `inventory_consumption_map` (LWW), `inventory_adjustments` (additive-only).

---

## Section 4: Business Logic

### `InventoryService`

File: `src-tauri/src/domains/inventory/services/inventory_service.rs`.

#### `consume_for_visit(tx, visit) -> Result<Vec<InventoryAdjustment>, AppError>`

Step sequence (called from `VisitService::lock` step 6):

1. SELECT `inventory_consumption_map` rows where `(check_type_id = visit.check_type_id AND COALESCE(check_subtype_id, '') = COALESCE(visit.check_subtype_id, ''))` AND `(on_dye_only = 0 OR visit.dye = 1)` AND `deleted_at IS NULL`.
2. For each row, build an `InventoryAdjustment { id: uuid_v7(), item_id, delta: -row.quantity_per_check, reason: 'consume_visit', visit_id: visit.id, by_user_id: visit.receptionist_user_id }`.
3. `repo.upsert(tx, &adjustment)` for each.
4. Recompute `quantity_on_hand` for each affected `item_id`: `UPDATE inventory_items SET quantity_on_hand = (SELECT COALESCE(SUM(delta), 0) FROM inventory_adjustments WHERE item_id = ? AND deleted_at IS NULL), updated_at = ?, version = version + 1, dirty = 1 WHERE id = ?`.
5. Return the list of adjustments (caller emits one outbox op per row plus one for each item).

#### `adjust(item_id, reason, delta, note, actor, role) -> Result<InventoryAdjustment, AppError>`

Manual adjust step sequence:

1. Permission check:
   - `receive` → any authenticated role.
   - `writeoff` → any authenticated role.
   - `count_correction` → superadmin only (D-025).
   - `consume_visit` → forbidden via this command (only `consume_for_visit` may write `consume_visit` rows).
2. Validate sign-of-delta vs reason:
   - `receive`: `delta > 0`.
   - `writeoff`: `delta < 0`.
   - `count_correction`: `delta != 0`.
3. `with_audit(action='create', entity='inventory_adjustments') { repo.upsert(tx, &adj); item_repo.recompute_quantity(tx, item_id) }`.
4. Outbox enqueue.

#### `recompute_quantity_on_hand(item_id)`

Pure function over `inventory_adjustments`; called from both `consume_for_visit` and `adjust` plus from a one-off "recompute all" admin action used after restore.

### Sync semantics

| Entity | Policy |
|-|-|
| `inventory_items` | LWW |
| `inventory_consumption_map` | LWW |
| `inventory_adjustments` | additive-only |

`inventory_items.quantity_on_hand` is materialized; the materialization is recomputed locally on every adjustment, so concurrent adjustments from two devices are handled by the additive-only ledger and the eventually-consistent recompute. No conflict surface for inventory.

### Phase-5 lock-workflow integration

`VisitService::lock` step 6 (previously `// TODO P6`) is now:

```rust
let adjustments = inventory_service.consume_for_visit(&mut tx, &visit).await?;
for adj in &adjustments {
    audit_repo.append(
        &mut tx,
        &AuditEvent::create_event(
            actor, device_id, "inventory_adjustments", adj.id,
            json!({ "delta": adj.delta, "reason": adj.reason, "visit_id": adj.visit_id })
        )
    ).await?;
}
```

The lock transaction now writes: 1 visit row + N adjustment rows + N item recomputes + audit row per change. All in one transaction.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
Append `InventoryItem`, `InventoryConsumptionMap`, `InventoryAdjustment`. **TENANT_MODELS at end of P6 = 15** (matches PRD §6.1 final inventory).

### Audit triggers
None.

### Local SQLite indexes
Listed under each migration.

### Capabilities / plugins
No additions.

---

## Section 6: Verification

1. Lint / build / test pass on all surfaces.
2. **Migrations apply.** Three new tables present; `quantity_on_hand` defaults to 0.
3. **Seed an item** "Iodine contrast 100ml" with threshold 10. **Seed consumption map row** for `(check_type_id = "مفراس", check_subtype_id = "with dye", item_id, quantity_per_check = 1, on_dye_only = 1)`.
4. **Receive 20.** Sidebar badge clears (was empty); `quantity_on_hand` = 20.
5. **Lock a "مفراس" visit with `dye = 1`.** `quantity_on_hand` decrements to 19 atomically with the lock; one `consume_visit` adjustment row exists; audit log has both the visit lock and the adjustment.
6. **Lock a "مفراس" visit with `dye = 0`.** `quantity_on_hand` UNchanged (`on_dye_only = 1` filtered).
7. **Manual writeoff −5.** `quantity_on_hand` → 14; audit row.
8. **Receptionist tries `count_correction`.** Rejected with `Forbidden`.
9. **Superadmin count_correction +2.** `quantity_on_hand` → 16; audit row.
10. **Low-stock badge.** Threshold 20; current 16; sidebar badge shows red dot.
11. **Sync round-trip** for items, consumption map, adjustments. `consume_visit` rows ride the same outbox as their parent visit.
12. **Conflict scenario.** Edit same `inventory_consumption_map` row on two devices; second push wins LWW; the loser's edit is visible in audit.
13. **Recompute consistency.** Run a "recompute all" admin command; `quantity_on_hand` for every item equals `SUM(delta)` of its non-deleted adjustments.
14. **i18n + RTL** verified.
15. **Pre-push composite** as before.

### What this phase does NOT verify
- Void rollback (P8 — at that point the offsetting `consume_visit` rows are exercised).
- Server-side reports (P7).
- Audit page UI (P9).

### Summary update
Bump `status.md` row 6 to `Completed`; record 3 local + 3 server tables, 10 IPC, 11 routes, 3 services. Add inventory routes + hooks + namespace to `frontend-summary.md`. Update sidebar component note (low-stock badge live).

---

## Section 7: PRD Gap Additions

### 7.1 Status pill enumeration — LOW
**Gap:** PRD §7.3.1 specifies the items list status pill values as `OK | LOW | NEG`. Phase 6 §3 mentions "status pills" but doesn't enumerate the cutoffs.
**Category:** Missing Logic.
**Remediation:** In `inventory_items_list` service:
```rust
pub enum StockStatus { Ok, Low, Negative }

fn status(quantity_on_hand: i64, threshold: i64) -> StockStatus {
    if quantity_on_hand < 0 { StockStatus::Negative }
    else if quantity_on_hand <= threshold { StockStatus::Low }
    else { StockStatus::Ok }
}
```
- `OK` → green badge; `LOW` → yellow; `NEG` → red (a negative balance indicates ledger drift, e.g. failed restore).
- i18n keys: `inventory.status.ok`, `inventory.status.low`, `inventory.status.negative`.

### 7.2 Concurrent consumption from two devices — LOW
**Gap:** Two receptionists can lock visits concurrently on different devices, each consuming the same item. Phase 6 §4 says inventory is "additive-only" so no conflicts; but the materialized `quantity_on_hand` is recomputed locally and the two devices may disagree until pulls catch up.
**Category:** Missing Integration.
**Remediation:** Document in Phase 6 §4 explicitly:
- `quantity_on_hand` is **eventually consistent** across devices.
- The authoritative value is `SUM(delta) FROM inventory_adjustments WHERE item_id = ? AND deleted_at IS NULL`.
- After every pull, `StockMaterializer::recompute_for_pulled_items` walks the affected items and rewrites `quantity_on_hand`.
- The low-stock badge can briefly disagree between devices; UX is acceptable as the badge is informational, not a hard cap.
