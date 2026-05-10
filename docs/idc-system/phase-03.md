# Phase 3: Reference Data & Admin CRUD

**Goal:** Land all eight reference-data entities (`check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `settings`, `patients`) end-to-end — local schema, server schema, sync round-trip, admin CRUD UI, FTS5 search — so later phases have catalog data to attach visits, shifts, and inventory to.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 2.
**Complexity:** XL
**PRD references:** §6.1.2-§6.1.9, §6.1.11 (settings), §7.4 (Admin module), §10.1 (FTS5).
**Decisions consumed:** D-001 (one doctor row + pricing join), D-005 (house = empty), D-007 (XOR has_subtypes), D-016 (sync conflict policies), D-020 (patient name only), D-022 (shadcn baseline + select/popover/command), D-027 (FTS5 vs LIKE).

---

## Section 1: Local Schema Changes (Tauri SQLite)

Eight migrations; one per entity. All schemas are verbatim from PRD §6.1; this section reproduces them for copy-paste. Tenant column `entity_id` is required on every row; sync columns from `offline-first.md` are present.

### Migration `004_check_types.sql` (PRD §6.1.2)

```sql
CREATE TABLE IF NOT EXISTS check_types (
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

### Migration `005_check_subtypes.sql` (PRD §6.1.3)

```sql
CREATE TABLE IF NOT EXISTS check_subtypes (
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

### Migration `006_doctors.sql` (PRD §6.1.4)

```sql
CREATE TABLE IF NOT EXISTS doctors (
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

-- FTS5 mirror for doctor name + specialty search.
CREATE VIRTUAL TABLE IF NOT EXISTS doctors_fts USING fts5(
  doctor_id UNINDEXED,
  name,
  specialty,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- Triggers keep FTS5 in sync.
CREATE TRIGGER IF NOT EXISTS doctors_ai AFTER INSERT ON doctors BEGIN
  INSERT INTO doctors_fts(doctor_id, name, specialty) VALUES (NEW.id, NEW.name, COALESCE(NEW.specialty,''));
END;
CREATE TRIGGER IF NOT EXISTS doctors_au AFTER UPDATE ON doctors BEGIN
  UPDATE doctors_fts SET name = NEW.name, specialty = COALESCE(NEW.specialty,'') WHERE doctor_id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS doctors_ad AFTER DELETE ON doctors BEGIN
  DELETE FROM doctors_fts WHERE doctor_id = OLD.id;
END;
```

### Migration `007_doctor_check_pricing.sql` (PRD §6.1.5)

```sql
CREATE TABLE IF NOT EXISTS doctor_check_pricing (
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

### Migration `008_operators.sql` (PRD §6.1.6)

```sql
CREATE TABLE IF NOT EXISTS operators (
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

### Migration `009_operator_specialties.sql` (PRD §6.1.7)

```sql
CREATE TABLE IF NOT EXISTS operator_specialties (
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

### Migration `010_settings.sql` (PRD §6.1.11)

```sql
CREATE TABLE IF NOT EXISTS settings (
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

Required keys seeded by `010_settings_seed.sql` (all 8 from PRD §6.1.11): `dye_cost_iqd`, `report_cost_iqd`, `internal_doctor_pct`, `idle_lock_minutes` (default 10), `arabic_numerals` (default `false` stored as `'0'`), `clinic_display_name_ar`, `clinic_display_name_en` (optional; can be empty string), `currency_symbol` (default `د.ع`). The seed migration is idempotent: `INSERT OR IGNORE` per key.

Bool parsing convention for `value_type='bool'`: stored value is `'0'` or `'1'`; `SettingsService::parse_bool` accepts both `'0'/'1'` and `'false'/'true'` for forward compatibility, normalizes on write to `'0'/'1'`.

### Migration `011_patients.sql` (PRD §6.1.9)

```sql
CREATE TABLE IF NOT EXISTS patients (
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

CREATE VIRTUAL TABLE IF NOT EXISTS patients_fts USING fts5(
  patient_id UNINDEXED,
  name,
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS patients_ai AFTER INSERT ON patients BEGIN
  INSERT INTO patients_fts(patient_id, name) VALUES (NEW.id, NEW.name);
END;
CREATE TRIGGER IF NOT EXISTS patients_au AFTER UPDATE ON patients BEGIN
  UPDATE patients_fts SET name = NEW.name WHERE patient_id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS patients_ad AFTER DELETE ON patients BEGIN
  DELETE FROM patients_fts WHERE patient_id = OLD.id;
END;
```

### What this phase does NOT touch (schema)
No `operator_shifts` (P4), `visits` (P5), inventory tables (P6).

---

## Section 2: Server Schema Changes (Prisma / Postgres)

Eight Prisma models added. All match PRD §6.1.2-§6.1.9 / §6.1.11. All become TENANT_MODELS.

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

  checkType       CheckType            @relation(fields: [checkTypeId], references: [id])
  doctorPricings  DoctorCheckPricing[]

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

  pricings DoctorCheckPricing[]

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

  doctor       Doctor        @relation(fields: [doctorId], references: [id])
  checkType    CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])

  @@unique([doctorId, checkTypeId, checkSubtypeId])
  @@map("doctor_check_pricing")
}

enum CutKind {
  pct
  fixed
}

model Operator {
  id                  String    @id
  name                String
  phone               String?
  baseCutPerCheckIqd  Int       @map("base_cut_per_check_iqd")
  isActive            Boolean   @default(true) @map("is_active")
  notes               String?
  createdAt           DateTime  @map("created_at") @db.Timestamptz
  updatedAt           DateTime  @map("updated_at") @db.Timestamptz
  deletedAt           DateTime? @map("deleted_at") @db.Timestamptz
  version             Int       @default(0)
  lastSyncedAt        DateTime? @map("last_synced_at") @db.Timestamptz
  originDeviceId      String?   @map("origin_device_id")
  entityId            String    @map("entity_id")

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

  operator  Operator  @relation(fields: [operatorId], references: [id])
  checkType CheckType @relation(fields: [checkTypeId], references: [id])

  @@unique([operatorId, checkTypeId])
  @@map("operator_specialties")
}

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

enum SettingType { int decimal text bool }

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

  @@index([entityId, name])
  @@map("patients")
}
```

---

## Section 3: DDD Implementation

### Frontend (React)

#### New pages / routes (`/admin/*`)

| Path | File | Description |
|-|-|-|
| `/admin` | `src/pages/admin/index.tsx` | Sub-nav landing; redirects to `/admin/users`. |
| `/admin/users` | `src/pages/admin/users/list.tsx` | List + create dialog. |
| `/admin/users/:id` | `src/pages/admin/users/detail.tsx` | Detail with edit + reset-password action. |
| `/admin/check-types` | `src/pages/admin/check-types/list.tsx` | List of types with toggle for `has_subtypes`. |
| `/admin/check-types/:id` | `src/pages/admin/check-types/detail.tsx` | Detail with inline subtypes table when `has_subtypes=1`. |
| `/admin/doctors` | `src/pages/admin/doctors/list.tsx` | FTS-backed doctor list. |
| `/admin/doctors/:id` | `src/pages/admin/doctors/detail.tsx` | Detail with `doctor_check_pricing` rows editor. |
| `/admin/operators` | `src/pages/admin/operators/list.tsx` | Operator list. |
| `/admin/operators/:id` | `src/pages/admin/operators/detail.tsx` | Detail with `operator_specialties` multi-select. |
| `/admin/settings` | `src/pages/admin/settings.tsx` | Form for the seven required settings keys. |
| `/admin/inventory` | `src/pages/admin/inventory/placeholder.tsx` | Empty placeholder; populated in P6. |

`src/components/admin/AdminSubNav.tsx` — macOS-System-Settings-style left sub-nav with the routes above.

#### Stores
None added. Reference data is server state (React Query).

#### React Query keys
One per entity: `['check-types', 'list']`, `['check-types', 'detail', id]`, etc. CRUD mutations invalidate the matching `list` key.

#### Zod schemas (`src/lib/schemas/`)

`check-type.ts`, `check-subtype.ts`, `doctor.ts`, `doctor-check-pricing.ts`, `operator.ts`, `operator-specialty.ts`, `settings.ts`, `patient.ts`. Each pairs an `<Entity>Schema` (read shape) with `<Entity>WriteSchema` (form input).

Bilingual fields use:
```ts
nameAr: z.string().trim().min(1).max(120),
nameEn: z.string().trim().min(1).max(120).nullable(),
```

#### i18n bundles
`admin.json` namespace populated. ~120 keys per locale across the eight admin sections.

### Tauri/Rust

#### Domain entities + repos
Per `ddd.md` layout, one folder per entity under `src-tauri/src/domains/<name>/`:
- `domain/<entity>.rs` (struct + factories + invariants).
- `domain/repositories.rs` (trait).
- `infrastructure/sqlite_<name>_repo.rs`.
- `commands.rs`.

Eight new domain folders: `check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `settings`, `patients`.

Invariants enforced in `try_new` / `update`:
- `CheckType`: XOR `has_subtypes ⊕ base_price_iqd != null`. Toggling `has_subtypes` requires the corresponding column null/non-null first.
- `DoctorCheckPricing`: `check_subtype_id` set iff parent `check_types.has_subtypes=1`. `cut_kind = pct` constrains `cut_value ∈ [0, 100]`.
- `Operator`: soft-delete blocked while any open shift exists (enforced in P4 service; here the constraint is documented only).
- `Setting`: `value` must parse to `value_type`.

#### Tauri commands

Per entity, six commands. Pattern: `<entity>_<verb>` snake_case.

| Command | Args | Returns |
|-|-|-|
| `<entity>_list` | `{ search?: String, limit?: i64, cursor?: String, include_inactive?: bool }` | `Vec<EntityRow>` |
| `<entity>_get` | `{ id: Uuid }` | `EntityRow` |
| `<entity>_create` | full write payload | `Uuid` |
| `<entity>_update` | `{ id: Uuid, patch: PartialEntity }` | `()` |
| `<entity>_delete` | `{ id: Uuid }` | `()` (soft delete) |
| `<entity>_search` (only `doctors`, `patients`) | `{ q: String, limit: i64 }` | `Vec<EntitySearchHit>` (FTS5-backed) |

That's 8 entities × 5 commands + 2 search = **42 IPC commands** added in P3.

(Settings has a slightly different shape: `settings_get_all`, `settings_set` per key. ~3 commands. Adjusted total ~40-44.)

### Sync Server (Fastify)

#### Domains added
One DDD module per entity at `sync-server/src/app/domains/<name>/`. Each has:
- `domain/<entity>.ts` — entity class with `toResponse()`, `toPrisma()`, factories.
- `infrastructure/repositories/<entity>.repo.ts`.
- `presentation/routes/<entity>.routes.ts` — REST endpoints.
- `presentation/schemas/<entity>.schemas.ts` — TypeBox.

#### HTTP routes added

| Method | Path | Description |
|-|-|-|
| `GET` | `/check-types` | list (paginated, filter by `q`, `is_active`). |
| `GET` | `/check-types/:id` | detail. |
| `POST` | `/check-types` | create. |
| `PATCH` | `/check-types/:id` | update. |
| `DELETE` | `/check-types/:id` | soft-delete. |

Repeat shape for `check-subtypes`, `doctors`, `doctor-check-pricing`, `operators`, `operator-specialties`, `patients`. That's 7 entities × 5 = 35 routes.

Settings:
- `GET /settings` — entire k/v map.
- `PATCH /settings/:key` — update one key (manual conflict policy).

Total ~37 server routes added in P3.

These routes coexist with `/sync/push` and `/sync/pull` from P2. The PRD's offline-first design has the desktop app push/pull through the sync engine in the steady state; the per-entity REST routes are useful for admin tooling and emergency direct edits, but the desktop app uses them rarely.

---

## Section 4: Business Logic

### Reference-data services (Tauri/Rust)

Each entity gets a service in `src-tauri/src/domains/<name>/services/<name>_service.rs` with the standard methods:
- `list(filter) -> Vec<Entity>`
- `get(id) -> Option<Entity>`
- `create(payload, actor) -> Uuid` (calls `with_audit`).
- `update(id, patch, actor) -> ()` (calls `with_audit`).
- `soft_delete(id, actor) -> ()` (calls `with_audit`).
- `search(q, limit) -> Vec<Hit>` for `doctors` / `patients`.

`SettingsService` extras:
- `get_all_cached(&self) -> HashMap<String, String>` reads `AppState.settings_cache`.
- `set(key, value, actor) -> ()` writes via `with_audit`, updates cache atomically.
- Cache is hydrated on app start by `MigrationRunner` after migrations finish.

### Money math (PRD §6.1.10) — implemented but not used yet

`src-tauri/src/services/money_math.rs` ships a `MoneyMath` module with the resolver functions:
- `resolve_price(check_type, check_subtype?, doctor_pricing?) -> i64`
- `resolve_doctor_cut(price, doctor_pricing? | None_for_house, settings) -> i64`
- `resolve_operator_cut(operator, dye) -> i64`

P5 calls these at lock; P3 ships them so P5 doesn't have to revisit reference data.

### Sync semantics

| Entity | Policy | Notes |
|-|-|-|
| `check_types` | LWW | rare admin edits |
| `check_subtypes` | LWW | rare admin edits |
| `doctors` | LWW | name changes carry over |
| `doctor_check_pricing` | LWW | price changes propagate to new visits only |
| `operators` | LWW | rare admin edits |
| `operator_specialties` | LWW | junction; LWW on the row |
| `settings` | **manual** | concurrent edits to the same key surface conflict UI |
| `patients` | LWW | name correction allowed |

Catalog edits do NOT retroactively rewrite locked visits (PRD §8.5). Drafts get a "prices updated" banner.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions on the server
Append: `CheckType`, `CheckSubtype`, `Doctor`, `DoctorCheckPricing`, `Operator`, `OperatorSpecialty`, `Setting`, `Patient`.

**TENANT_MODELS at end of P3 = 10** (`User`, `AuditLog` from P2 + 8 above).

### Audit triggers
None. `with_audit` in app code suffices.

### Local SQLite indexes added
Listed under each migration in Section 1.

### Tauri capabilities
No additions (existing capabilities cover the new commands).

### New Tauri plugin registrations
None.

### New Fastify plugins / queues
None.

---

## Section 6: Verification

1. **Rust + frontend lint/build/test pass.**
2. **Migrations apply cleanly.** Fresh app boot: 8 new migrations applied; FTS5 tables present; settings seeded.
3. **Admin CRUD live test (Tauri dev).**
   - Create a check_type with `has_subtypes=0` and `base_price_iqd=25000`. Try setting `base_price_iqd=null` — fails (XOR).
   - Create a check_type with `has_subtypes=1` and add 2 subtypes; each with their own price.
   - Create a doctor; add 3 pricing rows (one per check type/subtype combo); confirm uniqueness.
   - Create an operator; assign 2 specialties.
   - Edit `settings.dye_cost_iqd`; observe banner "settings changed" — UI confirms recalc on the (empty) drafts list.
   - Soft-delete a doctor; confirm it disappears from active autocompletes (verified through the search command directly; full visit form lands in P5).
4. **FTS5 doctor + patient search.** Insert 50 dev doctors via seed; type 3 chars; results return < 200ms with diacritic-insensitive matches.
5. **Sync round-trip per entity.** Create a check_type on Tauri device A; reconnect; observe row appear via `psql` on the server; create a doctor on the server; observe the client pull it within 10s. Repeat for each of the 8 entities.
6. **Manual conflict (settings).** Edit `dye_cost_iqd` on two devices while offline; reconnect both; confirm 409 on the second push and a queued conflict (resolver UI lands in P9).
7. **`MoneyMath` unit tests.** Cover: house pct, external pct, external fixed, subtype-priced, doctor-override-priced, dye+report combinations.
8. **i18n coverage.** No hardcoded UI string in `src/pages/admin/*` per `i18next-parser` scan.
9. **RTL.** Admin sub-nav, lists, forms render correctly with `<html dir="rtl">`. Sort arrows mirror.
10. **Pre-push composite.** Same as P2.

### What this phase does NOT verify
- Visit creation / locking (P5).
- Operator shifts / clock-in (P4).
- Inventory consumption (P6).
- Reports (P7).

### Summary update
Bump `status.md` row 3 to `Completed`; record 8 local tables + 2 FTS, 8 server models, ~42 IPC commands, ~37 routes, 5 services. Bump `frontend-summary.md` to add the 11 admin routes (Sections 1, 5, 6, 7) plus admin namespace.

---

## Section 7: PRD Gap Additions

### 7.1 Doctor soft-delete cascade — LOW
**Gap:** PRD §6.1.4 invariant 3 mandates that soft-deleting a doctor cascades soft-deletes to all `doctor_check_pricing` rows for that doctor. Phase 3 §3 lists the invariant but the service description doesn't enumerate the cascade step.
**Category:** Missing Logic.
**Remediation:** Update `DoctorService::soft_delete(doctor_id, actor)`:
1. Open transaction.
2. `with_audit('soft_delete', 'doctors', doctor_id, ...)`:
   - Set `doctors.deleted_at = now`, bump version.
3. For each `doctor_check_pricing` row with `doctor_id = ? AND deleted_at IS NULL`:
   - `with_audit('soft_delete', 'doctor_check_pricing', pricing.id, ...)`: set `deleted_at`.
4. Commit + outbox.

Add unit test: doctor with 3 pricing rows is soft-deleted; all 4 rows show `deleted_at`; 4 audit rows; 4 outbox ops.

### 7.2 `check_types.has_subtypes` toggle invariant enforcement — LOW
**Gap:** PRD §6.1.2 invariants 2-3 mandate transitional rules when `has_subtypes` flips. The schema enforces XOR at write time, but the toggle workflow (admin user clicks the checkbox) needs explicit handling.
**Category:** Missing Logic.
**Remediation:** Update `CheckTypeService::update`:
- If `patch.has_subtypes = 1` and current value is 0: require `patch.base_price_iqd = null` in the same patch; else reject with `CheckTypeError::InvalidToggle`.
- If `patch.has_subtypes = 0` and current value is 1: query `SELECT COUNT(*) FROM check_subtypes WHERE check_type_id = ? AND deleted_at IS NULL`; if > 0, reject with `CheckTypeError::HasActiveSubtypes`. Require `patch.base_price_iqd >= 0` in the same patch.
- UI on Check Type detail: when toggling, show a confirmation dialog spelling out the consequences.

### 7.3 Conflict resolver behavior between P3 and P9 — LOW
**Gap:** `settings` is `manual`-policy. The first conflict can occur in Phase 3 (two admins editing the same setting offline). The resolver UI lands in Phase 9. What does the engine do in the meantime?
**Category:** Missing Integration.
**Remediation:**
- Document explicitly in P3 §6 verification step 6: "manual conflicts surface as a `sync_conflicts` row (table created in P9 — for P3-P8 the row is held in a Phase 1 in-memory queue and persisted to a `sync_conflicts` table from P9 onward). The sync pill shows `error`. Resolution requires manual SQL until P9 ships the resolver UI."
- Add to P3 §5: "Document a manual resolution snippet (`UPDATE settings SET value=?, version=version+1, dirty=1 WHERE key=?` plus delete the conflict row) in the dev-runbook for ops use until P9."
- This is intentional cost of strict-sequential delivery (D-018); flagging here so reviewers don't assume the resolver UI lives in P3.
