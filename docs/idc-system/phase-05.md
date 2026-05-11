# Phase 5: Reception & Visit Lock

**Goal:** Deliver the per-check reception workflow end-to-end. Ship `patients`, `visits`, and `inventory_adjustments`; build the Checks Grid, Check Workspace, New Visit, Visit Detail, and Void modal; implement the lock workflow inside one SQLite transaction with snapshot math, operator eligibility, inventory consumption, audit, and receipt generation (A5 PDF + thermal text).

**Surfaces:** All
**Dependencies:** Phase 04
**Complexity:** XL

## §1 Local Schema Changes (Tauri SQLite)

Migration file: `src-tauri/migrations/005_patients_visits_adjustments.sql`.

### patients (PRD §6.1.9) + FTS

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

CREATE TRIGGER patients_ai AFTER INSERT ON patients BEGIN
  INSERT INTO patients_fts(rowid, name) VALUES (new.rowid, new.name);
END;
CREATE TRIGGER patients_ad AFTER DELETE ON patients BEGIN
  INSERT INTO patients_fts(patients_fts, rowid, name) VALUES('delete', old.rowid, old.name);
END;
CREATE TRIGGER patients_au AFTER UPDATE ON patients BEGIN
  INSERT INTO patients_fts(patients_fts, rowid, name) VALUES('delete', old.rowid, old.name);
  INSERT INTO patients_fts(rowid, name) VALUES (new.rowid, new.name);
END;
```

### visits (PRD §6.1.10)

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

### inventory_adjustments (PRD §6.1.14)

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

### Modified tables

None.

### New enums

- `visits.status CHECK IN ('draft','locked','voided')`.
- `inventory_adjustments.reason CHECK IN ('receive','writeoff','count_correction','consume_visit')`.

## §2 Server Schema Changes (Prisma / Postgres)

### Patient (PRD §6.1.9)

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

### Visit (PRD §6.1.10)

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

enum VisitStatus { draft locked voided }
```

### InventoryAdjustment (PRD §6.1.14)

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

enum AdjustmentReason { receive writeoff count_correction consume_visit }
```

### New enums

`VisitStatus`, `AdjustmentReason`.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/reception` | `src/pages/reception/checks-grid.tsx` | Cards per active check type (PRD §7.1.1). |
| `/reception/checks/:slug` | `src/pages/reception/check-workspace.tsx` | Per-check workspace with today's visits (PRD §7.1.2). |
| `/reception/checks/:slug/new` | `src/pages/reception/new-visit.tsx` | New-visit form (PRD §7.1.3). |
| `/reception/visits/:id` | `src/pages/reception/visit-detail.tsx` | Details / Audit / Receipts tabs (PRD §7.1.4). |

Slug resolution: `check_types.id` maps to a slug via `slugify(name_en ?? transliterate(name_ar))`; resolved at route-load time from local SQLite, not stored.

Components:

| Component | File | Purpose |
|-|-|-|
| `<ChecksGridCard>` | `src/components/reception/checks-grid-card.tsx` | Single check card with today's count. |
| `<WorkspaceVisitsTable>` | `src/components/reception/workspace-visits-table.tsx` | Today's visits in this check. |
| `<NewVisitForm>` | `src/components/reception/new-visit-form.tsx` | Form per PRD §7.1.3. |
| `<PatientAutocomplete>` | `src/components/reception/patient-autocomplete.tsx` | FTS5 over `patients_fts`; last 30 days. |
| `<DoctorAutocomplete>` | `src/components/reception/doctor-autocomplete.tsx` | FTS5 over `doctors_fts`; empty = house. |
| `<SubtypeRadioList>` | `src/components/reception/subtype-radio-list.tsx` | Radio cards listing non-deleted subtypes with prices. |
| `<DyeReportToggles>` | `src/components/reception/dye-report-toggles.tsx` | Gated by `check_types.dye_supported` / `report_supported`. |
| `<RunningTotalSummary>` | `src/components/reception/running-total-summary.tsx` | Live total + doctor/operator-cut preview. |
| `<OperatorPickerDialog>` | `src/components/reception/operator-picker-dialog.tsx` | Shown at lock time; lists qualified operators only. |
| `<VisitDetailDetailsTab>` | `src/components/reception/visit-detail-details-tab.tsx` | Read-only snapshot panel. |
| `<VisitDetailAuditTab>` | `src/components/reception/visit-detail-audit-tab.tsx` | Filtered `audit_log` on this visit. |
| `<VisitDetailReceiptsTab>` | `src/components/reception/visit-detail-receipts-tab.tsx` | PDF + thermal list with reprint button. |
| `<VoidModal>` | `src/components/reception/void-modal.tsx` | Reason input (>= 5 chars). |

Zustand stores:

| Store | File | State |
|-|-|-|
| `useDraftVisitStore` | `src/stores/draft-visit-store.ts` | Per-route draft cache for the new-visit form, scoped by `(workspaceSlug, draftId)`. Persists across reloads. |

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useTodayVisitsByCheck(typeId)` | `['visits','byCheck', typeId, 'today']` | Today's visits in a workspace. |
| `useVisit(id)` | `['visits','detail', id]` | Visit row + joined references. |
| `useVisitAuditLog(id)` | `['visits','audit', id]` | Audit rows filtered on `entity='visits' AND entity_id=:id`. |
| `useVisitReceipts(id)` | `['visits','receipts', id]` | Receipt artifacts list. |
| `useQualifiedOperators(checkTypeId)` | `['operators','qualified', checkTypeId]` | Live computation. |
| Mutations: `useVisitCreate`, `useVisitUpdate`, `useVisitDiscard`, `useVisitLock`, `useVisitVoid`, `useReceiptReprint` | per IPC | |
| `usePatientSearch(query)` | `['patients','search', query]` | FTS5. |

Zod schemas:

| Schema | File |
|-|-|
| `PatientSchema`, `PatientCreateSchema` | `src/lib/schemas/patient.ts` |
| `VisitSchema`, `VisitDraftSchema`, `VisitLockInputSchema`, `VisitVoidInputSchema` | `src/lib/schemas/visit.ts` |
| `InventoryAdjustmentSchema` | `src/lib/schemas/inventory.ts` (extended from Phase 3) |

### Tauri / Rust

Domain entities (in `src-tauri/src/domains/visits/` and `src-tauri/src/domains/patients/`):

```rust
pub struct Patient {
  pub id: Uuid,
  pub name: String,
  pub entity_id: String,
}
impl Patient {
  pub fn try_new(name: &str) -> Result<Self, AppError> { /* trim, min length 2 */ }
}

pub enum VisitStatus { Draft, Locked, Voided }

pub struct Visit {
  pub id: Uuid,
  pub patient_id: Uuid,
  pub status: VisitStatus,
  pub receptionist_user_id: Uuid,
  pub check_type_id: Uuid,
  pub check_subtype_id: Option<Uuid>,
  pub doctor_id: Option<Uuid>,
  pub operator_id: Option<Uuid>,
  pub dye: bool,
  pub report: bool,
  pub locked_at: Option<DateTime<Utc>>,
  pub voided_at: Option<DateTime<Utc>>,
  pub voided_by_user_id: Option<Uuid>,
  pub void_reason: Option<String>,
  pub snapshots: Option<VisitSnapshots>,
  pub entity_id: String,
}
pub struct VisitSnapshots {
  pub price_iqd: i64,
  pub dye_cost_iqd: i64,
  pub report_cost_iqd: i64,
  pub doctor_cut_iqd: i64,
  pub operator_cut_iqd: i64,
  pub internal_pct: Option<i64>,
  pub total_amount_iqd: i64,
}
impl Visit {
  pub fn create_draft(...) -> Result<Self, AppError> { ... }
  pub fn edit_draft(self, patch: VisitDraftPatch) -> Result<Self, AppError> { /* only when status=Draft */ }
  pub fn lock(self, operator_id: Uuid, snapshots: VisitSnapshots, at: DateTime<Utc>) -> Result<Self, AppError> { ... }
  pub fn void(self, reason: String, by_user_id: Uuid, at: DateTime<Utc>) -> Result<Self, AppError> { ... }
}

pub struct InventoryAdjustment { ... }
pub enum AdjustmentReason { Receive, Writeoff, CountCorrection, ConsumeVisit }
```

Money math module (`src-tauri/src/domains/visits/money_math.rs`):

```rust
pub struct MoneyInputs<'a> { /* references to check_type, subtype?, doctor_pricing?, settings, operator, dye, report */ }
pub fn compute(inputs: &MoneyInputs) -> Result<VisitSnapshots, AppError>;
```

Operator-eligibility module (`src-tauri/src/domains/visits/operator_eligibility.rs`):

```rust
pub async fn qualified(repo: &dyn QualificationRepo, visit: &Visit) -> Result<Vec<Operator>, AppError>;
```

Receipt generator (`src-tauri/src/domains/receipts/`):

```rust
pub struct ReceiptArtifacts { pub pdf_path: PathBuf, pub thermal_path: PathBuf }
pub fn render(visit: &Visit, references: &ReceiptReferences) -> Result<ReceiptArtifacts, AppError>;
```

PDF rendering uses `printpdf` or `wkhtmltopdf` crate (TBD in implementation; the plan locks the contract: `ReceiptArtifacts { pdf_path, thermal_path }` and a single `render()` entry point). Thermal renders fixed-width text with the `settings.thermal_width` (32 or 48 chars).

Repository traits:

```rust
#[async_trait]
pub trait PatientRepo {
  async fn create(&self, tx: &mut Tx, patient: Patient) -> Result<(), AppError>;
  async fn update(&self, tx: &mut Tx, patient: Patient) -> Result<(), AppError>;
  async fn soft_delete(&self, tx: &mut Tx, id: Uuid) -> Result<(), AppError>;
  async fn search(&self, query: &str, limit: usize) -> Result<Vec<Patient>, AppError>;
  async fn get(&self, id: Uuid) -> Result<Option<Patient>, AppError>;
}

#[async_trait]
pub trait VisitRepo {
  async fn create(&self, tx: &mut Tx, visit: Visit) -> Result<(), AppError>;
  async fn update(&self, tx: &mut Tx, visit: Visit) -> Result<(), AppError>;
  async fn soft_delete(&self, tx: &mut Tx, id: Uuid) -> Result<(), AppError>;
  async fn get(&self, id: Uuid) -> Result<Option<Visit>, AppError>;
  async fn list_today_by_check(&self, check_type_id: Uuid) -> Result<Vec<Visit>, AppError>;
}

#[async_trait]
pub trait InventoryAdjustmentRepo {
  async fn append(&self, tx: &mut Tx, adjustment: InventoryAdjustment) -> Result<(), AppError>;
  async fn list_consume_for_visit(&self, visit_id: Uuid) -> Result<Vec<InventoryAdjustment>, AppError>;
  async fn recompute_item(&self, tx: &mut Tx, item_id: Uuid) -> Result<i64, AppError>;
}
```

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `patients::search` | `{ query, limit }` | `Patient[]` | FTS5. |
| `patients::create` | `{ name }` | `Patient` | |
| `patients::get` | `{ id }` | `Patient` | |
| `visits::list_today_by_check` | `{ checkTypeId }` | `VisitWithJoinedRefs[]` | Workspace listing. |
| `visits::get` | `{ id }` | `VisitWithJoinedRefs` | |
| `visits::create_draft` | `VisitCreateDraftInput` | `Visit` | Creates draft tied to a check type and patient. |
| `visits::update_draft` | `VisitUpdateDraftInput` | `Visit` | Edits subtype / doctor / dye / report. |
| `visits::discard` | `{ visitId }` | `()` | Soft-delete draft. |
| `visits::qualified_operators` | `{ checkTypeId }` | `Operator[]` | Computes eligibility set. |
| `visits::lock` | `{ visitId, operatorId }` | `LockResult` | Runs the full lock workflow. |
| `visits::void` | `{ visitId, reason }` | `VoidResult` | Superadmin-only. |
| `receipts::reprint` | `{ visitId }` | `ReceiptArtifacts` | Re-renders if files missing; opens print dialog. |

`LockResult` shape: `{ visit: VisitWithJoinedRefs, artifacts: ReceiptArtifacts }`.

Register all in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity classes: `Patient`, `Visit`, `InventoryAdjustment` with `static create()` validators and `toResponse()` shapers.

Repository interfaces:

```ts
interface PatientRepository { /* upsert, get, list */ }
interface VisitRepository { /* upsert, get, list with snapshot validation */ }
interface InventoryAdjustmentRepository { /* additive upsert */ }
```

Prisma repos: `Visit` push acceptance validates the `status` check constraint server-side via TypeBox refinement.

TypeBox schemas:

| Schema | Purpose |
|-|-|
| `PatientPushSchema` / `PatientResponseSchema` | Push + response. |
| `VisitPushSchema` / `VisitResponseSchema` | Including snapshot fields, all conditional on `status`. |
| `InventoryAdjustmentPushSchema` / `InventoryAdjustmentResponseSchema` | |
| `ConflictVisitResponseSchema` | `{ local, server }` envelope for 409 on `visits`. |

Route table:

| Method | Path | Description |
|-|-|-|
| (no new routes) | n/a | Visits / patients / adjustments flow through `/sync/push` and `/sync/pull`. |

## §4 Business Logic

### Frontend

`<NewVisitForm>` flow per PRD §7.1.3:

1. Patient name field with FTS autocomplete over `patients_fts`; on selection, fills `patient_id`; on free type with no selection, a new patient row is created on first save via `patients::create`.
2. Subtype radio cards: rendered iff parent `check_types.has_subtypes = 1`.
3. Doctor autocomplete: empty = house.
4. Dye / Report toggles: disabled with tooltip if not supported by check type.
5. Live running total: computes via local `money_math` using the same module as the Rust lock workflow to keep parity.
6. Save Draft: dispatches `visits::create_draft` or `visits::update_draft`; updates query cache.
7. Discard: confirm modal; dispatches `visits::discard`.
8. Lock & Print:
   1. Dispatches `visits::qualified_operators`.
   2. If empty, surfaces `LockError::NoQualifiedOperator` toast.
   3. Else opens `<OperatorPickerDialog>`; on confirm, dispatches `visits::lock`.
   4. On success, opens the OS print dialog with the PDF artifact and writes the thermal text to the configured thermal printer via `tauri-plugin-dialog` save-as (per PRD §11.4 rejected auto-print).
   5. Navigates to `/reception/visits/:id`.

`<VoidModal>` flow per PRD §8.2:

1. Reason input, min 5 chars.
2. Submit dispatches `visits::void`.
3. Caller role is superadmin (UI gate).

### Tauri / Rust

`VisitService::create_draft(input, current_user)`:

1. Validate `check_type_id` exists and is not deleted.
2. If parent has subtypes, require `check_subtype_id`; else require null.
3. Validate dye/report against type capabilities.
4. `with_audit(action='create', entity='visits', entity_id=new_id)`.

`VisitService::lock(visit_id, operator_id, by_user_id)`:

1. Load draft visit; reject if status != Draft.
2. Validate `check_subtype_id` consistency.
3. Validate dye/report.
4. Compute operator eligibility; reject if `operator_id` not in eligibility set (`LockError::NoQualifiedOperator` if set empty; `LockError::OperatorNotQualified` if mismatch).
5. Resolve money math via `money_math::compute`.
6. Open SQLite transaction:
   1. Update `visits` row with `operator_id`, snapshots, `locked_at`, `status='locked'`, `updated_at`, `version+1`, `dirty=1`.
   2. Resolve `inventory_consumption_map` rows for `(check_type_id, check_subtype_id?)` filtered by `on_dye_only ⇒ dye=1`.
   3. For each match: append one `inventory_adjustments` row with `delta = -quantity_per_check`, `reason='consume_visit'`, `visit_id=visit.id`.
   4. Recompute `inventory_items.quantity_on_hand` for each affected item.
   5. Write audit rows for the visit lock and each adjustment and each item recompute.
   6. Render receipts via `receipts::render`.
   7. If receipt render fails, abort the transaction (entire lock aborts).
   8. Enqueue outbox rows for visit, each adjustment, each item, audit rows.
   9. Commit.
7. Return `LockResult{ visit, artifacts }`.

`VisitService::void(visit_id, reason, by_user_id)`:

1. Load visit; reject unless status=Locked.
2. Caller role must be superadmin.
3. Open SQLite transaction:
   1. Update `visits` row to `status='voided'`, `voided_at=now`, `voided_by_user_id`, `void_reason`.
   2. Read all `inventory_adjustments` for `visit_id` with `reason='consume_visit'`.
   3. Append offsetting positive-delta rows with same `reason='consume_visit'` and same `visit_id` (sign flipped).
   4. Recompute `quantity_on_hand` for affected items.
   5. Write audit rows (`void` on visit; `create` on each offset; `update` on each item).
   6. Enqueue outbox.
   7. Commit.

### Sync Server

`Visit` push acceptance:

1. Validate TypeBox schema (status-conditional fields).
2. `manual` policy: load existing row by `(id, entityIdTenant)`; if exists and (`local.version > pushed.version` or `local.updatedAt > pushed.updatedAt`) and the snapshot or status differ, park in `ConflictParked` and respond 409.
3. Else upsert.
4. Insert audit row in same Prisma transaction.

`InventoryAdjustment` push acceptance: pure additive; replays return cached `ProcessedOp` response.

`Patient` push acceptance: `last-write-wins`.

### Sync Semantics

| Entity | Policy | Idempotency | Notes |
|-|-|-|-|
| `patients` | `last-write-wins` | `op_id` | |
| `visits` | `manual` | `op_id` | Conflict envelope returns full local + server payload. Resolver UI ships in Phase 8. |
| `inventory_adjustments` | `additive-only` | `op_id` | Both consume and offset rows survive; sign tracks the effect. |

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
  'patients', 'visits', 'inventory_adjustments',
] as const;
```

### Audit trigger additions

None (audit via `with_audit`).

### Local SQLite indexes

Listed inline with each `CREATE TABLE`.

### Tauri capabilities

Edit `src-tauri/capabilities/default.json` to add file-system scopes for receipts:

- `fs:scope: $APPDATA/idc-system/receipts/**`.
- `dialog:save`, `dialog:open` already from Phase 1.

### Plugin registrations

- PDF crate (`printpdf` or equivalent) added via `cargo add`.
- No new Tauri plugins beyond Phase 1.

### Fastify plugins / BullMQ queues

- BullMQ NOT introduced; receipts are local-only artifacts; no server upload yet (Document Center deferred to Horizon 1).

### What this phase does NOT touch

- No inventory operations UI (Phase 6 builds the Adjust page).
- No accounting reports (Phase 7).
- No resolver UI (Phase 8).

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover money_math correctness across all branches (flat/subtype/override/house/dye/report combinations), operator eligibility, lock state-machine transitions, void offsetting math, receipt render failure aborting the lock.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`:
   1. Navigate to `/reception`; see check cards with today's count.
   2. Click a card; create a new draft visit; pick subtype if applicable; pick doctor or leave empty (house).
   3. Toggle dye; observe running total updates.
   4. Try Lock with no operator on shift -> error toast.
   5. Clock in an operator with matching specialty.
   6. Lock again -> operator picker; pick; print dialog opens with PDF.
   7. Visit detail loads with Details / Audit / Receipts tabs populated.
   8. Reprint receipt from the Receipts tab.
   9. Superadmin voids the visit; assert status, inventory offset, and audit rows.
5. `cd sync-server && pnpm test`: visit push 409 conflict test (two devices edit same draft); patient push echo; adjustment additive replay.
6. Sync round-trip: lock a visit offline; reconnect; assert the visit, adjustments, and audit rows all arrive on the server.
7. Conflict scenario: two devices each edit the same draft; one locks while the other still drafts; reconnect; the second push returns 409; assert `ConflictParked` row exists server-side.
8. Receipts: assert A5 PDF file written to `$APPDATA/idc-system/receipts/<YYYY>/<MM>/<visit-id>.pdf` and thermal text at `.../thermal/<visit-id>.txt`.
9. Performance: instrument a lock end-to-end; assert p95 < 30 seconds on a representative dev machine.
10. Audit: every business mutation (create, update, discard, lock, void, each adjustment, each item recompute) writes its own audit row in the same transaction.
11. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 30 gaps incorporated below._

### 7.1 `visits` CHECK enforces `internal_pct_snapshot` iff `doctor_id IS NULL`
- **Gap:** HIGH | Missing Constraint | PRD §6.1.10 inv 6
- The CHECK constraint on `visits` enforces snapshot-non-null when `status = 'locked'` but does not enforce the iff relationship between `internal_pct_snapshot` and `doctor_id`.
- **Resolution:** Extend the local CHECK constraint:
  ```sql
  CHECK (
      status != 'locked' OR (
          price_snapshot_iqd IS NOT NULL
          AND dye_cost_snapshot_iqd IS NOT NULL
          AND report_cost_snapshot_iqd IS NOT NULL
          AND doctor_cut_snapshot_iqd IS NOT NULL
          AND operator_cut_snapshot_iqd IS NOT NULL
          AND total_amount_iqd_snapshot IS NOT NULL
          AND ((doctor_id IS NULL AND internal_pct_snapshot IS NOT NULL)
            OR (doctor_id IS NOT NULL AND internal_pct_snapshot IS NULL))
      )
  )
  ```
  Server TypeBox `VisitPushSchema` refinement enforces the same.

### 7.2 Total-amount equality invariant
- **Gap:** MEDIUM | Missing Business Rule | PRD §6.1.10 inv 6 equality
- The plan never enforces `total_amount_iqd_snapshot = price_snapshot_iqd + dye_cost_snapshot_iqd + report_cost_snapshot_iqd`.
- **Resolution:** Add to §1 CHECK:
  ```sql
  CHECK (status != 'locked' OR total_amount_iqd_snapshot = price_snapshot_iqd + dye_cost_snapshot_iqd + report_cost_snapshot_iqd)
  ```
  Server-side validator re-asserts. Unit test in `money_math::tests::total_equals_sum`.

### 7.3 Subtype / dye / report consistency at DB layer
- **Gap:** MEDIUM | Missing Constraint | PRD §6.1.10 inv 2-5
- Invariants 2-5 (subtype required iff parent `has_subtypes = 1`; `dye = 1` requires `dye_supported = 1`; `report = 1` requires `report_supported = 1`) live only in service code; a sync apply from a malicious / buggy device can bypass.
- **Resolution:** Cross-table CHECK is not expressible in SQLite. Enforce at:
  - `VisitService::lock` step 1 (existing) - keep.
  - Sync-apply path: phase-01 `SyncEngine::apply_pull` adds a per-entity validator hook; for `visits`, run a re-validate routine that loads the referenced `check_type` and `check_subtype` rows and rejects if invariants 2-5 fail.
  - Server-side `VisitService::accept_push` runs the same checks before persisting.

### 7.4 Server Prisma indexes mirror local
- **Gap:** MEDIUM | Missing Index | PRD §6.1.10
- Server Prisma `Visit` model lacks `@@index([entityId, status, lockedAt])`; report queries on the server will full-scan.
- **Resolution:** Add to §2:
  ```prisma
  model Visit {
      ...
      @@index([entityId, status, lockedAt])
      @@index([entityId, patientId])
      @@index([entityId, checkTypeId, status])
  }
  ```
  Run `prisma migrate dev` after pulling the next schema.

### 7.5 Local index for draft listing
- **Gap:** MEDIUM | Missing Index | PRD §7.1.2
- Drafts have `locked_at = NULL`, so the `visits_status_date` index is half-blind for the workspace draft list.
- **Resolution:** Add to §1 migration:
  ```sql
  CREATE INDEX visits_drafts ON visits(entity_id, check_type_id, created_at DESC)
      WHERE status = 'draft' AND deleted_at IS NULL;
  ```

### 7.6 `VisitPushSchema` TypeBox refinement detail
- **Gap:** MEDIUM | Missing Validation | PRD §6.1.10
- §3.Server TypeBox schemas list `VisitPushSchema` without enumerating the status-conditional invariants.
- **Resolution:** Extend §3.Server schemas table:
  > `VisitPushSchema` uses `Type.Composite([VisitBase, Type.Union([DraftFields, LockedFields, VoidedFields])])`. The `LockedFields` variant requires all 7 snapshot columns non-null; the `VoidedFields` variant additionally requires `voided_at`, `voided_by_user_id`, `void_reason`. A custom `ajv-keywords` refinement enforces `total = price + dye + report` and the `internal_pct ↔ doctor_id` iff.

### 7.7 `visits::list_drafts_by_check` IPC
- **Gap:** MEDIUM | Missing IPC Command | PRD §7.1.2
- §3.Tauri commands include `visits::list_today_by_check(check_id)` but the workspace also needs to resume drafts from yesterday/today. The existing command filters by `date(created_at) = today`.
- **Resolution:** Add to §3.Tauri commands table:
  ```
  | visits::list_drafts_by_check | { check_type_id } | Vec<VisitSummary>  | Returns all non-deleted visits with status='draft' for the check, regardless of date. Backed by visits_drafts index. |
  ```

### 7.8 `void_reason` min-length DB CHECK
- **Gap:** LOW | Missing Validation | PRD §8.2
- Frontend enforces min-5 chars; sync push from another device could deliver a 1-char reason.
- **Resolution:** Extend §1 CHECK on `visits`:
  ```sql
  CHECK (status != 'voided' OR (void_reason IS NOT NULL AND length(trim(void_reason)) >= 5))
  ```

### 7.9 `Patient::try_new` non-empty rule aligned with PRD
- **Gap:** LOW | Missing Business Rule | PRD §6.1.9 inv 1
- `Patient::try_new` enforces min length 2; PRD says non-empty after trim.
- **Resolution:** Align to PRD: `if name.trim().is_empty() { return Err(PatientError::NameEmpty); }`. Drop the min-2 constraint.

### 7.10 Lock workflow respects audit-first ordering
- **Gap:** HIGH | Wrong Order | PRD §4.3
- §4 lock step 6.5 writes audit rows after business mutations (6.1-6.4). PRD §4.3 mandates audit-first ordering.
- **Resolution:** Restructure §4 `VisitService::lock` step 6 (inside `with_audit` two-pass closure per phase-01 §7.7):
  - 6.0 Open tx; load current snapshots needed for `before_json`.
  - 6.1 AuditWriter inserts audit row for the lock (action = `lock`).
  - 6.2 Update visits row (status, snapshots, version, dirty).
  - 6.3 Insert each `inventory_adjustments` row for consumption (each preceded by its own audit row via nested `with_audit`).
  - 6.4 Recompute affected `inventory_items.quantity_on_hand` (each preceded by an `update` audit row).
  - 6.5 Render receipts (PDF + thermal) to a temporary buffer; if rendering fails, abort the tx.
  - 6.6 Persist receipt artifacts to `$APPDATA/.../receipts/...`.
  - 6.7 Enqueue outbox rows for visit, each adjustment, each item, each audit row.
  - 6.8 Commit.

### 7.11 Void workflow respects audit-first ordering
- **Gap:** HIGH | Wrong Order | PRD §4.3
- §4 void step 3.5 writes audit rows after mutations 3.1-3.4.
- **Resolution:** Restructure `VisitService::void` step 3 same as §7.10: audit row for visit-void inserted before the visit update; audit rows for each offsetting adjustment inserted before the inserts; recompute audit before the recompute write.

### 7.12 Operator-eligibility TOCTOU race
- **Gap:** HIGH | Missing Concurrency Guard | PRD §4.2
- Phase-05 §4 lock step 4 computes eligibility before the tx opens (step 6). An operator can clock out between check and commit.
- **Resolution:** Move eligibility re-validation into the tx. Lock step now reads:
  - Step 4 (pre-tx): compute eligibility set; UI uses it to pick the operator.
  - Step 6.1 (in-tx, before audit/mutation): re-run `operator_eligibility::qualified(check_type_id, dye, operator_id)` inside the same tx; reject `LockError::OperatorBecameIneligible` if the candidate no longer satisfies the predicates.

### 7.13 Patient-name validation at lock
- **Gap:** MEDIUM | Missing Validation | PRD §8.1
- Lock does not re-validate that the patient row has a non-empty `name`. Drafts created with a stub patient (since Patient::try_new is fixed in §7.9) could still arrive with whitespace-only names from sync.
- **Resolution:** Append to `VisitService::lock` step 1: `if patient.name.trim().is_empty() { return Err(LockError::PatientNameEmpty); }`.

### 7.14 Server-side `void_reason` re-validation
- **Gap:** MEDIUM | Missing Validation | PRD §8.2
- `VisitService::void(visit_id, reason, by_user_id)` step 1 trusts the client; minimum length is enforced only in `<VoidModal>`.
- **Resolution:** Add to step 1: `if reason.trim().chars().count() < 5 { return Err(VoidError::ReasonTooShort); }`. Server-side `accept_push` runs the same check.

### 7.15 `by_user_id` on void-offset adjustments
- **Gap:** MEDIUM | Missing Audit Write | PRD §6.1.14
- Schema has `by_user_id NOT NULL` on `inventory_adjustments`; void offset row writer never explicitly sets it.
- **Resolution:** Add to `VisitService::void` step 3.3: the offsetting positive-delta rows are written with `by_user_id = void_actor_id` (the superadmin invoking the void). Document explicitly in §4.

### 7.16 Receipt rendering before WAL-lock-holding writes
- **Gap:** MEDIUM | Missing Step | PRD §8.1 step 10
- The current lock step 6.6 renders receipts inside the tx, holding the WAL lock through disk I/O. CLAUDE.md flags this as a known pitfall.
- **Resolution:** Pre-rendering in step 5 (outside tx): build the receipt PDF + thermal text into an in-memory buffer once `LockArgs` are computed. Inside the tx (step 6), only write the artifacts to disk after all DB mutations succeed but before commit; if disk write fails, roll back. This trims WAL hold-time. Net flow:
  - 5. (out-of-tx) compute money_math, render receipt buffers.
  - 6.0-6.4 (in-tx) all DB writes.
  - 6.5 (in-tx) write receipt files to disk; on error, roll back tx, leaving file system clean (use a temp dir + atomic rename).
  - 6.6 Commit tx.

### 7.17 Snapshot includes human-readable names
- **Gap:** MEDIUM | Missing Snapshot | PRD §4.1
- Money values are snapshotted but `patient.name`, `doctor.name`, `operator.name`, `check_type.name_ar/en`, `check_subtype.name_ar/en` are NOT - reprinting after a rename shows different text from the original receipt.
- **Resolution:** Extend §1 schema with name-snapshot columns (nullable until status=locked, then enforced non-null):
  ```sql
  patient_name_snapshot TEXT,
  doctor_name_snapshot TEXT,
  operator_name_snapshot TEXT,
  check_type_name_ar_snapshot TEXT,
  check_type_name_en_snapshot TEXT,
  check_subtype_name_ar_snapshot TEXT,
  check_subtype_name_en_snapshot TEXT,
  ```
  Lock step 6 reads the rows and writes the snapshots. `ReceiptGenerator::render(visit)` reads exclusively from snapshot columns - never re-joins. Server Prisma mirrors.

### 7.18 Patient outbox enqueue when newly created at lock
- **Gap:** MEDIUM | Missing Outbox Enqueue | PRD §11.1
- `<NewVisitForm>` creates a patient inline via `patients::create`; that call enqueues the patient. Lock step 6.7 enqueues "visit, each adjustment, each item, audit rows" but the rule is unspoken for the inline-created patient.
- **Resolution:** Lock step 6.7 explicitly lists: visit, every newly-inserted patient row (if any drafted via `<NewVisitForm>`), every adjustment, every recomputed item row, every audit row. Each one corresponds to its own outbox entry. Document in §4 numbered list.

### 7.19 Void transition rejects re-void
- **Gap:** LOW | Missing Transition Rule | PRD §8.2
- `VisitService::void` step 1 checks `status = Locked`; the explicit reject for already-voided is implicit.
- **Resolution:** Re-state in §4 void:
  ```
  if visit.status != VisitStatus::Locked {
      return Err(VoidError::NotLocked { current: visit.status });
  }
  ```

### 7.20 `<ChecksGridCard>` sample subtype list
- **Gap:** HIGH | Missing UI Element | PRD §7.1.1
- Each check card must render a sample subtype list when the check has subtypes; §3.Frontend description is single-line.
- **Resolution:** Extend `<ChecksGridCard>` row in §3.Frontend table:
  > Renders name (locale-resolved) + today's count + (when `has_subtypes = 1`) a comma-separated list of up to 3 subtype names by FTS recent-usage order + "+N more" overflow. Click anywhere on the card navigates to `/reception/checks/:check-slug`.

### 7.21 `<WorkspaceVisitsTable>` columns/filters/pagination
- **Gap:** HIGH | Missing UI Element | PRD §7.1.2
- PRD §7.1.2 specifies 12 columns, 4 filters, default sort, 50-row cursor pagination. §3.Frontend describes none.
- **Resolution:** Replace `<WorkspaceVisitsTable>` row in §3.Frontend with:
  > Columns (left-to-right when LTR; right-to-left when RTL): `#`, `Created at`, `Patient`, `Subtype`, `Doctor`, `Operator`, `Dye`, `Report`, `Total IQD`, `Status` pill, `Pending sync` indicator (dot when `dirty = 1`), `Actions` (Open / Print / Void).
  > In-workspace filters: subtype (multi-select), doctor (autocomplete), status (`draft | locked | voided`), date (today / yesterday / custom range).
  > Default sort: `created_at DESC`.
  > Pagination: cursor-based, 50 rows per page, backed by `visits::list_workspace({ check_type_id, filters, cursor, limit: 50 })`.

### 7.22 `<ReceiptPreview>` component
- **Gap:** MEDIUM | Missing Component | roadmap §Engines
- Roadmap names `<ReceiptPreview>`; §3.Frontend components table omits it.
- **Resolution:** Add to §3.Frontend table:
  ```
  | <ReceiptPreview> | src/components/receipts/preview.tsx | Side-by-side A5 PDF embed + monospace thermal text preview. Shown in <VisitDetailReceiptsTab> and modal after a successful lock. Re-uses the rendered artifacts (no re-render). |
  ```

### 7.23 `receipts::print` IPC and printer routing
- **Gap:** HIGH | Missing IPC Command | PRD §8.1 step 12
- §3.Tauri exposes `receipts::reprint` (re-render) but no command opens the print dialog or sends thermal text to a configured printer. The current §4 frontend step 8.4 routes thermal through `tauri-plugin-dialog` save-as, contradicting PRD §8.1.
- **Resolution:** Add to §3.Tauri commands table:
  ```
  | receipts::print_pdf      | { visit_id }                 | () | Opens the OS print dialog with the persisted A5 PDF via tauri-plugin-shell. |
  | receipts::print_thermal  | { visit_id }                 | () | Sends the thermal text to the printer named in settings.thermal_printer_name (new setting). |
  | settings::list_printers  | ()                            | Vec<PrinterInfo> | OS printer enumeration via tauri-plugin-shell. |
  ```
  Add settings keys `thermal_printer_name` (default `null`; null → user prompt at first print) seeded by phase-02 §7.1 update. Document in §4 ReceiptGenerator.

### 7.24 Read-only `<VisitDetail>` mode for accountant drill-down
- **Gap:** MEDIUM | Missing Behavior | phase-07
- Phase-07 says `<VisitDetail>` is reused "in read-only mode" by accountant; phase-05 declares no read-only prop.
- **Resolution:** Extend `<VisitDetail>` (and child tab components) with prop `mode: 'edit' | 'readonly'`. When `readonly`: Actions row is hidden, Receipts tab shows print buttons (always allowed), Audit tab is unchanged. The page route file derives the mode from `useCurrentUser().role` and the URL prefix (`/accounting/visits/:id` → readonly).

### 7.25 Lines-run wiring for `<ShiftHistoryToday>`
- **Gap:** MEDIUM | Incomplete Coverage | PRD §7.1.5
- Phase-04 ships `<ShiftHistoryToday>` with the Lines-run column as `0` placeholder; phase-05 owns visits but never wires the count.
- **Resolution:** Add IPC `shifts::lines_run_today(operator_id) -> u32` (returns count of non-deleted visits where `operator_id = ?` AND `date(locked_at) = today`). Update `<ShiftHistoryToday>` (phase-04) to call `useLinesRunToday(operator_id)` on each row. Cross-reference in phase-04 §7.7.

### 7.26 Operator-eligibility error-code naming alignment
- **Gap:** MEDIUM | Missing Logic | PRD §4.2
- §4 lock step distinguishes `LockError::NoQualifiedOperator` and `LockError::OperatorNotQualified`; PRD names only `OperatorAttribution::NoQualifiedOperator`.
- **Resolution:** Adopt the union `LockError::OperatorIneligible { reason: NoQualifiedOperator | NotQualifiedForCheck | NoOpenShift | OperatorBecameIneligible }`. i18n keys live under `errors:operator.ineligible.<reason>`; both Tauri and server use the same union (serde tagged).

### 7.27 Pricing-change banner on active drafts
- **Gap:** HIGH | Missing Logic | PRD §8.5
- PRD §8.5 step 3 mandates a "prices updated - refresh totals?" banner on drafts when pricing changes; no phase declares it.
- **Resolution:** Add to §3.Frontend:
  - `<PricingChangedBanner>` component, mounted inside `<NewVisitForm>` and `<CheckWorkspace>` for any draft row.
  - Triggered by Tauri event `catalog:pricing_changed` (emitted by phase-03 `CheckTypeService`, `CheckSubtypeService`, `DoctorPricingService` after each mutation).
  - Banner provides a "Recalculate" button that re-fetches `pricing::resolve(visit_id)` (new IPC; runs the algorithm and returns the new totals) and updates the draft.

### 7.28 "Recalculate draft" button on `<NewVisitForm>`
- **Gap:** MEDIUM | Missing Logic | PRD §8.5
- PRD §8.5 Business Rules requires a Recalculate button on the new-visit form distinct from the banner trigger.
- **Resolution:** Add to `<NewVisitForm>` row in §3.Frontend table:
  > Footer action: `[Recalculate]` button (always enabled; on click → `pricing::resolve(visit_id)` → updates `<RunningTotalSummary>` and bumps the draft's `version`). The banner from §7.27 also routes here.

### 7.29 Per-row dirty indicator across reception/inventory/audit
- **Gap:** HIGH | Missing UI Element | PRD §10.8
- PRD §10.8 requires per-row pending-sync indicators (small dot, tooltip "Pending sync") wherever `dirty = 1`.
- **Resolution:** Add to §3.Frontend Setup:
  ```
  | <DirtyDot> | src/components/sync/dirty-dot.tsx | Tiny 6px dot with tooltip "Pending sync". Receives a boolean prop; respects RTL alignment. |
  ```
  `<WorkspaceVisitsTable>`, `<InventoryItemsTable>` (phase-06), `<AuditTable>` (phase-08) all add a Pending-sync column rendering `<DirtyDot dirty={row.dirty === 1} />`. The IPC list responses must include the `dirty` field on every row.

### 7.30 RTL receipt rendering tested
- **Gap:** MEDIUM | Missing Setup | PRD §10.3
- §6 Verification step 8 asserts file persistence but not RTL mirroring.
- **Resolution:** Add verification step:
  > 12. Switch language to `ar`; lock a synthetic visit; assert the rendered A5 PDF has the clinic name and headers on the right edge and the totals column on the left; assert the thermal text is RTL-aligned in a monospace 32/48-col grid. Snapshot test under `src-tauri/tests/receipts/rtl_snapshot.rs`.

### 7.31 Visit `discard` service contract
- **Gap:** HIGH | Missing State Transition | PRD §6.1.10
- PRD §6.1.10 transition table allows `draft -> (deleted)` only from `draft`. §4 `VisitService::discard` is unspecified.
- **Resolution:** Add to §4:
  ```
  VisitService::discard(visit_id, by_user_id):
    1. Load visit; if visit.status != VisitStatus::Draft → Err(VisitError::DiscardNotDraft { current: visit.status }).
    2. with_audit('soft_delete','visits', id, delta={ reason: 'discard' }).
    3. UPDATE visits SET deleted_at = now, version = version + 1, dirty = 1, updated_at = now WHERE id = ?.
    4. Enqueue outbox under manual conflict policy.
  ```
  Server `VisitService::acceptPush` rejects a soft-delete push whose `existing.status != 'draft'` with `422 visit_discard_not_draft` (locked/voided rows MUST NOT be soft-deleted; void is the only terminal).

### 7.32 Visit illegal-transition matrix
- **Gap:** CRITICAL | Missing State Transition | PRD §6.1.10
- PRD §6.1.10 lists 5 legal transitions. The illegal set is not explicitly blocked anywhere.
- **Resolution:** Add to §4 a `VisitService::assert_transition(from: VisitStatus, to: VisitStatus) -> Result<(), VisitError>` helper:
  ```rust
  match (from, to) {
      (Draft, Locked)  => Ok(()),
      (Draft, Draft)   => Ok(()),  // field edits within draft
      (Locked, Voided) => Ok(()),
      _                => Err(VisitError::IllegalTransition { from, to }),
  }
  ```
  Illegal cases: `locked -> draft` (NoUnlock), `locked -> locked` (AlreadyLocked), `voided -> *` (Terminal), `draft -> voided` (MustLockFirst). Every mutator (`update`, `lock`, `void`, `discard`) calls `assert_transition` at the top. Server `VisitService::acceptPush` runs the same check on `(existing.status, incoming.status)`.

### 7.33 `inventory_adjustments` immutability trigger
- **Gap:** CRITICAL | Missing CHECK | PRD §6.1.14 inv 1
- PRD §6.1.14 inv 1: "Adjustments are never edited or hard-deleted." Schema allows arbitrary UPDATE.
- **Resolution:** Add to §1 migration `005_reception.sql`:
  ```sql
  CREATE TRIGGER inventory_adjustments_no_update
  BEFORE UPDATE ON inventory_adjustments
  FOR EACH ROW
  WHEN OLD.delta != NEW.delta
    OR OLD.reason != NEW.reason
    OR COALESCE(OLD.visit_id,'') != COALESCE(NEW.visit_id,'')
    OR OLD.item_id != NEW.item_id
    OR OLD.by_user_id != NEW.by_user_id
    OR (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
  BEGIN
    SELECT RAISE(ABORT, 'inventory_adjustments are append-only');
  END;
  ```
  Updates to sync metadata (`version`, `dirty`, `last_synced_at`, `origin_device_id`) remain allowed because they are not in the predicate. Server-side: add Postgres trigger `inventory_adjustments_no_update_pg` with the same rule in a Prisma raw migration.

### 7.34 Patient soft-delete in-use guard
- **Gap:** HIGH | Missing Validation | PRD §6.1.9
- Soft-deleting a patient with non-deleted visits orphans the visit FK.
- **Resolution:** Add to §4 `PatientService::soft_delete(id)`:
  ```
  1. SELECT count(*) FROM visits WHERE patient_id = ? AND deleted_at IS NULL.
  2. If count > 0 → Err(PatientError::ReferencedByVisits).
  3. with_audit('soft_delete','patients', id).
  4. UPDATE patients SET deleted_at = now, version = version + 1, dirty = 1, updated_at = now WHERE id = ?.
  ```
  Surfaced in admin UI as a toast `errors:patient.referenced`. Server-side `PatientService::acceptPush` re-validates.

### 7.35 Patient FTS recency filter (last 30 days)
- **Gap:** MEDIUM | Missing Logic | PRD §7.1.3
- PRD §7.1.3 says FTS5 search backs autocompletion of recent patients (last 30 days). `patients::search` has no recency filter.
- **Resolution:** Add to §3 Tauri command signature: `patients::search { query: String, limit: u32, since_days: Option<u32> } -> Vec<PatientSummary>` defaulting `since_days = 30`. SQL:
  ```sql
  SELECT p.* FROM patients_fts f
  JOIN patients p ON p.rowid = f.rowid
  WHERE patients_fts MATCH ?
    AND p.deleted_at IS NULL
    AND EXISTS (
      SELECT 1 FROM visits v
      WHERE v.patient_id = p.id
        AND v.created_at >= datetime('now', '-' || :since_days || ' days')
    )
  ORDER BY p.updated_at DESC LIMIT ?;
  ```
  Add `CREATE INDEX patients_recent ON patients(entity_id, updated_at DESC) WHERE deleted_at IS NULL` to §1.

### 7.36 `inventory_adjustments` push immutability (server)
- **Gap:** HIGH | Missing Sync Rule | PRD §6.1.14
- §4 declares `additive-only` but the server `InventoryAdjustmentService::acceptPush` (declared in phase-06 §7.3) uses Prisma `upsert`, allowing a peer to silently mutate an immutable row.
- **Resolution:** Append to §4 sync semantics row for `inventory_adjustments`: "Server `InventoryAdjustmentService::acceptPush` MUST use `prisma.inventoryAdjustment.create` (NOT upsert). If a row with the same `id` already exists, check `ProcessedOp.has(op_id)`: hit returns the cached response (idempotent replay); miss returns `409 ADDITIVE_VIOLATION` (the row is append-only; deltas cannot be rewritten by a peer)." Phase-06 §7.3 amended accordingly. Also add explicit Prisma indexes mirroring local: `@@index([entityId, itemId, createdAt])` and `@@index([entityId, visitId])`.

### 7.37 Audit-action closed enum for visit lock/void/discard
- **Gap:** MEDIUM | Missing Audit | PRD §6.1.15
- §4 writes `with_audit('lock', ...)`, `with_audit('void', ...)`, `with_audit('soft_delete', ..., entity='visits')` for discard. The closed enum is documented (phase-01 §7.8) but phase-05 lacks an explicit re-statement.
- **Resolution:** Append to §5 Infrastructure: "Audit action union extended in this phase: `lock` and `void` actions are written by `VisitService::lock` and `VisitService::void`. The 12-value closed union from phase-01 §7.8 already includes them; no schema CHECK is added (audit.action remains free TEXT, application-enforced via AuditAction::from_str). The 90-day pruner (phase-08 §4) is the SOLE writer permitted to set `deleted_at` on `audit_log`; its rows do NOT get audited (would create infinite recursion). Pruner skips rows with `dirty=1`."

### 7.38 `<LockValidationErrors>` and `visits::lock_dryrun` IPC
- **Gap:** MEDIUM | Missing UI Element | PRD §7.1.3
- PRD §7.1.3 says Lock-validation failure shows an "inline list of unmet requirements (no operator on shift for this check; subtype missing)". No phase declares the UI or the dryrun command.
- **Resolution:** Add to §3 Tauri commands: `visits::lock_dryrun { visit_id: Uuid } -> Vec<LockBlocker>` where `LockBlocker` is a tagged union:
  ```rust
  pub enum LockBlocker {
      SubtypeMissing,
      NoShiftForCheckType { check_type_id: Uuid },
      DyeNotSupported,
      ReportNotSupported,
      PatientNameEmpty,
      NoQualifiedOperator { check_type_id: Uuid },
      DraftStale,
  }
  ```
  Add `<LockValidationErrors>` to §3 Frontend components: `src/components/reception/lock-validation-errors.tsx` renders bullets keyed by `errors:visit.*` / `errors:operator.*` messages above the Lock button. Hook into `<NewVisitForm>`: calls `visits::lock_dryrun` debounced 300ms on field changes; Lock button is disabled while the returned vec is non-empty.

### 7.39 Visit `created_at` immutability on lock/void
- **Gap:** LOW | Missing CHECK | PRD §6.1.10
- PRD §6.1.10 implicitly treats `created_at` as the booking timestamp consumed by receipts and reports. `lock` and `void` must not touch it.
- **Resolution:** Append to §4 `Visit::lock` and `Visit::void` step lists: "Preserve `self.created_at` unchanged. Only `updated_at`, `locked_at`, `voided_at`, snapshot/status fields, and `version` mutate." Add §6 verification: snapshot `created_at` before lock, lock the visit, assert post-lock `created_at` unchanged.

### 7.40 LWW tiebreak re-stated for `patients`; `visits` manual semantics
- **Gap:** HIGH | Missing Tiebreak | research.md
- §4 Sync Semantics row `patients | last-write-wins | op_id` has empty Notes. No tiebreak.
- **Resolution:** Append to §4 Sync Semantics:
  - `patients`: "LWW tiebreak: when `updated_at` matches to the millisecond, the row with the lexicographically smaller `origin_device_id` wins (global rule from phase-01 §4 SyncEngine)."
  - `visits`: "Server `accept_push` algorithm: (1) ProcessedOp.has(op_id) → return cached. (2) Load existing by id. (3) If absent → INSERT (normal draft creation). (4) Compare pushed.version to existing.version: if pushed.version < existing.version, park conflict; if equal AND snapshots differ, park conflict; if pushed.version > existing.version, accept (most-recent legal transition); else idempotent no-op. Tiebreak on equal `version` with identical snapshots: idempotent no-op via ProcessedOp."

### 7.41 Additive-entity listing order (composite key)
- **Gap:** MEDIUM | Missing Tiebreak | research.md
- For additive entities (`inventory_adjustments`, `operator_shifts`, `audit_log`) two devices may generate rows with identical `created_at`. List order in `<ItemAdjustmentsList>`, `<ShiftHistoryToday>`, `<AuditTable>` would be non-deterministic.
- **Resolution:** Document the additive-entity listing order globally: `ORDER BY created_at ASC, origin_device_id ASC, id ASC` (3-tuple composite for total stability). Documented once here; re-stated in phase-01 §4 SyncPullService and consumed by phase-06 / phase-08. Add to §1 the index `CREATE INDEX inventory_adjustments_chrono ON inventory_adjustments(entity_id, item_id, created_at, origin_device_id, id)` for fast totally-stable scans.

### 7.42 `<SettingsChangedBanner>` declared (phase-05)
- **Gap:** HIGH | Orphaned Component | phase-02 §7.4
- Phase-02 §7.4 says `<NewVisitForm>` renders `<SettingsChangedBanner>` on `settings:changed` events. Phase-05 §7 had no receipt.
- **Resolution:** Add to §3 Frontend components:
  ```
  | <SettingsChangedBanner> | src/components/reception/settings-changed-banner.tsx | Banner inside <NewVisitForm> shown when a `settings:changed` event arrives while a draft is open. Action button "Recalculate" dispatches `pricing::resolve(visit_id)`. Dismissable until the next event. |
  ```
  Subscribes via `useEffect(() => { listen('settings:changed', () => setVisible(true)); }, [])`. Identical mechanism as `<PricingChangedBanner>` from §7.27; both share a `<DraftStaleBanner>` base. i18n keys: `reception.banner.settings_changed.{title,recalculate,dismiss}`.

### 7.43 `pricing::resolve` IPC declared
- **Gap:** HIGH | Orphaned IPC | §7.27, §7.28
- Both §7.27 (PricingChangedBanner) and §7.28 (Recalculate button) reference `pricing::resolve(visit_id)`. Not in §3 commands.
- **Resolution:** Add to §3 Tauri commands: `pricing::resolve | { visit_id: Uuid } | VisitSnapshots | Re-runs money_math::compute against current settings/pricing for the named draft; returns the freshly computed snapshot block without mutating the visit row. Caller (the Recalculate button) decides whether to write the result to the draft.` Algorithm: load visit, load current effective_price via phase-03 §7.26, load current settings, run `MoneyMath::compute`, return the new snapshots. Read-only.

### 7.44 `visits::list_workspace` IPC declared
- **Gap:** HIGH | Orphaned IPC | §7.21
- §7.21 pagination spec referenced `visits::list_workspace({check_type_id, filters, cursor, limit: 50})` but the IPC was nowhere declared.
- **Resolution:** Add to §3 Tauri commands:
  ```
  visits::list_workspace |
    { check_type_id: Uuid, filters: WorkspaceFilters, cursor: Option<String>, limit: u32 } |
    { rows: Vec<VisitSummary>, next_cursor: Option<String> } |
    Workspace listing with subtype/doctor/status/date filters; cursor encodes (created_at,id). Reads include the `dirty` flag for §7.29.
  ```
  `WorkspaceFilters = { subtype_ids: Vec<Uuid>, doctor_ids: Vec<Uuid>, statuses: Vec<VisitStatus>, date_range: Option<(DateTime<Utc>, DateTime<Utc>)> }`. SQL uses the composite cursor `(created_at, id)` and `ORDER BY created_at DESC, id DESC` then reverses for ascending pages; `LIMIT :limit + 1` to detect more.

### 7.45 Printer-enumeration capability and `tauri-plugin-shell`
- **Gap:** HIGH | Missing Capability | §7.23
- §7.23 introduced `settings::list_printers` via shell commands but never registered the plugin or scoped capabilities; phase-02 §7.1 (now extended) seeds `thermal_printer_name`.
- **Resolution:** Add to §5 Infrastructure: register `tauri-plugin-shell` in `src-tauri/src/lib.rs`. Add to `src-tauri/capabilities/main.json`:
  ```json
  { "identifier": "shell:allow-execute", "allow": [
      { "name": "lpstat",  "cmd": "lpstat", "args": ["-p"] },
      { "name": "wmic",    "cmd": "wmic",   "args": ["printer","get","name"] }
  ]}
  ```
  Implementation chooses the platform-appropriate command at runtime. Output parsed into a `Vec<PrinterInfo>` and returned by `settings::list_printers`. `<SettingsForm>` Combobox consumes it. The empty string in `thermal_printer_name` means "OS default printer".

### 7.46 Rust receipt arabic-numerals formatter
- **Gap:** HIGH | Missing i18n Key | PRD §10.6
- Phase-02 §7.12 added `formatIqd`/`formatInt` JS helpers, but `ReceiptGenerator::render` runs in Rust and cannot call frontend helpers; receipts would render Western digits regardless of the setting.
- **Resolution:** Add to §4 Rust a `numerals` module:
  ```rust
  pub fn format_iqd(amount_iqd: i64, locale: &str, arabic_digits: bool) -> String;
  pub fn format_int(n: i64, arabic_digits: bool) -> String;
  ```
  Reading `settings.arabic_numerals` from the `settings_cache`. Uses a simple ASCII-digit -> Arabic-Indic-digit lookup map (`٠١٢٣٤٥٦٧٨٩`). Both A5 PDF and thermal-text receipts consume the helper exclusively. Snapshot tests assert digit set matches the setting for both locales.

### 7.47 Keyboard-only contracts for lock/void/discard modals
- **Gap:** HIGH | Missing A11y | PRD §10.7
- PRD §10.7 requires keyboard nav. §3 Frontend declares Lock, Void modal, Discard confirm without keyboard contracts.
- **Resolution:** Add to §3 Frontend the contracts:
  - `<NewVisitForm>`: Enter on the last input does NOT submit; the Lock button is reached via Tab; Enter/Space activates. Hotkey `Ctrl/Cmd+Enter` triggers Lock when validation passes.
  - `<VoidModal>` and `<DiscardConfirm>`: shadcn `<Dialog>` with focus trap, Escape-to-cancel, autofocus on the reason textarea, primary button reachable via Tab.
  - `<OperatorPickerDialog>`: arrow-key navigation through the candidate list; Enter selects; Esc cancels; first item autofocused on open.
  Phase-08 a11y sweep verifies axe-core `focus-order-semantics` and `focus-trap` rules pass on all three modals.

### 7.48 `<OperatorPickerDialog>` empty-set surfacing
- **Gap:** MEDIUM | Missing UI Element | §7.26
- PRD §7.1.3: "Operator picker loaded from currently-clocked-in operators with `operator_specialties` covering the workspace's check type." §3 does not specify zero-candidate behavior.
- **Resolution:** Append to §3 Frontend `<OperatorPickerDialog>`: "When `useQualifiedOperators(checkTypeId)` returns `[]`, the dialog does not open. Instead an inline `<LockBlockedToast>` shows `errors:operator.ineligible.no_qualified` with a 'Manage shifts' link to `/reception/shifts`. The Lock button remains disabled with `aria-describedby` pointing at the toast." Hooks into §7.38 dryrun: `LockBlocker::NoQualifiedOperator` triggers the same toast.

### 7.49 Recalculate-flow verification
- **Gap:** MEDIUM | Missing Verification | §6
- §7.27/§7.28/§7.42/§7.43 introduce banners, `pricing::resolve`, and the Recalculate button. §6 has no step exercising them.
- **Resolution:** Add §6 verification:
  > 13. Recalculate flow: log in as superadmin, open `/admin/pricing`, edit a doctor's `cut_value` for `check_type_id = X`. As an open-draft-on-X owner (separate user session), assert `<PricingChangedBanner>` renders within 1s; click Recalculate; assert the new computed total reflects the changed cut. Repeat for `<SettingsChangedBanner>` driven by a `settings:changed` event (edit `dye_cost_iqd`).

### 7.50 Lines-run wiring delivery confirmation
- **Gap:** LOW | Missing Handshake | phase-04 §7.7
- Phase-04 §7.7 defers Lines-run column to phase-05. Phase-05 §7.25 already added the IPC; cross-confirm the wiring is complete.
- **Resolution:** Append to §7.25 (and re-state here): `<ShiftHistoryToday>` consumes `shifts::lines_run_today(operator_id)` lazily per row (one IPC call per visible row, deduped via React Query keys `['shifts','lines_run', operator_id]`). Cache TTL 30s; invalidated on `visits:locked` event (new Tauri event emitted by `VisitService::lock` after commit). Phase-04 row in §3 Frontend table updated from "placeholder 0" to "live count via this hook".

### 7.51 Raw-SQL migration ordering mirror
- **Gap:** HIGH | Missing Migration Spec | Pass-3 GAP-A-1; §7.33, §7.36
- §7.33 adds the `inventory_adjustments_no_update_pg` trigger as raw SQL on the server. Same operational ordering question as phase-03 §7.31.
- **Resolution:** Append to Section 5 Infrastructure: "Raw-SQL migrations from §7.33 (and any future §7.x adding triggers/CHECKs) ship as `prisma migrate dev --create-only` files under `sync-server/prisma/migrations/`. Filename prefix `005_` matches the owning phase epoch. Order: (a) ensure target table exists from prior `prisma migrate` of the model; (b) trigger / CHECK addition in its own migration. Verification: `pnpm prisma migrate status` clean after merge. Cross-reference phase-03 §7.31 for the parallel rule."

### 7.52 `pulledAt` + name-snapshot columns on Patient/Visit/InventoryAdjustment Prisma
- **Gap:** HIGH | Missing Field | Pass-3 GAP-A-6 part 2 + GAP-C-3; PRD line 302; §7.17
- (1) PRD line 302 requires `pulledAt` on every server Prisma model; §2 `Patient`, `Visit`, `InventoryAdjustment` lack it. (2) §7.17 added 7 name-snapshot columns to local SQLite `visits`; the server `Visit` model never received them, so pulled rows would round-trip without snapshot names.
- **Resolution:** Add to §2:
  ```prisma
  // model Patient
  pulledAt DateTime? @map("pulled_at") @db.Timestamptz

  // model InventoryAdjustment
  pulledAt DateTime? @map("pulled_at") @db.Timestamptz

  // model Visit
  pulledAt                     DateTime? @map("pulled_at") @db.Timestamptz
  patientNameSnapshot          String?   @map("patient_name_snapshot")
  doctorNameSnapshot           String?   @map("doctor_name_snapshot")
  operatorNameSnapshot         String?   @map("operator_name_snapshot")
  checkTypeNameArSnapshot      String?   @map("check_type_name_ar_snapshot")
  checkTypeNameEnSnapshot      String?   @map("check_type_name_en_snapshot")
  checkSubtypeNameArSnapshot   String?   @map("check_subtype_name_ar_snapshot")
  checkSubtypeNameEnSnapshot   String?   @map("check_subtype_name_en_snapshot")
  ```
  TypeBox `VisitPushSchema` (§7.6) extended with the same 7 string-or-null fields.

### 7.53 Visit name-snapshot CHECK extension + discard error code reconcile
- **Gap:** HIGH/LOW | Missing Constraint + Inconsistent Error Code | Pass-3 GAP-C-3 + GAP-C-8; §1, §7.31
- (a) §7.17 declared the name-snapshot columns "nullable until status=locked, then enforced non-null" but the §1 CHECK was never extended. (b) §7.31 declares the server returns `422 visit_discard_not_draft` for state-machine violations on discard; phase-01 §7.32 uses `409 ConflictParked` for state-machine violations. Mixed.
- **Resolution:**
  1. Extend §1 visits CHECK with a status='locked' clause:
     ```sql
     CHECK (
       status != 'locked' OR (
         patient_name_snapshot IS NOT NULL
         AND check_type_name_ar_snapshot IS NOT NULL
         AND operator_name_snapshot IS NOT NULL
         AND ((doctor_id IS NULL AND doctor_name_snapshot IS NULL)
           OR (doctor_id IS NOT NULL AND doctor_name_snapshot IS NOT NULL))
         AND ((check_subtype_id IS NULL AND check_subtype_name_ar_snapshot IS NULL)
           OR (check_subtype_id IS NOT NULL AND check_subtype_name_ar_snapshot IS NOT NULL))
       )
     )
     ```
     The `_name_en_snapshot` columns remain nullable since `name_en` source is nullable in catalog.
  2. Reconcile §7.31 server error: change to `409 ILLEGAL_VISIT_TRANSITION` with detail `{ from: existing.status, to: 'discarded' }` matching the §7.32 helper, instead of `422 visit_discard_not_draft`. i18n key `errors:visit.illegal_transition` (already in phase-03 §7.29 inventory).

### 7.54 Lock-time + receipt-print telemetry emission
- **Gap:** HIGH | Missing Telemetry | phase-01 §7.28; Pass-3 GAP-F-1; PRD §1.3
- Phase-01 §7.28 declared `metrics_events` and stated "Phase-05 lock service emits `lock_start`/`lock_end`" plus `receipt_print_ok`/`receipt_print_fail`. §4 `VisitService::lock` and `ReceiptGenerator` are silent on writing them. PRD §1.3 lock p95 (<30s) and receipt-print success (>99%) cannot be measured.
- **Resolution:** Append to §4 Tauri:
  - `VisitService::lock` step 0 (before tx open): `metrics_events { kind:'lock_start', payload:{ visit_id } }`. Step N (after commit): `kind:'lock_end', payload:{ visit_id, duration_ms, blocked:false }`. On any early validation failure: `kind:'lock_end', payload:{ visit_id, duration_ms, blocked:true, reason:<error_code> }`.
  - `ReceiptGenerator::render_pdf` and `::render_thermal` write `kind:'receipt_print_ok'` / `'receipt_print_fail'` keyed by `{ visit_id, format:'pdf'|'thermal', error?:string }`. Failures captured from save-as / shell errors.
  All writes are non-syncable, share the WAL pool, and use the same retention as phase-08 §7.21. Cross-reference phase-08 §7.16 soak harness.

### 7.55 Document Center deferral receipt
- **Gap:** LOW | Inconsistency | PRD §5.4; Pass-3 GAP-F-4
- PRD §5.4 says "no upload to Document Center service in v1; Horizon-1 introduces centralized receipt archive." §5 BullMQ paragraph mentions the deferral inline; the "What this phase does NOT touch" subsection does not.
- **Resolution:** Append to §5 "What this phase does NOT touch" bullets: "No Document Center upload -- receipts are local-only artifacts under `$APPDATA/idc-system/receipts/{yyyy}/{mm}/{visit_id}.{pdf,txt}`. Horizon-1 introduces the centralized receipt archive (PRD §5.4)."

### 7.56 Reception module `handle.crumb` declarations
- **Gap:** MEDIUM | Missing Handshake | phase-01 §7.13, phase-08 §7.20; Pass-3 GAP-E-10
- Phase-08 §7.20 catalogues "Phase-05 §3 owns reception/* and visits/*" handle.crumb but §3 routing block does not declare them.
- **Resolution:** Append to §3 Frontend routing block: each reception/visit route exports a `handle.crumb` thunk:
  - `/reception/checks/:slug` -> `handle: { crumb: ({ data }) => resolveLocaleName(data.checkType) }`
  - `/reception/checks/:slug/new` -> `handle: { crumb: () => t('reception.crumb.new_visit') }`
  - `/reception/visits/:id` -> `handle: { crumb: ({ data }) => data.patient_name_snapshot ?? t('reception.crumb.visit') }`
  Resolved by phase-01 §7.13 `<Breadcrumbs>`. `resolveLocaleName` from phase-03 §7.16. New i18n keys `reception.crumb.new_visit`, `reception.crumb.visit`.

### 7.57 Reception UI elements: header link, workspace headers, action bars, void/reprint buttons
- **Gap:** MEDIUM/LOW | Missing UI Element | PRD §7.1.1-§7.1.4; Pass-3 GAP-E-1, E-2, E-3, E-4
- PRD ASCII layouts enumerate UI affordances that §3 omits: the Checks Grid header `[Operator shifts]` link; Workspace and New-Visit header back-links; New Visit `[Save draft]` / `[Discard]` / `[Lock & print]` action bar; Visit Detail superadmin `<VoidButton>` trigger.
- **Resolution:** Add to §3 Frontend components table:
  - `<ChecksGridHeader>` -- top-right `Operator shifts` link to `/reception/shifts` (i18n `reception.checks_grid.operator_shifts`).
  - `<WorkspaceHeader>` -- back-link to `/reception` + active check name (i18n `reception.workspace.back_to_grid`).
  - `<NewVisitHeader>` -- back-link to `/reception/checks/:slug` (i18n `reception.new_visit.back_to_workspace`).
  - `<NewVisitActionsBar>` -- three buttons: `Save draft` (calls `visits::create_draft`/`update_draft`), `Discard` (opens `<DiscardConfirm>`), `Lock & print` (existing flow). `Save draft` is explicit even with implicit auto-save.
  - `<VoidButton>` -- rendered inside `<VisitDetailDetailsTab>` only when `useCurrentUser().role === 'superadmin'` AND `visit.status === 'locked'` AND not in read-only mode (§7.24); opens `<VoidModal>` (existing).
  All wired per the PRD ASCII layouts.

### 7.58 `/reception/*` and `/inventory/*` route role gates
- **Gap:** HIGH | Missing Role Guard | PRD navigation tree §3.1; Pass-3 GAP-E-7
- PRD restricts `/reception/*` to `receptionist, superadmin`. No phase declared the wrapper. (Inventory wrapper is symmetric -- owned by phase-06 §7.13.)
- **Resolution:** Append to §3 Frontend routing block: "The `/reception/*` outlet is wrapped in `<RequireRole roles={['receptionist','superadmin']}>` (component from phase-02 §7.8). `/reception/shifts` (phase-04) inherits this wrapper. Non-matching role redirects to `/no-access`. `<UserMenu>` hides the Reception link based on the same role check."
