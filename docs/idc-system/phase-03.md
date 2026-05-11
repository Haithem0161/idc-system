# Phase 3: Catalog & Reference Data

**Goal:** Ship the eight reference-data entities that drive the visit form and the Admin module shell with sub-sidebar plus list+detail pages for each.

**Surfaces:** All
**Dependencies:** Phase 02
**Complexity:** XL

## §1 Local Schema Changes (Tauri SQLite)

Migration file: `src-tauri/migrations/003_catalog.sql`.

### check_types (PRD §6.1.2)

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

### check_subtypes (PRD §6.1.3)

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

### doctors (PRD §6.1.4) + FTS

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

CREATE TRIGGER doctors_ai AFTER INSERT ON doctors BEGIN
  INSERT INTO doctors_fts(rowid, name, specialty) VALUES (new.rowid, new.name, new.specialty);
END;
CREATE TRIGGER doctors_ad AFTER DELETE ON doctors BEGIN
  INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty) VALUES('delete', old.rowid, old.name, old.specialty);
END;
CREATE TRIGGER doctors_au AFTER UPDATE ON doctors BEGIN
  INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty) VALUES('delete', old.rowid, old.name, old.specialty);
  INSERT INTO doctors_fts(rowid, name, specialty) VALUES (new.rowid, new.name, new.specialty);
END;
```

### doctor_check_pricing (PRD §6.1.5)

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

### operators (PRD §6.1.6)

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

### operator_specialties (PRD §6.1.7)

```sql
CREATE TABLE operator_specialties (
  id                TEXT PRIMARY KEY,
  operator_id       TEXT NOT NULL REFERENCES operators(id),
  check_type_id     TEXT NOT NULL REFERENCES check_types(id),
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

### inventory_items (PRD §6.1.12)

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

### inventory_consumption_map (PRD §6.1.13)

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

### Modified tables

None.

### New enums

`doctor_check_pricing.cut_kind CHECK IN ('pct','fixed')`.

## §2 Server Schema Changes (Prisma / Postgres)

Adds models per PRD §6.1.2-§6.1.7, §6.1.12, §6.1.13:

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
  @@map("check_types")
}

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
  @@map("check_subtypes")
}

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
  @@index([entityId, name])
  @@map("doctors")
}

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

enum CutKind { pct fixed }

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
  @@map("operators")
}

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

  checkType     CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype  CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])
  item          InventoryItem @relation(fields: [itemId], references: [id])

  @@unique([checkTypeId, checkSubtypeId, itemId, onDyeOnly])
  @@map("inventory_consumption_map")
}
```

### New enums

`CutKind { pct, fixed }`.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/admin/check-types` | `src/pages/admin/check-types/list.tsx` | List with sort_order drag handle. |
| `/admin/check-types/:id` | `src/pages/admin/check-types/detail.tsx` | Detail with subtypes inline table; `has_subtypes` XOR UI. |
| `/admin/doctors` | `src/pages/admin/doctors/list.tsx` | FTS search bar over `doctors_fts`. |
| `/admin/doctors/:id` | `src/pages/admin/doctors/detail.tsx` | Doctor edit + `doctor_check_pricing` table editor. |
| `/admin/operators` | `src/pages/admin/operators/list.tsx` | Operator list with base cut column. |
| `/admin/operators/:id` | `src/pages/admin/operators/detail.tsx` | Operator edit + specialties picker. |
| `/admin/inventory` | `src/pages/admin/inventory/list.tsx` | Catalog list. |
| `/admin/inventory/:id` | `src/pages/admin/inventory/detail.tsx` | Item edit + consumption map editor. |

Admin shell with macOS-style sub-sidebar (per PRD §3.3):

| Component | File | Purpose |
|-|-|-|
| `<AdminShell>` | `src/components/admin/admin-shell.tsx` | Sub-sidebar listing the eight admin sub-pages (plus Users from Phase 2 and Settings from Phase 2 and Audit from Phase 8). |
| `<HasSubtypesToggle>` | `src/components/admin/has-subtypes-toggle.tsx` | Enforces the XOR rule from PRD §6.1.2. |
| `<DoctorPricingEditor>` | `src/components/admin/doctor-pricing-editor.tsx` | Per-(check_type, subtype?) row editor. |
| `<OperatorSpecialtyPicker>` | `src/components/admin/operator-specialty-picker.tsx` | Multi-select check types. |
| `<ConsumptionMapEditor>` | `src/components/admin/consumption-map-editor.tsx` | Item + qty + on_dye_only per (check_type, subtype?). |

Zustand stores:

| Store | File | State |
|-|-|-|
| `useAdminNavStore` | `src/stores/admin-nav-store.ts` | Active sub-page within `/admin/*`. |

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useCheckTypesList` | `['catalog','checkTypes','list']` | List of check types with subtypes counts. |
| `useCheckType(id)` | `['catalog','checkTypes', id]` | Single type. |
| `useCheckSubtypesByType(typeId)` | `['catalog','checkSubtypes', typeId]` | Subtypes for one type. |
| `useDoctorsList` | `['catalog','doctors','list']` | Doctors with FTS query support. |
| `useDoctor(id)` | `['catalog','doctors', id]` | Doctor + pricing rows. |
| `useOperatorsList` | `['catalog','operators','list']` | Operators with specialties count. |
| `useOperator(id)` | `['catalog','operators', id]` | Operator + specialties. |
| `useInventoryCatalog` | `['catalog','inventory','list']` | Items. |
| `useInventoryItem(id)` | `['catalog','inventory', id]` | Item + consumption rows. |
| Mutations | `*Create`, `*Update`, `*SoftDelete` per entity | IPC bindings. |

Zod schemas (in `src/lib/schemas/`):

| Schema | File |
|-|-|
| `CheckTypeSchema`, `CheckTypeCreateSchema` (XOR rule via refinement) | `src/lib/schemas/check-type.ts` |
| `CheckSubtypeSchema` | `src/lib/schemas/check-subtype.ts` |
| `DoctorSchema`, `DoctorPricingSchema` (`cut_kind` constraint refinement) | `src/lib/schemas/doctor.ts` |
| `OperatorSchema`, `OperatorSpecialtySchema` | `src/lib/schemas/operator.ts` |
| `InventoryItemSchema`, `InventoryConsumptionMapSchema` | `src/lib/schemas/inventory.ts` |

### Tauri / Rust

Domain entities (per surface, in `src-tauri/src/domains/catalog/`):

```rust
pub struct CheckType { ... }
impl CheckType {
  pub fn try_new_flat(name_ar: &str, base_price_iqd: i64, ...) -> Result<Self, AppError> { /* XOR enforced */ }
  pub fn try_new_subtyped(name_ar: &str, ...) -> Result<Self, AppError> { /* base_price = None */ }
  pub fn toggle_to_subtyped(self) -> Result<Self, AppError> { /* base_price must be None */ }
  pub fn toggle_to_flat(self, price: i64, has_subtypes_rows: bool) -> Result<Self, AppError> { /* err if rows exist */ }
}

pub struct CheckSubtype { ... }
pub struct Doctor { ... }
pub struct DoctorCheckPricing { ... }
pub enum CutKind { Pct, Fixed }
pub struct Operator { ... }
pub struct OperatorSpecialty { ... }
pub struct InventoryItem { ... }
pub struct InventoryConsumptionMap { ... }
```

Repository traits: standard CRUD shape with `tx: &mut Tx` parameter on every write. Reads cached via `sqlx`'s prepared-statement cache.

SQLite repo notes:

- FTS triggers attached to `doctors`; the repo never writes to `doctors_fts` directly.
- `inventory_consumption_map` writes always check the parent `check_types.has_subtypes` invariant in the service layer.

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `check_types::list` | `{ includeDeleted?: bool }` | `CheckType[]` | List. |
| `check_types::get` | `{ id }` | `CheckType` | One. |
| `check_types::create` | `CheckTypeCreateInput` | `CheckType` | XOR-rule enforced. |
| `check_types::update` | `CheckTypeUpdateInput` | `CheckType` | |
| `check_types::soft_delete` | `{ id }` | `()` | Blocked if referenced by non-deleted child rows. |
| `check_subtypes::list_by_type` | `{ typeId }` | `CheckSubtype[]` | |
| `check_subtypes::create` | `CheckSubtypeCreateInput` | `CheckSubtype` | |
| `check_subtypes::update` | `CheckSubtypeUpdateInput` | `CheckSubtype` | |
| `check_subtypes::soft_delete` | `{ id }` | `()` | |
| `doctors::list` | `{ query?: string, includeInactive?: bool }` | `Doctor[]` | FTS via `doctors_fts MATCH :query`. |
| `doctors::get` | `{ id }` | `{ doctor, pricings }` | |
| `doctors::create` | `DoctorCreateInput` | `Doctor` | |
| `doctors::update` | `DoctorUpdateInput` | `Doctor` | |
| `doctors::soft_delete` | `{ id }` | `()` | Cascades soft-delete on pricings. |
| `doctor_pricing::upsert` | `DoctorPricingInput` | `DoctorCheckPricing` | Subtype required when parent has subtypes. |
| `doctor_pricing::soft_delete` | `{ id }` | `()` | |
| `operators::list` | `{ includeInactive?: bool }` | `Operator[]` | |
| `operators::get` | `{ id }` | `{ operator, specialties }` | |
| `operators::create` | `OperatorCreateInput` | `Operator` | |
| `operators::update` | `OperatorUpdateInput` | `Operator` | |
| `operators::soft_delete` | `{ id }` | `()` | Blocked if open shifts exist (enforced from Phase 4). |
| `operator_specialties::upsert` | `OperatorSpecialtyInput` | `OperatorSpecialty` | |
| `operator_specialties::soft_delete` | `{ id }` | `()` | |
| `inventory_catalog::list` | `{ includeInactive?: bool }` | `InventoryItem[]` | |
| `inventory_catalog::get` | `{ id }` | `{ item, consumption }` | |
| `inventory_catalog::create` | `InventoryItemCreateInput` | `InventoryItem` | |
| `inventory_catalog::update` | `InventoryItemUpdateInput` | `InventoryItem` | |
| `inventory_catalog::soft_delete` | `{ id }` | `()` | |
| `inventory_consumption::upsert` | `InventoryConsumptionMapInput` | `InventoryConsumptionMap` | |
| `inventory_consumption::soft_delete` | `{ id }` | `()` | |

Register all in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity classes: one per Prisma model with `static create()` validators and `toResponse()` shapers. Repository interfaces: standard CRUD with `tenantId` injection. Prisma repos use `where: { entityId: tenantId, deletedAt: null }` by default. TypeBox schemas: one bundle per entity (`CheckTypeResponseSchema`, `CheckTypeCreateBodySchema`, etc.) for use only by `/sync/push` payload validation; this phase introduces zero dedicated REST endpoints.

Route table:

| Method | Path | Description |
|-|-|-|
| (no new routes) | n/a | Catalog flows entirely through `/sync/push` and `/sync/pull` from Phase 1. |

## §4 Business Logic

### Frontend

`<HasSubtypesToggle>`:

1. When user flips `has_subtypes` from 0 to 1, prompt: "Setting subtypes mode clears the flat price. Continue?"
2. On confirm, set `base_price_iqd = null` and save.
3. When user flips 1 to 0 while subtypes exist, block with toast "Soft-delete all subtypes first."

`<DoctorPricingEditor>`:

1. Render one row per `(check_type, check_subtype?)` pair where the doctor has a pricing row.
2. "+ Add row" button opens a dialog: pick check type; if `has_subtypes = 1`, force subtype picker; set `cut_kind` (radio `pct` vs `fixed`); set `cut_value`; optional `price_override_iqd`.
3. Validate `cut_kind = 'pct' AND cut_value <= 100` on submit.

`<OperatorSpecialtyPicker>`:

1. Multi-select shadcn `<Combobox>` over active `check_types`.
2. On save, diffs against current `operator_specialties` rows; emits `upsert` for new and `soft_delete` for removed.

`<ConsumptionMapEditor>`:

1. Row layout: pick item, qty, `on_dye_only` toggle.
2. Validates the `on_dye_only` toggle is only enabled when the parent check type has `dye_supported = 1`.

### Tauri / Rust

`CheckTypeService::soft_delete(id)`:

1. Verify no non-deleted `check_subtypes`, `doctor_check_pricing`, `operator_specialties`, `inventory_consumption_map` rows reference this id; else return `DeleteBlocked::Referenced`.
2. `with_audit(action='soft_delete', entity='check_types', entity_id=id)`.

`DoctorService::soft_delete(id)`:

1. `with_audit(action='soft_delete', entity='doctors', entity_id=id)`.
2. Cascade soft-delete to `doctor_check_pricing` rows for this doctor inside the same transaction (each cascade emits its own audit row).

`DoctorPricingService::upsert(input)`:

1. Load parent `check_types`; if `has_subtypes = 1`, require `check_subtype_id`; else require null.
2. Validate `cut_kind` and `cut_value` constraints.
3. Upsert by `(doctor_id, check_type_id, check_subtype_id)` unique; `with_audit`.

`OperatorService::soft_delete(id)`:

1. From Phase 4: block if any open `operator_shifts` exists. In Phase 3 the open-shift block is a placeholder; Phase 4 hardens it.
2. `with_audit(action='soft_delete', entity='operators', entity_id=id)`.

`InventoryConsumptionMapService::upsert(input)`:

1. Validate parent `check_types`-`check_subtypes` consistency.
2. Validate `quantity_per_check > 0`.
3. Validate `on_dye_only` against `check_types.dye_supported`.
4. Upsert by `(check_type_id, check_subtype_id, item_id, on_dye_only)` unique; `with_audit`.

### Sync Server

`SyncPushService` per-entity branches added for each of the eight entities. Each validates against the corresponding TypeBox schema, runs `last-write-wins` (every entity in this phase has policy `last-write-wins`), and writes the row with `entityIdTenant = tenantId`.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `check_types` | `last-write-wins` | `op_id` | Low edit frequency. |
| `check_subtypes` | `last-write-wins` | `op_id` | |
| `doctors` | `last-write-wins` | `op_id` | |
| `doctor_check_pricing` | `last-write-wins` | `op_id` | |
| `operators` | `last-write-wins` | `op_id` | |
| `operator_specialties` | `last-write-wins` | `op_id` | |
| `inventory_items` | `last-write-wins` | `op_id` | `quantity_on_hand` is informational over sync; recomputed locally from adjustments (which arrive in Phase 5). |
| `inventory_consumption_map` | `last-write-wins` | `op_id` | |

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

```ts
export const TENANT_MODELS = [
  'audit_log', 'users', 'settings',
  'check_types', 'check_subtypes',
  'doctors', 'doctor_check_pricing',
  'operators', 'operator_specialties',
  'inventory_items', 'inventory_consumption_map',
] as const;
```

### Audit trigger additions

None (audit via `with_audit`).

### Local SQLite indexes

Listed inline above with each `CREATE TABLE` block.

### Tauri capabilities

No new capability scopes.

### Plugin registrations

No new Tauri plugins. No new Fastify plugins.

### What this phase does NOT touch

- No `operator_shifts` table (Phase 4).
- No `patients`, `visits`, `inventory_adjustments` (Phase 5).
- No reports.
- No Receipts.

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover XOR rule on `check_types`, doctor pricing uniqueness, operator-specialty uniqueness, consumption-map invariant.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`: Admin shell renders with sub-sidebar; create a check type with subtypes; toggle XOR; verify error toasts; FTS search returns matching doctors as user types in ar.
5. `cd sync-server && pnpm test`: per-entity push acceptance test using `/sync/push` payloads; pull returns the same rows.
6. Sync round-trip: create a doctor and a pricing row offline; reconnect; assert both appear on server. Create a different operator on the server; pull; assert it lands locally.
7. Conflict: edit the same doctor row on two clients (LWW); reconnect both; assert the later `updated_at` wins; verify origin_device_id tiebreak path with a synthetic same-timestamp test.
8. RTL: switch language to ar; verify all admin pages mirror correctly; chevrons rotate.
9. i18n: zero literal strings in admin pages (`grep -RnE '[؀-ۿ]' src/pages/admin` returns no JSX matches).
10. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 18 gaps incorporated below._

### 7.1 `check_types` toggle invariants enforced in service layer
- **Gap:** HIGH | Missing Business Rule | PRD §6.1.2 inv 2 + 3
- §4 enforces XOR via UI but not service: invariant 2 (toggle `has_subtypes` 0→1 requires `base_price_iqd = NULL` atomically) and invariant 3 (toggle 1→0 blocked when non-deleted subtypes exist).
- **Resolution:** Add `CheckTypeService::update(id, fields)` and `CheckTypeService::toggle_has_subtypes(id, new_value)` to §4:
  ```
  toggle_has_subtypes(id, to: true):
      tx.execute("UPDATE check_types SET has_subtypes = 1, base_price_iqd = NULL ... WHERE id = ?");
      with_audit("update");
  toggle_has_subtypes(id, to: false):
      let live = tx.query("SELECT count(*) FROM check_subtypes WHERE check_type_id = ? AND deleted_at IS NULL");
      if live > 0 { return Err(CheckTypeError::SubtypesExist); }
      tx.execute("UPDATE check_types SET has_subtypes = 0 ... WHERE id = ?");
      with_audit("update");
  ```
  Server-side `SyncPushService` re-runs the same checks before accepting.

### 7.2 `check_subtypes` parent-state guard
- **Gap:** HIGH | Missing Business Rule | PRD §6.1.3 inv 1
- §4 has no `CheckSubtypeService::create` / `update` spec; PRD requires verifying parent `check_types.has_subtypes = 1` at every write.
- **Resolution:** Add `CheckSubtypeService::create / update / soft_delete` to §4. Each method opens a tx and runs:
  ```
  SELECT has_subtypes FROM check_types WHERE id = ? AND deleted_at IS NULL;
  if row.has_subtypes != 1 { return Err(CheckSubtypeError::ParentNotSubtyped); }
  ```
  before mutating. `soft_delete` is a placeholder for the visit-reference check that lands in phase-05 §7.

### 7.3 Explicit service methods for catalog CRUD
- **Gap:** MEDIUM | Missing Service Method | PRD §6.1.2-§6.1.7
- §4 documents `DoctorService::soft_delete`, `OperatorService::soft_delete` (placeholder), `DoctorPricingService::upsert`, but no `create` / `update` for `CheckType`, `CheckSubtype`, `Doctor`, `Operator`, `OperatorSpecialty`, `InventoryItem`, `InventoryConsumptionMap`.
- **Resolution:** Add one numbered step-list per service method in §4. Each follows the template:
  1. Validate inputs via the domain entity's `try_new` / `update_fields`.
  2. Lowercase / trim normalized fields where the entity mandates it.
  3. `with_audit` (action = `create` or `update`).
  4. Bump `version`, set `dirty = 1`.
  5. Enqueue outbox row.
  Server-side counterparts apply the same validation pipeline before persisting through Prisma.

### 7.4 Field non-empty validation across catalog entities
- **Gap:** MEDIUM | Missing Validation | PRD §6.1.4 inv 1, §6.1.6 inv 1
- `Doctor::try_new` and `Operator::try_new` are referenced but the non-empty-after-trim rule is not asserted in code.
- **Resolution:** Domain layer: `Doctor::try_new(name, ...)` returns `Err(DoctorError::NameEmpty)` when `name.trim().is_empty()`. Same for `Operator::try_new` and `InventoryItem::try_new` (`name_ar.trim().is_empty()` → error). Re-state in §3.Tauri domain blocks.

### 7.5 `unit` non-empty CHECK on `inventory_items`
- **Gap:** LOW | Missing Constraint | PRD §6.1.12
- Local schema declares `unit TEXT NOT NULL` without a length CHECK. Empty strings slip through.
- **Resolution:** Update §1 migration:
  ```sql
  unit TEXT NOT NULL CHECK (length(trim(unit)) > 0),
  ```
  Mirror in Prisma with a TypeBox validator on push acceptance.

### 7.6 `doctor_check_pricing` server-side invariant enforcement
- **Gap:** MEDIUM | Missing Validation | PRD §6.1.5 inv 2-4
- Phase-03 server `SyncPushService` says "validate via TypeBox" but does not call out the cross-row invariant (`check_subtype_id` non-null iff parent `has_subtypes = 1`; `cut_value` ∈ `[0, 100]` for `cut_kind = 'pct'`).
- **Resolution:** Server-side `DoctorPricingService::accept_push(row)` adds:
  1. Load parent `CheckType` by `check_type_id`; reject if `has_subtypes` mismatches `check_subtype_id` nullability.
  2. If `cut_kind == 'pct'` && (`cut_value < 0` || `cut_value > 100`) → reject 400.
  3. If `cut_kind == 'fixed'` && `cut_value < 0` → reject 400.

### 7.7 `inventory_items` index for active filter
- **Gap:** MEDIUM | Missing Index | PRD §7.3.1, §7.4.5
- The catalog list page filters by `is_active`; no index covers it.
- **Resolution:** Update §1 migration:
  ```sql
  CREATE INDEX inventory_items_active ON inventory_items(entity_id, is_active) WHERE deleted_at IS NULL;
  ```

### 7.8 `inventory_items::soft_delete` reference guard
- **Gap:** MEDIUM | Missing Business Rule | PRD §6.1.12
- §4 `InventoryItemService::soft_delete` is unspecified; PRD implies soft-delete should block when the item is referenced by a non-deleted `inventory_consumption_map` row.
- **Resolution:** Add to §4:
  ```
  InventoryItemService::soft_delete(id):
    1. SELECT count(*) FROM inventory_consumption_map WHERE item_id = ? AND deleted_at IS NULL;
       if > 0 → Err(InventoryItemError::ReferencedByConsumptionMap)
    2. SELECT count(*) FROM inventory_adjustments WHERE item_id = ? AND deleted_at IS NULL AND created_at > now - INTERVAL 90 DAY;
       (informational warning; do not block)
    3. UPDATE inventory_items SET deleted_at = now, is_active = 0, version = version+1, dirty = 1 ...
    4. with_audit('soft_delete').
  ```

### 7.9 `inventory_consumption_map` cross-row invariants (Rust + server)
- **Gap:** HIGH | Missing Constraint | PRD §6.1.13 inv 1-2
- SQLite cannot express the "subtype required iff parent has_subtypes = 1" constraint. Phase-03 says service layer enforces it but never declares the explicit guard.
- **Resolution:** Both `ConsumptionMapService::upsert` (Rust) and `ConsumptionMapService::accept_push` (server) MUST:
  1. Load parent `CheckType` by `check_type_id`.
  2. If `parent.has_subtypes == 1` and `check_subtype_id IS NULL` → reject `ConsumptionMapError::SubtypeRequired`.
  3. If `parent.has_subtypes == 0` and `check_subtype_id IS NOT NULL` → reject `ConsumptionMapError::SubtypeForbidden`.
  Add as numbered steps in §4 for both surfaces.

### 7.10 `inventory_consumption_map` audit writes
- **Gap:** LOW | Missing Audit Trigger | PRD §4.3
- §4 does not explicitly state consumption-map upserts go through `with_audit`.
- **Resolution:** Append to `ConsumptionMapService::upsert` step list: "5. `with_audit` writes `create` or `update` action with before/after deltas." Same for `soft_delete`.

### 7.11 Admin sub-sidebar 7-area enumeration
- **Gap:** HIGH | Missing UI Element | PRD §3.3 + §7.4
- §3.Frontend `<AdminShell>` description says "eight admin sub-pages (plus Users from Phase 2 and Settings from Phase 2 and Audit from Phase 8)" - the count conflicts with PRD §3.2 (admin = 11 pages = 5 list+detail + Settings + Audit).
- **Resolution:** Rewrite the `<AdminShell>` line:
  > Sub-sidebar lists exactly seven areas in this order: **Users, Check Types, Doctors, Operators, Inventory, Settings, Audit**. Each area expands to list/detail pages where applicable. Settings and Audit are single-page leaves. Authored in this phase; populated incrementally (Users + Settings in phase-02, Audit in phase-08).

### 7.12 `<CheckTypeForm>` component declared
- **Gap:** MEDIUM | Missing Component | roadmap §Engines
- Roadmap names `<CheckTypeForm>` but §3.Frontend components table omits it (only `<HasSubtypesToggle>`, `<DoctorPricingEditor>`, etc. are declared).
- **Resolution:** Add to §3.Frontend components table:
  ```
  | <CheckTypeForm>  | src/pages/admin/check-types/form.tsx | Create/Edit form. Hosts <HasSubtypesToggle> + name_ar/name_en + base_price + dye_supported/report_supported toggles + active flag. |
  ```

### 7.13 Admin Inventory list extra columns
- **Gap:** MEDIUM | Missing UI Element | PRD §7.4.5
- PRD §7.4.5 says the admin Inventory list shows active flag + last-edit-author columns beyond the operational §7.3.1 view; §3.Frontend `/admin/inventory` description omits these.
- **Resolution:** Extend `/admin/inventory` page row in §3.Frontend table to include columns: Name (resolved by locale), Unit, Active flag chip, Last edited by (user name snapshot), Updated at. Also add the `<InventoryAdminTable>` component to the §3 Frontend components table:
  ```
  | <InventoryAdminTable> | src/components/admin/inventory-admin-table.tsx | Joins inventory_items to the audit log for the latest update row to derive author/timestamp. |
  ```

### 7.14 Search min-length and debounce policy
- **Gap:** MEDIUM | Missing Setup | PRD §10.1
- Search inputs across the catalog have no minimum-query-length or debounce policy declared.
- **Resolution:** Add to §4 (cross-surface convention): "Every searchable list IPC (`doctors::list`, `patients::list`, `inventory_catalog::list`, `check_types::list`) requires `query.trim().chars().count() >= 2` before invoking FTS or LIKE; the React hook (`use<Entity>List`) debounces 250ms before the query fires." Implemented in `src/lib/search.ts`.

### 7.15 `query` arg on low-cardinality catalog list commands
- **Gap:** LOW | Missing IPC Command | PRD §10.1
- §3.Tauri commands table shows only `includeInactive` / `includeDeleted` args on `inventory_catalog::list` and `check_types::list`; PRD §10.1 says these are LIKE-prefix searchable.
- **Resolution:** Add optional `query: Option<String>` to both command signatures; backed by `SELECT ... WHERE (name_ar LIKE ? OR name_en LIKE ?) AND ...`. Document the prefix wildcard pattern (`query%`).

### 7.16 `resolveLocaleName` helper for bilingual entities
- **Gap:** LOW | Missing Setup | PRD §10.6
- The "active locale `en` and `name_en` non-null then `name_en`; else `name_ar`" resolver is not centralized.
- **Resolution:** Add to §3.Frontend Setup:
  ```ts
  // src/lib/format/locale-name.ts
  export function resolveLocaleName(
      entity: { name_ar: string; name_en: string | null },
      locale: 'ar' | 'en',
  ): string;
  ```
  All admin/reception/accounting list cells must call this helper for bilingual entities.

### 7.17 LWW tiebreak rule re-stated per-entity
- **Gap:** LOW | Missing Conflict Policy | research.md
- §4 Sync semantics declares `last-write-wins` but does not re-state the `origin_device_id` lex tiebreak per entity. research.md has the rule once but not embedded in the phase file.
- **Resolution:** Append to the sync-policy table in §4 a single line: "All LWW entities in this phase use the global tiebreak rule: when `updated_at` matches to the millisecond, the row with the lexicographically smaller `origin_device_id` wins. Documented once in phase-01 §4 SyncEngine."

### 7.18 Audit writes on every catalog mutation
- **Gap:** MEDIUM | Missing Audit Trigger | PRD §8.5
- §4 only spells out `with_audit` for `DoctorPricingService::upsert`. PRD §8.5 ("Pricing Change Propagation") implies every pricing change writes audit. Catalog updates more broadly should also audit.
- **Resolution:** Add to §4 (cross-service rule): "Every service method that mutates a catalog row (create/update/soft_delete on any of the 8 entities introduced here) MUST call `with_audit`. The audit row uses action `create`, `update`, or `soft_delete`; before/after JSON capture the row diff." Re-stated under each service block.

### 7.19 Server-only `pulledAt` column on catalog Prisma models
- **Gap:** HIGH | Missing Field | PRD §6 (line 302)
- PRD line 302 mandates every server Prisma model includes a server-only `pulledAt` timestamp. All 8 Prisma models in §2 omit it.
- **Resolution:** Add `pulledAt DateTime? @map("pulled_at") @db.Timestamptz` to `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `InventoryItem`, `InventoryConsumptionMap`. Set by `SyncPullService` after each successful pull batch. Used for diagnostics only; not exposed to clients.

### 7.20 `doctor_check_pricing` uniqueness for NULL `checkSubtypeId`
- **Gap:** CRITICAL | Missing Validation | PRD §6.1.5 inv 1
- §2 declares `@@unique([doctorId, checkTypeId, checkSubtypeId])` on `DoctorCheckPricing`. Postgres treats NULL as distinct: two rows with same `(doctorId, checkTypeId)` and `checkSubtypeId = NULL` both insert, violating "uniqueness among non-deleted rows".
- **Resolution:** Drop the Prisma `@@unique` and replace with two partial unique indexes via a raw-SQL migration:
  ```sql
  CREATE UNIQUE INDEX doctor_check_pricing_unique_with_subtype
    ON doctor_check_pricing(doctor_id, check_type_id, check_subtype_id)
    WHERE deleted_at IS NULL AND check_subtype_id IS NOT NULL;
  CREATE UNIQUE INDEX doctor_check_pricing_unique_no_subtype
    ON doctor_check_pricing(doctor_id, check_type_id)
    WHERE deleted_at IS NULL AND check_subtype_id IS NULL;
  ```
  Server-side `DoctorPricingService::upsert` (and `acceptPush`) catches the unique-violation error and surfaces `DUPLICATE_PRICING_ROW`. Local SQLite uses `IFNULL(check_subtype_id, '')` in its unique index (already declared in §1).

### 7.21 `inventory_consumption_map` uniqueness for NULL `checkSubtypeId`
- **Gap:** HIGH | Missing Validation | PRD §6.1.13 inv 1
- Same Postgres NULL distinctness flaw as §7.20.
- **Resolution:** Replace the `@@unique` with two partial unique indexes:
  ```sql
  CREATE UNIQUE INDEX inv_consumption_unique_with_sub
    ON inventory_consumption_map(check_type_id, check_subtype_id, item_id, on_dye_only)
    WHERE deleted_at IS NULL AND check_subtype_id IS NOT NULL;
  CREATE UNIQUE INDEX inv_consumption_unique_no_sub
    ON inventory_consumption_map(check_type_id, item_id, on_dye_only)
    WHERE deleted_at IS NULL AND check_subtype_id IS NULL;
  ```

### 7.22 Operator soft-delete cascades to `operator_specialties`
- **Gap:** MEDIUM | Missing Logic | PRD §6.1.6 inv 3
- §4 `OperatorService::soft_delete` blocks on open shifts and writes an audit row but leaves `operator_specialties` rows live, creating orphan FKs.
- **Resolution:** Add steps to `OperatorService::soft_delete(id)`:
  ```
  3. SELECT id FROM operator_specialties WHERE operator_id = ? AND deleted_at IS NULL.
  4. For each row: UPDATE operator_specialties SET deleted_at = now, version = version + 1, dirty = 1, updated_at = now WHERE id = ?; with_audit('soft_delete', 'operator_specialties', specialty_id).
  5. Enqueue one outbox row per affected specialty (additive sync continuation; LWW policy).
  ```
  Mirror in server `OperatorService::acceptPush` when receiving a `soft_delete` for an operator.

### 7.23 `doctors::set_active` IPC and autocomplete filter
- **Gap:** MEDIUM | Missing IPC | PRD §6.1.4 inv 2
- PRD §6.1.4 requires inactive doctors to remain selectable on existing drafts but absent from new-visit autocomplete. §3 Tauri commands table has `doctors::update` but no dedicated activation toggle; §3 Frontend autocomplete filter is undocumented.
- **Resolution:** Add to §3 Tauri commands: `doctors::set_active(id, is_active) -> ()`. Service: bumps version, dirty, `with_audit('update', delta={is_active})`. Frontend `<DoctorAutocomplete>` (consumed by phase-05 `<NewVisitForm>`) issues `doctors::list({ active_only: true, include_id: Some(currentDraftDoctorId) })`; SQL: `WHERE deleted_at IS NULL AND (is_active = 1 OR id = :includeId)`. Documented in §4 Frontend.

### 7.24 Operator `is_active` flip while shift is open
- **Gap:** MEDIUM | Missing State Transition | PRD §6.1.6
- PRD §6.1.6 inv 3 covers soft-delete (blocked on open shift) but not `is_active=0` while open shifts exist.
- **Resolution:** Define explicitly in §4 `OperatorService::set_active(id, is_active)`: no shift-state guard. Setting `is_active=0` removes the operator from clock-in autocomplete (`operators::list({ active_only: true })`) but leaves the existing open shift live; it can be closed normally by either the same operator (if a receptionist clocks them out) or by a superadmin via `shifts::edit`. Audit action `update` with delta `{is_active}`.

### 7.25 `inventory_items.quantity_on_hand` pull-time recompute contract
- **Gap:** MEDIUM | Missing Sync Rule | PRD §6.1.12
- §4 Sync Semantics note says quantity is "informational" but does not specify pull-time behavior. Cross-device drift could compound.
- **Resolution:** Append to §4 Sync Semantics row for `inventory_items`: "On `/sync/pull` receipt of an `inventory_items` row, the local repo MUST overwrite the pulled `quantity_on_hand` with `SELECT COALESCE(SUM(delta),0) FROM inventory_adjustments WHERE item_id = ? AND deleted_at IS NULL` in the same tx as the pull apply. Pulled `quantity_on_hand` is discarded after the recompute. Until phase-05 ships `inventory_adjustments`, the pulled value is taken as-is (no adjustments to sum). Implementation: phase-06 §7.9 registers the post-apply hook." Cross-reference phase-06 §7.9.

### 7.26 `effective_price` resolver contract
- **Gap:** MEDIUM | Missing Logic | PRD §6.1.5 inv 5
- Phase-05 `VisitService::lock` needs an `effective_price(doctor_id, check_type_id, check_subtype_id)` resolver but the underlying data is owned here, and the resolver contract is undeclared.
- **Resolution:** Add to §4 `DoctorPricingService`:
  ```rust
  pub fn effective_price(
      &self,
      doctor_id: Option<Uuid>,
      check_type_id: Uuid,
      check_subtype_id: Option<Uuid>,
  ) -> Result<i64, AppError> {
      // 1. doctor_id is None → return subtype.price_iqd if Some(subtype) else check_type.base_price_iqd.
      // 2. Load doctor_check_pricing by (doctor_id, check_type_id, check_subtype_id) WHERE deleted_at IS NULL.
      // 3. If found AND price_override_iqd IS NOT NULL → return price_override_iqd.
      // 4. Else fall back to subtype.price_iqd or check_type.base_price_iqd.
  }
  ```
  Read-only contract consumed by phase-05 `VisitService::lock` step 4; never mutates state.

### 7.27 `catalog:pricing_changed` event emission
- **Gap:** HIGH | Missing Handshake | phase-05 §7.27
- Phase-05 §7.27 says `<PricingChangedBanner>` listens for `catalog:pricing_changed` "emitted by phase-03 services". Phase-03 has no receipt declaring the emit point.
- **Resolution:** Add to §4 (cross-service rule): "After `with_audit` commits for any mutation in `CheckTypeService::update`, `CheckTypeService::toggle_has_subtypes`, `CheckSubtypeService::create|update|soft_delete`, or `DoctorPricingService::upsert|soft_delete`, the service emits a Tauri event `catalog:pricing_changed` with payload `{ entity: 'check_type'|'check_subtype'|'doctor_check_pricing', entity_id: Uuid, changed_at: DateTime<Utc> }`." Frontend `<PricingChangedBanner>` (phase-05) and `<ActiveDraftsBadge>` listen. Implemented via `app_handle.emit_all("catalog:pricing_changed", payload)`.

### 7.28 `handle.crumb` on admin detail routes
- **Gap:** LOW | Missing Handshake | phase-01 §7.13
- Phase-01 §7.13 reads breadcrumbs from each route's `handle.crumb`. No admin detail route declares it.
- **Resolution:** Append to §3 Frontend routing block: "Every admin detail route (`/admin/check-types/:id`, `/admin/doctors/:id`, `/admin/operators/:id`, `/admin/inventory/:id`, etc.) exports a `handle: { crumb: ({ data }) => resolveLocaleName(data) }` so phase-01 §7.13 `<Breadcrumbs>` can render the entity display name."

### 7.29 `errors` i18n namespace key inventory (consolidated)
- **Gap:** MEDIUM | Missing i18n Key | phase-01 §7.10
- Phase-01 §7.10 scaffolded the `errors` namespace shell. No phase enumerated the keys it must contain; phase-08 i18n lint cannot verify coverage.
- **Resolution:** Consolidated key inventory (each phase's services must emit messages matching these keys):
  - phase-02: `errors:auth.invalid`, `errors:auth.locked`, `errors:auth.session_expired`, `errors:auth.forbidden`, `errors:settings.required_key`, `errors:settings.thermal_width_invalid`, `errors:user.email_taken`, `errors:user.first_admin_exists`.
  - phase-03: `errors:catalog.subtypes_exist`, `errors:catalog.parent_not_subtyped`, `errors:catalog.referenced`, `errors:consumption.subtype_required`, `errors:consumption.subtype_forbidden`, `errors:doctor.referenced`, `errors:operator.referenced`, `errors:pricing.duplicate_row`.
  - phase-04: `errors:shift.open_exists`, `errors:shift.not_open`, `errors:shift.checkout_before_checkin`, `errors:shift.deleted`.
  - phase-05: `errors:operator.ineligible.no_qualified`, `errors:operator.ineligible.not_on_shift`, `errors:operator.ineligible.specialty_missing`, `errors:visit.patient_name_empty`, `errors:visit.invariant_broken`, `errors:visit.discard_not_draft`, `errors:visit.illegal_transition`, `errors:visit.already_locked`, `errors:visit.terminal`, `errors:patient.referenced`, `errors:void.reason_too_short`, `errors:void.not_locked`, `errors:void.already_voided`.
  - phase-06: `errors:adjustment.forbidden`, `errors:adjustment.delta_zero`, `errors:adjustment.immutable`.
  - phase-08: `errors:audit.immutable`, `errors:sync.conflict_parked`, `errors:sync.unsupported_op`.
  Phase-08 i18n lint cross-checks each Rust error variant against this inventory and fails CI if a variant lacks an i18n key on both locales.

### 7.30 Prisma back-relations: `Operator.shifts` and `Visit[]` on catalog parents
- **Gap:** CRITICAL/HIGH | Missing Relation | Pass-3 GAP-B-2 + GAP-B-3; PRD §6.1.2 line 453, §6.1.3 line 525, §6.1.4 line 595, §6.1.6 line 750; phase-04 §2; phase-05 §2
- (1) `Operator` (§2) lacks `shifts OperatorShift[]` back-relation; phase-04's `operator Operator @relation(...)` won't validate. (2) PRD declares `visits Visit[]` back-relations on `CheckType`, `CheckSubtype`, `Doctor`, `Operator`; phase-03 §2 strips them; when phase-05 introduces the `Visit` model with FK relations the schema will fail validation.
- **Resolution:** Add to §2 (or amend during phase-05 schema land):
  ```prisma
  // model Operator
  shifts  OperatorShift[]
  visits  Visit[]

  // model CheckType
  visits  Visit[]

  // model CheckSubtype
  visits  Visit[]

  // model Doctor
  visits  Visit[]
  ```
  Inverse-only fields (no FK on this side). No migration impact. Cross-reference phase-04 §7.13 / phase-05 §7.52 for symmetric forward fields.

### 7.31 Raw-SQL migration ordering vs `prisma migrate deploy`
- **Gap:** HIGH | Missing Migration Spec | Pass-3 GAP-A-1; §7.20, §7.21
- §7.20 / §7.21 add raw-SQL partial unique indexes (replacing dropped Prisma `@@unique` blocks). No phase declares the operational order: how these raw-SQL migrations live alongside `prisma migrate dev --create-only` files, and whether `prisma migrate deploy` applies DROP-then-CREATE in the safe order.
- **Resolution:** Append to Section 5 Infrastructure: "All raw-SQL migrations from §7.20 / §7.21 ship as `prisma migrate dev --create-only` migrations with hand-edited SQL stored under `sync-server/prisma/migrations/<ts>_<name>/migration.sql`. File order: (a) drop the old `@@unique` block in `<ts>_drop_unique_<entity>/migration.sql`; (b) create the partial unique index in the next sequential `<ts+1>_partial_unique_<entity>/migration.sql`. `prisma migrate deploy` runs files in lex order. Each phase prefixes its raw-SQL migration filename with the same numeric epoch as its owning phase (003 for phase-03). Verification: `pnpm prisma migrate status` is clean after each phase merge; `pnpm prisma validate` passes." Mirrored for phase-05 raw-SQL migrations in phase-05 §7.51.

### 7.32 i18n key inventory amendment
- **Gap:** MEDIUM | Missing i18n Key | §7.29; Pass-3 GAP-A-2
- §7.29 inventory missed: phase-01 sync error keys (added in phase-01 §7.30); phase-04 `errors:shift.edit_forbidden_role`, `errors:shift.soft_delete_open`, `errors:shift.illegal_transition`; phase-08 `errors:rtl.icon_unmirrored` lint diagnostic; phase-03 `errors:consumption.dye_not_supported_on_parent` (per §7.34).
- **Resolution:** Amend §7.29 phase rows in place. Final keys per phase:
  - phase-01 (new row): `errors:sync.network_offline`, `errors:sync.server_unavailable`, `errors:sync.auth_expired`, `errors:sync.already_resolved`.
  - phase-03 (extend): add `errors:consumption.dye_not_supported_on_parent`.
  - phase-04 (extend): add `errors:shift.edit_forbidden_role`, `errors:shift.soft_delete_open`, `errors:shift.illegal_transition`.
  - phase-08 (extend): add `errors:rtl.icon_unmirrored`.

### 7.33 `doctors_fts` triggers filter soft-deleted rows
- **Gap:** MEDIUM | Missing FTS Trigger Filter | Pass-3 GAP-B-4; §1 doctors_fts triggers
- §1 declares triggers `doctors_ai`, `doctors_ad`, `doctors_au` indexing every row including soft-deleted. PRD §6.1.4 inv 2 ("inactive doctors do not appear in receptionist autocomplete") is covered by §7.23; soft-deleted rows still leak into FTS results because triggers don't filter on `deleted_at`.
- **Resolution:** Replace the §1 trigger definitions:
  ```sql
  CREATE TRIGGER doctors_ai AFTER INSERT ON doctors WHEN new.deleted_at IS NULL BEGIN
    INSERT INTO doctors_fts(rowid, name, specialty)
      VALUES (new.rowid, new.name, COALESCE(new.specialty, ''));
  END;
  CREATE TRIGGER doctors_ad AFTER DELETE ON doctors BEGIN
    INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty)
      VALUES ('delete', old.rowid, old.name, COALESCE(old.specialty, ''));
  END;
  CREATE TRIGGER doctors_au AFTER UPDATE ON doctors BEGIN
    INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty)
      VALUES ('delete', old.rowid, old.name, COALESCE(old.specialty, ''));
    INSERT INTO doctors_fts(rowid, name, specialty)
      SELECT new.rowid, new.name, COALESCE(new.specialty, '')
      WHERE new.deleted_at IS NULL;
  END;
  ```
  Soft-delete (UPDATE setting `deleted_at`) removes the FTS row; un-soft-delete (UPDATE clearing `deleted_at`) re-adds it.

### 7.34 `on_dye_only` requires parent `dye_supported=1` in service
- **Gap:** MEDIUM | Missing Validation | Pass-3 GAP-C-5; PRD §6.1.13
- Frontend `<ConsumptionMapEditor>` enables the `on_dye_only` toggle only when parent `dye_supported=1`. Service-layer `ConsumptionMapService::upsert` (Tauri + server) only validates `quantity_per_check>0` and subtype-required (§7.9). A sync apply from a buggy device can land `on_dye_only=1` against a non-dye check type.
- **Resolution:** Extend `ConsumptionMapService::upsert` (Rust) and `ConsumptionMapService::accept_push` (server) step list:
  ```
  4. If on_dye_only == 1, load parent CheckType.dye_supported.
     If parent.dye_supported == 0 -> return ConsumptionMapError::DyeNotSupportedOnParent
     (`errors:consumption.dye_not_supported_on_parent`, mapped to 422 on the wire).
  ```
  i18n key registered in §7.32 inventory amendment.

### 7.35 `catalog:pricing_changed` event payload schema
- **Gap:** MEDIUM | Missing Event Contract | §7.27; Pass-3 GAP-D-4; phase-05 §7.27
- §7.27 emits `catalog:pricing_changed`; phase-05 §7.27 declares the banner. Neither defines the payload, so the banner cannot decide whether a given open draft is actually affected.
- **Resolution:** Append the event payload schema to §7.27:
  ```rust
  // Tauri event: emit_filter("catalog:pricing_changed", payload);
  pub struct PricingChangedPayload {
    pub kind: PricingChangeKind,         // 'check_type' | 'check_subtype' | 'doctor_pricing' | 'settings'
    pub changed_entity_id: Uuid,
    pub check_type_id: Option<Uuid>,
    pub check_subtype_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub changed_at: DateTime<Utc>,
  }
  ```
  TypeScript mirror in `src/lib/events/pricing.ts`. Phase-05 §7.27 banner filters its open drafts by intersecting `(check_type_id, check_subtype_id, doctor_id)` against the payload before rendering -- drafts whose tuple does not match the changed scope are not banner-flagged.

### 7.36 `/admin/*` route-level role gate
- **Gap:** HIGH | Missing Role Guard | PRD §7.4 line 1849; Pass-3 GAP-E-8
- Phase-02 §7.8 declared `<RequireRole>` but never said `/admin/*` outlet uses it. `<AdminShell>` (this phase §3) has no guard; receptionists could navigate to admin URLs.
- **Resolution:** Append to §3 Frontend routing block: "The `/admin/*` outlet is wrapped in `<RequireRole roles={['superadmin']}>` (component from phase-02 §7.8); `<AdminShell>` renders inside the guard. Non-superadmin requests redirect to `/no-access` (page declared in phase-02 §3). Sub-sidebar links remain hidden in `<UserMenu>` for non-superadmins via the same role check."
