# IDC System Research

Domain research that informs all phase files. Authored 2026-05-11. Cross-references PRD-V0.1.0.md (V0.1.1 draft).

## Algorithms

### Money Math at Visit Lock (PRD §6.1.10)

Inputs available at lock time:

- `visit.check_type_id`, `visit.check_subtype_id?`, `visit.doctor_id?`, `visit.operator_id`, `visit.dye`, `visit.report`.
- `check_types.base_price_iqd` (or null if subtyped).
- `check_subtypes.price_iqd` (when applicable).
- `doctor_check_pricing` row for `(doctor_id, check_type_id, check_subtype_id?)` if present.
- `operators.base_cut_per_check_iqd`.
- `settings.dye_cost_iqd`, `settings.report_cost_iqd`, `settings.internal_doctor_pct`.

Algorithm:

```
price =
  if doctor_id is set and doctor_check_pricing.price_override_iqd is not null:
      doctor_check_pricing.price_override_iqd
  elif visit.check_subtype_id is not null:
      check_subtypes.price_iqd
  else:
      check_types.base_price_iqd

dye_cost    = if visit.dye    then settings.dye_cost_iqd    else 0
report_cost = if visit.report then settings.report_cost_iqd else 0

total_amount = price + dye_cost + report_cost

doctor_cut =
  if doctor_id is null:
      floor(price * settings.internal_doctor_pct / 100)
  else if doctor_check_pricing.cut_kind = 'pct':
      floor(price * doctor_check_pricing.cut_value / 100)
  else if doctor_check_pricing.cut_kind = 'fixed':
      doctor_check_pricing.cut_value

operator_cut = operators.base_cut_per_check_iqd * (if visit.dye then 2 else 1)
```

Snapshot writeback on the `visits` row:

- `price_snapshot_iqd = price`
- `dye_cost_snapshot_iqd = dye_cost`
- `report_cost_snapshot_iqd = report_cost`
- `doctor_cut_snapshot_iqd = doctor_cut`
- `operator_cut_snapshot_iqd = operator_cut`
- `internal_pct_snapshot = settings.internal_doctor_pct` only when `doctor_id IS NULL`; else NULL.
- `total_amount_iqd_snapshot = total_amount`

Invariant per PRD §6.1.10 (6): `total_amount_iqd_snapshot = price_snapshot_iqd + dye_cost_snapshot_iqd + report_cost_snapshot_iqd`. The dye/report costs contribute to total but NOT to `doctor_cut_basis`. The operator cut is independent of dye/report cost, but doubles when `dye = 1`.

All math is integer arithmetic in IQD whole units; `floor()` is the i64 truncating division.

### Operator Eligibility Set (PRD §4.2)

At lock, candidate operators are computed as:

```
qualified(visit) =
  { o in operators where o.is_active = 1
    AND exists open shift: operator_shifts.operator_id = o.id
        AND operator_shifts.check_out_at IS NULL
        AND operator_shifts.deleted_at IS NULL
    AND exists specialty: operator_specialties.operator_id = o.id
        AND operator_specialties.check_type_id = visit.check_type_id
        AND operator_specialties.deleted_at IS NULL }
```

If the set is empty, `LockError::NoQualifiedOperator` surfaces in the UI. There is no silent fallback.

### Inventory Recompute (PRD §4.4)

`inventory_items.quantity_on_hand` is materialized but never authoritative. Recompute pattern, executed in the same SQLite transaction as any adjustment write:

```
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
WHERE id IN (:affected_ids);
```

Voiding a visit appends offsetting positive-delta rows referencing the same `visit_id`. The original consume rows are NEVER mutated.

### Daily Close (PRD §8.4)

v1 derives the close on demand from `visits` snapshot columns; no `daily_close` entity yet. Aggregation is local SQL with `entity_id = :tenant AND status = 'locked' AND date(locked_at) = :target_date`.

## Sync Semantics

### Outbox Shape (from `.claude/rules/offline-first.md`)

```sql
CREATE TABLE outbox (
  op_id            TEXT PRIMARY KEY,
  entity           TEXT NOT NULL,
  entity_id        TEXT NOT NULL,
  op               TEXT NOT NULL,
  payload          BLOB NOT NULL,
  created_at       TEXT NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  next_attempt_at  TEXT NOT NULL,
  last_error       TEXT NULL
);
CREATE INDEX outbox_next_attempt ON outbox(next_attempt_at) WHERE attempts < 10;
```

Each business write enqueues exactly one outbox row inside the same SQLite transaction. The sync engine drains the outbox in batches of up to 50 ops to `POST /sync/push`.

### Sync State Table

Tracks the pull cursor and last sync metadata:

```sql
CREATE TABLE sync_state (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  pull_cursor     TEXT NULL,
  last_pulled_at  TEXT NULL,
  last_pushed_at  TEXT NULL,
  device_id       TEXT NOT NULL
);
```

Singleton row enforced by `CHECK (id = 1)`.

### Idempotency

Every push op carries an `op_id` (UUID v7) generated at outbox enqueue time. The server stores `op_id` in a `processed_ops` table per tenant; replays return the original response unchanged. The server NEVER side-effects a duplicate.

### Four Conflict Policies

| Policy | Used By | Resolution Mechanism |
|-|-|-|
| `last-write-wins` | users, check_types, check_subtypes, doctors, doctor_check_pricing, operators, operator_specialties, patients, inventory_items, inventory_consumption_map | Higher `updated_at` wins; tiebreak by `origin_device_id` lex order. |
| `additive-only` | operator_shifts, inventory_adjustments, audit_log | All writes accepted; ordering by `created_at`. No merge. |
| `manual` | visits, settings | Server returns 409 with `{ local, server }` payload; UI surfaces resolver at `/sync/conflicts`. |
| `field-merge` | (unused at v1) | Reserved for future per-field merge entities. |

### Version Bump

Every local mutation increments `version`. The server stores the version it accepted; the pull payload includes both `version` and `updated_at`. On apply, the client compares `version`; if the local row's `version` exceeds the pull's, the pull is discarded for that row (the client will eventually push its higher version).

### Tombstone Propagation

`deleted_at` is set, never cleared. The push payload carries the tombstone; the server retains the row with `deleted_at` set. Pulls re-emit tombstones; consumers hide them in queries (`WHERE deleted_at IS NULL`).

### FTS5 Under Sync

FTS triggers (`AFTER INSERT`, `AFTER UPDATE`, `AFTER DELETE` on the base table) maintain the virtual FTS table. The FTS rows are NOT synced; they are derived locally on apply. The virtual table holds no canonical data.

## Formats

### Identifiers

- **UUID v7** for every row primary key, generated client-side. v7 is time-sortable and supplies an implicit creation order without a separate column.
- **Device ID** is a UUID v7 generated on first boot via `tauri-plugin-os`, stored in `tauri-plugin-store` as `device_id`.

### Timestamps

- All `*_at` columns: **RFC3339 UTC** strings (`2026-05-11T14:23:11.123Z`). Stored as TEXT in SQLite; as `Timestamptz` in Postgres.
- Clock source: local OS clock. Server clock is informational on push; never authoritative for ordering.

### Money

- **IQD whole units**, stored as INTEGER. No decimals. No currency conversion.
- Display: `settings.currency_symbol` (default `د.ع`) suffix. Eastern-Arabic digits optional via `settings.arabic_numerals`.

### CSV Export

- UTF-8 with BOM (`0xEF 0xBB 0xBF`).
- Date column: `YYYY-MM-DD HH:MM:SS` in UTC.
- Money column: raw integer with the unit in the column header (`Total IQD`).
- Field separator: comma. Newline: `\r\n`.
- Strings: RFC 4180 quoting (double-quote enclosure, `""` escape).

### Receipt A5 PDF

- Page size: A5 portrait (148mm x 210mm).
- Layout sketch (RTL when `ar`):

```
+------------------------------------------+
| <Clinic name in active locale>           |
| Date: 2026-05-11 14:23                   |
| Visit #: 0192f...                        |
+------------------------------------------+
| Patient:    <name>                       |
| Check:      <name_ar / name_en>          |
| Subtype:    <name_ar / name_en>          |
| Doctor:     <name or "house">            |
| Operator:   <name>                       |
+------------------------------------------+
| Price:                       <amount IQD>|
| Dye:                         <amount IQD>|
| Report:                      <amount IQD>|
| Total:                       <amount IQD>|
+------------------------------------------+
| <Receptionist name>                      |
+------------------------------------------+
```

Mirroring in RTL: header anchors right, totals column anchors left.

### Receipt Thermal Text

- Width: 58mm (32 chars) or 80mm (48 chars). Configurable via `settings.thermal_width`.
- Code page: UTF-8.
- Layout: fixed-width text grid, single column. Newlines `\n`. ESC/POS not required; the printer is invoked through the OS print dialog as plain text.

## Regulations / Localization

### Jurisdiction

- Single Iraqi medical imaging center. No mandatory e-invoicing in target jurisdiction (per PRD §1.4 scope rationale).
- No tax filing integration; no VAT computation.
- No insurance integration in v1.

### Bilingual Contract

- Default locale: `ar` (RTL). English (`en`) is the only other supported locale.
- Per PRD §10.6, no literal Arabic or English strings in JSX/TSX outside `src/i18n/locales/`.
- Domain data: `check_types`, `check_subtypes`, `inventory_items` carry `name_ar` (NOT NULL) and `name_en` (nullable). Display resolution: active locale `en` and `name_en` non-null then `name_en`; else `name_ar`. People-names (doctors, patients, operators) are single free-form `name` columns.

### RTL Implementation Contract

- `<html dir="rtl">` set by `i18n` on language change.
- Tailwind v4 logical properties only: `ps-*` / `pe-*` / `ms-*` / `me-*` / `text-start` / `text-end`. No `pl-*` / `pr-*` / `ml-*` / `mr-*` / `text-left` / `text-right` in feature code.
- Arrow and chevron icons mirror via `rtl:rotate-180`.
- Receipts and printed PDFs mirror full layout in RTL (header right, totals left).

### Eastern-Arabic Digits

- Optional via `settings.arabic_numerals` (default `false`). When true and active locale is `ar`, digits render via `Intl.NumberFormat('ar-IQ', { numberingSystem: 'arab' })`.
- Iraqi invoicing convention uses Western digits by default; the toggle exists for staff preference.

## Decisions Log

| Date | Decision | Rationale |
|-|-|-|
| 2026-05-11 | 8 phases of mostly M/L size, sequenced 01-08. | User-locked; matches PRD scope size without producing XL phases that drag verification. |
| 2026-05-11 | Sync server built in lockstep with Tauri starting Phase 1. | PRD §1 states the two surfaces are planned jointly; late integration is a known risk. |
| 2026-05-11 | Conflict-resolution mechanism (op_id, 409, /sync/conflicts/:opId/resolve) ships in Phase 1; resolver UI in Phase 8. | Manual-policy entities (visits, settings) cannot be deployed without the mechanism; the UI is low-risk to defer because conflicts are rare. |
| 2026-05-11 | A5 PDF receipt + thermal text generators both ship inside Phase 5's lock workflow. | PRD §8.1 requires receipt generation within the lock transaction; splitting the artifact leaves the workflow incomplete. |
| 2026-05-11 | LWW rejected for `visits`; `manual` chosen. | Financial-critical entity; concurrent edits must surface to a human (PRD §11.4). |
| 2026-05-11 | No per-internal-doctor named tracking. | User-stated workflow leaves the doctor field empty for in-house cases (PRD §11.4). |
| 2026-05-11 | Receipt printing routes through the OS print dialog; no auto-print. | Avoid jammed-printer-driven lock failures (PRD §11.4). |
| 2026-05-11 | All IDs client-generated UUID v7. | Server-generated IDs break offline-first (PRD §11.4 and offline-first.md). |
| 2026-05-11 | Hard delete rejected for every business row; tombstones only. | Audit and reversibility (PRD §11.4). |
| 2026-05-11 | Operator role with logins deferred to Horizon 2. | Matches user spec; operators are tracked records, not users (PRD §11.4). |
| 2026-05-11 | `daily_close` as in-memory artifact at v1; signed entity deferred to Horizon 1. | Avoids modelling a server-signed entity before the sync engine is hardened (PRD §11.1). |
| 2026-05-11 | Local audit retention 90 days; server retention indefinite. | Balance local DB size against drill-down depth (PRD §10.4). |
| 2026-05-11 | Inventory `quantity_on_hand` may go negative without blocking. | Surgical/dye items can over-consume during emergencies (PRD §6.1.12). |
| 2026-05-11 | FTS5 for `patients_fts` and `doctors_fts` only. | Small cardinality of check types and inventory items makes FTS overkill there (PRD §10.1). |

## References

- PRD: [docs/idc-system/PRD-V0.1.0.md](./PRD-V0.1.0.md)
- Offline-first rules: [.claude/rules/offline-first.md](../../.claude/rules/offline-first.md)
- Auth rules: [.claude/rules/auth.md](../../.claude/rules/auth.md)
- Sync-server rules: [.claude/rules/sync-server.md](../../.claude/rules/sync-server.md)
- Tauri rules: [.claude/rules/tauri.md](../../.claude/rules/tauri.md)
- Rust rules: [.claude/rules/rust.md](../../.claude/rules/rust.md)
- Frontend rules: [.claude/rules/frontend.md](../../.claude/rules/frontend.md)
- DDD rules: [.claude/rules/ddd.md](../../.claude/rules/ddd.md)
- Planning rules: [.claude/rules/planning.md](../../.claude/rules/planning.md)
