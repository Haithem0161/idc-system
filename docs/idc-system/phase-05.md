# Phase 5: Reception Per-Check & Lock Workflow

**Goal:** Land the `visits` entity (single-check, with all snapshot fields), the per-check Reception UI (Checks Grid → Check Workspace → New Visit form → Visit Detail), and the lock workflow that snapshots prices, attributes the operator, and prints a receipt — all within a single SQLite transaction.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 4.
**Complexity:** XL
**PRD references:** §3.1 (routes), §4.1 (lock-then-snapshot), §4.2 (operator attribution), §6.1.10 (visits), §7.1.1-§7.1.5 (Reception sub-pages), §8.1 (Lock workflow), §10.3 (receipts).
**Decisions consumed:** D-002, D-005, D-006, D-013 (per-check), D-016 (manual conflict on visits), D-017 (money math), D-019 (receipt persistence).

---

## Section 1: Local Schema Changes (Tauri SQLite)

### Migration `013_visits.sql` (PRD §6.1.10 verbatim)

```sql
CREATE TABLE IF NOT EXISTS visits (
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
CREATE INDEX visits_patient        ON visits(entity_id, patient_id) WHERE deleted_at IS NULL;
```

---

## Section 2: Server Schema Changes (Prisma / Postgres)

### `Visit`

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

  patient      Patient       @relation(fields: [patientId], references: [id])
  receptionist User          @relation("VisitReceptionist", fields: [receptionistUserId], references: [id])
  voidedBy     User?         @relation("VisitVoider",       fields: [voidedByUserId], references: [id])
  checkType    CheckType     @relation(fields: [checkTypeId], references: [id])
  checkSubtype CheckSubtype? @relation(fields: [checkSubtypeId], references: [id])
  doctor       Doctor?       @relation(fields: [doctorId], references: [id])
  operator     Operator?     @relation(fields: [operatorId], references: [id])
  // The `inventoryAdjustments InventoryAdjustment[]` back-relation lands in Phase 6
  // when `InventoryAdjustment` is introduced (P6 §2 "Existing models updated").
  // Adding it here would make `prisma db push` fail in Phase 5 because the
  // referenced model doesn't exist yet.

  @@index([entityId, checkTypeId, lockedAt])
  @@index([entityId, doctorId,    lockedAt])
  @@index([entityId, operatorId,  lockedAt])
  @@map("visits")
}

enum VisitStatus { draft locked voided }
```

---

## Section 3: DDD Implementation

### Frontend (React)

#### New routes (`/reception/*`)

| Path | File | Description |
|-|-|-|
| `/reception` | `src/pages/reception/checks-grid.tsx` | Grid of `check_types` cards; click navigates to workspace. |
| `/reception/checks/:slug` | `src/pages/reception/check-workspace.tsx` | Per-check workspace: stats, filters, visits table filtered by `check_type_id`. |
| `/reception/checks/:slug/new` | `src/pages/reception/new-visit.tsx` | Single-check New Visit form. |
| `/reception/visits/:id` | `src/pages/reception/visit-detail.tsx` | Detail with `Details` / `Audit` / `Receipts` tabs. |

`:slug` resolves via a server-side `slug` derived from `name_en` (lowercased, dashed) or, when `name_en` is null, from `id` prefix. Lookup helper: `useCheckTypeBySlug(slug)`.

`/reception/shifts` already lives from P4.

#### Stores
None added (server state via React Query).

#### React Query hooks
- `useCheckTypesGrid()` — list of active check_types with today's locked-visit counts.
- `useCheckWorkspace(checkTypeId, filters)` — paged visits for that check.
- `useVisitDetail(id)` — single visit + computed cuts (server-side aggregation later, local for now).
- `useDoctorSearch(q)` — FTS-backed.
- `usePatientSearch(q)` — FTS-backed.
- `useOperatorEligibility(checkTypeId)` — clocked-in operators with matching specialty.
- `useCreateVisit()`, `useUpdateVisit()`, `useDiscardVisit()`, `useLockVisit()`.

#### Zod schemas
`visit.ts` — `VisitSchema`, `VisitDraftWriteSchema`, `LockVisitPayloadSchema`.

#### i18n
`reception.json` populated (~150 keys: form labels, toggles, totals, lock prompts, error messages). `receipts.json` namespace populated with all printed-artifact strings.

#### shadcn additions
`command` (search-select for doctors / patients), `popover` (operator picker), `tooltip`, `combobox` pattern.

### Tauri/Rust

#### Domain `visits/`

`src-tauri/src/domains/visits/domain/visit.rs` — `Visit` struct mirroring the schema. Methods:
- `Visit::create_draft(patient_id, check_type_id, receptionist, entity_id) -> Visit`
- `update_draft_field(...)` — for subtype, doctor, dye, report changes.
- `lock(operator_id, snapshots, now) -> Result<Visit, AppError>` — flips status; sets all snapshot fields.
- `void(reason, voided_by, now)` — flips status; clears nothing (snapshots remain authoritative).
- Factories: `try_new_draft`, `reconstitute`.

#### Repository

```rust
#[async_trait]
pub trait VisitRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Visit>, AppError>;
    async fn list_for_check_type(&self, check_type_id: Uuid, filter: VisitFilter) -> Result<Vec<Visit>, AppError>;
    async fn count_today_locked(&self, check_type_id: Uuid) -> Result<i64, AppError>;
    async fn upsert(&self, tx: &mut sqlx::Transaction<'_, Sqlite>, visit: &Visit) -> Result<(), AppError>;
}
```

#### Tauri commands

| Command | Args | Returns | Description |
|-|-|-|-|
| `visits_checks_grid` | `()` | `Vec<ChecksGridCard>` | List of check_types with today's count. |
| `visits_list_for_check` | `{ check_type_id: Uuid, filter: VisitFilter }` | paged `Vec<VisitRow>` | Workspace table data. |
| `visits_get` | `{ id: Uuid }` | `VisitDetailRow` | Detail page. |
| `visits_create_draft` | `{ check_type_id: Uuid, patient_name: String }` | `Uuid` | Create patient (or match) + new visit row. |
| `visits_update_draft` | `{ id: Uuid, patch: VisitDraftPatch }` | `()` | Subtype / doctor / dye / report changes. |
| `visits_discard` | `{ id: Uuid }` | `()` | Soft-delete a draft. |
| `visits_operator_eligibility` | `{ check_type_id: Uuid }` | `Vec<OperatorOption>` | Wraps `shifts_eligibility_for_check` from P4. |
| `visits_lock` | `{ visit_id: Uuid, operator_id: Uuid }` | `LockResult` (visit + receipt paths) | Runs the lock workflow (Section 4). |
| `visits_print_receipt` | `{ visit_id: Uuid, format: 'pdf' | 'thermal' }` | `String` (path) | Re-prints an existing receipt. |

9 IPC commands.

### Sync Server (Fastify)

Domain `visits/`. Routes:

| Method | Path | Description |
|-|-|-|
| `GET` | `/visits` | List with deep filters (status, dateRange, checkType, doctor, operator, dye, report). Supports cursor pagination. |
| `GET` | `/visits/:id` | Detail. |
| `PATCH` | `/visits/:id/lock` | Server-side lock (admin tooling parity). Same money math. |
| `PATCH` | `/visits/:id/void` | Stub; void workflow lands fully in P8. Returns 501 in P5. |

The desktop pushes via the sync engine in steady state; the REST routes are admin tooling.

`/sync/push` registry adds `visits` with **manual** policy.

---

## Section 4: Business Logic

### `MoneyMath` (already shipped in P3)

Reused here:
- `resolve_price` (PRD §6.1.10).
- `resolve_doctor_cut`.
- `resolve_operator_cut`.
- `resolve_internal_pct_snapshot` (returns `settings.internal_doctor_pct` only when `doctor_id IS NULL`).

### `OperatorEligibility`

```rust
pub struct OperatorEligibility { /* repo handle */ }

impl OperatorEligibility {
    pub async fn for_check_type(&self, check_type_id: Uuid) -> Result<Vec<OperatorOption>, AppError> {
        // 1. SELECT operators currently clocked in (operator_shifts.check_out_at IS NULL).
        // 2. JOIN operator_specialties WHERE check_type_id = ?.
        // 3. Return distinct operators with name + base cut for UI.
    }
}
```

### `VisitService::lock` — the heart of P5 (PRD §8.1)

File: `src-tauri/src/domains/visits/services/visit_service.rs`.

Step sequence (all inside one SQLite transaction):

1. **Validate draft.** `visit.status == 'draft'`. `check_type_id` set; `check_subtype_id` set iff parent type has subtypes; `dye` consistent with `check_types.dye_supported`; `report` consistent with `check_types.report_supported`; patient has a non-empty name.
2. **Eligibility check.** `OperatorEligibility::for_check_type(visit.check_type_id)` — incoming `operator_id` MUST be in this set, else `LockError::OperatorNotQualified` (or `NoQualifiedOperator` if the set is empty).
3. **Begin transaction.**
4. **Resolve snapshots.**
   - Load `CheckType`, `CheckSubtype` (if any), `Doctor` + `DoctorCheckPricing` (if doctor set), `Operator`, and the relevant settings.
   - `price = MoneyMath::resolve_price(check_type, check_subtype, doctor_pricing)`.
   - `dye_cost = settings.dye_cost_iqd if visit.dye else 0`.
   - `report_cost = settings.report_cost_iqd if visit.report else 0`.
   - `doctor_cut = MoneyMath::resolve_doctor_cut(price, doctor_pricing, settings)`.
   - `operator_cut = MoneyMath::resolve_operator_cut(operator, visit.dye)`.
   - `internal_pct_snapshot = settings.internal_doctor_pct if visit.doctor_id IS NULL else None`.
   - `total = price + dye_cost + report_cost`.
5. **Write the visit.** Set `status='locked'`, `locked_at=now`, `operator_id`, all `*_snapshot_iqd` columns, `internal_pct_snapshot`. `repo.upsert(tx, &visit)`.
6. **Inventory consumption** (Phase 6 hook). In P5 this is a `// TODO P6` comment; the integration test is added in P6 verification.
7. **Audit log.** `with_audit(action='lock', entity='visits', delta=<every snapshot field from null to value>)` writes the row in the same transaction.
8. **Receipt rendering.** `ReceiptRenderer::render(visit_with_joins, locale)` produces:
   - PDF via `printpdf` crate to `$APPDATA/idc-system/receipts/<YYYY>/<MM>/<visit-id>.pdf`.
   - Thermal text to `$APPDATA/idc-system/receipts/thermal/<visit-id>.txt`.
   - Both paths returned to the UI in `LockResult`.
   Failure to render aborts the transaction (PRD §8.1 business rule).
9. **Commit.**
10. **Outbox enqueue.** One `upsert` op for `visits`. (Inventory consume rows in P6 are enqueued in step 6.)
11. **Return `LockResult`** to the UI: `{ visit, pdf_path, thermal_path }`.

### `ReceiptRenderer`

File: `src-tauri/src/services/receipt_renderer.rs`.

PDF layout (A5):
- Header: clinic name (`settings.clinic_display_name_ar`) right-aligned in RTL, left-aligned in LTR.
- Patient: quadripartite name.
- Visit timestamp.
- Check (name_ar/_en per locale) + subtype (if any).
- Doctor (name or "house").
- Dye / report flags.
- Lines table with `price`, `dye_cost`, `report_cost`, `total` columns.
- Footer: receptionist name + visit-id (small).
- All strings from `i18n receipts.<key>`. Currency formatted with `formatIqd` (frontend) / `format_iqd_string` (Rust) — same algorithm.

Thermal text (58/80 mm character grid):
- Pure fixed-width ASCII / Unicode; tested with both Arabic and English. Q-001 raised in `research.md` flags the wrap for multi-word Arabic check names — addressed in P5 verification.

### Sync semantics

`visits` is **manual** conflict policy. The push handler returns `409` with `{ local, server }` payload when two devices have edited the same visit's snapshot fields concurrently. The resolver UI lands in P9; for P5 the engine logs the conflict and the visit row is rolled back to its server state.

### Frontend — Lock UX

The `New Visit` form has a "Lock & print" button. Click flow:
1. Validate inputs in-form.
2. Call `visits_operator_eligibility(check_type_id)`.
3. Display operator picker dropdown (popover).
4. On confirm, call `visits_lock({ visit_id, operator_id })`.
5. On success, fire `system print dialog` with PDF + write thermal text via `tauri-plugin-shell` to the configured printer.
6. Navigate to `/reception/visits/:id` (Detail page) showing the locked state.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
Append `Visit`. **TENANT_MODELS at end of P5 = 12.**

### Audit triggers
None (audit-first via `with_audit`).

### Local SQLite indexes
All listed under `013_visits.sql`.

### Tauri capabilities
- Add `dialog:save` (already from P1 baseline).
- Add `shell:allow-execute` with `name: "print"` allowlist for the thermal-print command.

```json
{ "identifier": "shell:allow-execute", "allow": [{ "name": "print", "args": [{ "validator": "^[A-Za-z0-9_/.-]+$" }] }] }
```

### New Tauri plugins
None (printpdf is a Rust crate; not a Tauri plugin).

### New Fastify plugins
None.

### Crate additions (Rust)
- `printpdf` — PDF rendering.
- `chrono-tz` — for receipt timestamp localization (Asia/Baghdad default).

Per `dev-workflow.md`: `cargo add printpdf chrono-tz`.

---

## Section 6: Verification

1. Rust + frontend lint/build/test pass.
2. Migration applies; CHECK constraints enforced (manual SQL trial: insert with `status='locked'` and `price_snapshot_iqd=null` → fails).
3. **Checks Grid renders 5+ check cards** (after seeding via P3 admin).
4. **Workspace.** Click a card; lands at `/reception/checks/:slug`; today's count + filtered visits shown.
5. **Create draft.** New Visit form opens with check name banner; subtype radio shown when type has subtypes; doctor search returns FTS hits in <200ms; operator picker shows the right set after a clock-in.
6. **Lock flow live.**
   - Lock visit with house (no doctor) + no dye + no report → snapshot reflects internal pct, dye_cost=0, report_cost=0.
   - Lock visit with external doctor (pct cut) → doctor cut = `floor(price * cut_value / 100)`.
   - Lock visit with external doctor (fixed cut) → doctor cut = `cut_value`.
   - Lock visit with dye = 1 → operator cut doubled; dye_cost added to total.
   - Lock visit with report = 1 → report_cost added to total; doctor & operator cuts unaffected.
   - Each lock writes one `audit_log` row with full delta.
7. **Receipt artifacts.** PDF + thermal text persisted; reprint button on Visit Detail produces the same files.
8. **Operator attribution failure.** Try to lock with no operator clocked in → `NoQualifiedOperator`; UI shows error in active locale.
9. **Lock-then-snapshot invariance.** After lock, edit `dye_cost_iqd` in admin; locked visit's `dye_cost_snapshot_iqd` does NOT change.
10. **Sync round-trip.** Create + lock visit on device A; observe row + audit row + receipt-untouched (receipts are local-only) on device B's pull within 10s; the receipt is regenerated on demand from the snapshot data.
11. **Manual conflict.** Two devices edit same draft (different fields); reconnect both; second push is rejected with 409 — engine logs; resolver UI in P9.
12. **i18n + RTL** verified on every Reception screen.
13. **Pre-push composite** as before.

### What this phase does NOT verify
- Inventory consumption on lock (P6 — explicit `// TODO P6` test exists).
- Void (P8).
- Accounting reports (P7).
- Audit page (P9).
- Conflict resolver UI (P9).

### Summary update
Bump `status.md` row 5 to `Completed`. Add 4 new routes (Checks Grid, Workspace, New Visit, Visit Detail), `useChecksGrid`, `useCheckWorkspace`, `useVisitDetail`, `useDoctorSearch`, `usePatientSearch`, `useOperatorEligibility`, `useCreateVisit`, `useUpdateVisit`, `useDiscardVisit`, `useLockVisit`, the `reception.json` and `receipts.json` namespaces, and the new shadcn components to `frontend-summary.md`.

---

## Section 7: PRD Gap Additions

### 7.1 Pricing-change banner on draft visits — MEDIUM
**Gap:** PRD §8.5 mandates that, after an admin edits a price (`check_types`, `check_subtypes`, `doctor_check_pricing`, or `settings`), existing **draft** visits show a "prices updated — refresh totals?" banner so the receptionist can refresh per-visit. Phase 5 §3 lists "Recalculate draft" button under §8.5 but doesn't detail the trigger or the banner UI.
**Category:** Missing Integration.
**Remediation:** In Phase 5:
- Add a Tauri event `prices:changed` emitted by `CheckTypeService`, `CheckSubtypeService`, `DoctorCheckPricingService`, `SettingsService` on every successful update affecting price-resolution.
- In `New Visit` and `Visit Detail` (draft state), subscribe to the event via a hook `usePriceChangeBanner()`.
- When fired, render a top banner via shadcn `Alert`:
  > "أسعار محدّثة — حدّث المجموع؟ / Prices updated — refresh totals?" with a "Recalculate" button.
- Click → re-fetch reference data + recompute the running total panel.
- Locked visits ignore the event (snapshots are authoritative).

### 7.2 Subtype soft-delete blocked when visits reference it — LOW
**Gap:** PRD §6.1.3 invariant 3: "Soft-delete is allowed if no non-deleted `visits` reference the subtype with `status != voided`." Phase 3 ships subtype CRUD before `visits` exists. Phase 5 introduces `visits` and is therefore the first phase where the invariant can be enforced.
**Category:** Missing Logic (cross-phase).
**Remediation:** Extend `CheckSubtypeService::soft_delete` in Phase 5:
- Query `SELECT COUNT(*) FROM visits WHERE check_subtype_id = ? AND deleted_at IS NULL AND status != 'voided'`.
- If > 0, reject with `CheckSubtypeError::HasActiveVisits { count }`.
- UI surfaces: "X non-voided visits reference this subtype — void them first or migrate to a different subtype."
- Add unit test: lock a visit using subtype A → soft-delete subtype A → rejected; void the visit → soft-delete subtype A → succeeds.

### 7.3 Receipt thermal-print Arabic word-wrap (Q-001 follow-up) — LOW
**Gap:** Open question Q-001 from `research.md` asks how multi-word Arabic check names wrap on thermal print without breaking ligatures. Phase 5 verification mentions "tested with `tauri-plugin-shell` printer driver" but the wrap algorithm isn't pinned.
**Category:** Missing Logic.
**Remediation:** In `ReceiptRenderer::render_thermal`:
- Use the Unicode bidirectional algorithm (`unicode-bidi` crate, `cargo add unicode-bidi`).
- Word-wrap at the BIDI run boundary, never mid-word; break on whitespace.
- Test with the bilingual fixtures: "مفراس بدون صبغة", "Ultrasound (abdominal)", and a mixed-locale clinic name.
- Document the algorithm in a code comment + the dev runbook.
