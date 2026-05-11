# Phase 7: Accounting & Reports

**Goal:** Read-only accounting module. Ship the Dashboard, Visits Report (with CSV export), Doctor Earnings (+ drill-down), Operator Earnings (+ drill-down), and Daily Close (in-memory v1 artifact). Add server endpoints `/reports/visits` and `/reports/daily-close/:date` for cross-90-day queries.

**Surfaces:** All
**Dependencies:** Phase 06
**Complexity:** L

## §1 Local Schema Changes (Tauri SQLite)

No new tables. Reports read from existing snapshot columns on `visits` and `inventory_adjustments`.

Migration file: `src-tauri/migrations/007_reports.sql` (no DDL beyond possible covering indexes added during implementation; ships as a no-op placeholder unless needed).

### Modified tables

None.

### New enums

None.

## §2 Server Schema Changes (Prisma / Postgres)

No new models. The v1 `daily_close` artifact is in-memory; the signed `daily_close` entity is Horizon-1 work per PRD §11.1.

### Modified models

None.

### New enums

None.

## §3 DDD Implementation

### Frontend (React)

Pages:

| Path | File | Description |
|-|-|-|
| `/accounting` | `src/pages/accounting/dashboard.tsx` | KPIs + trend cards (PRD §7.2.1). |
| `/accounting/visits` | `src/pages/accounting/visits.tsx` | Filterable table + CSV export (PRD §7.2.2). |
| `/accounting/visits/:id` | `src/pages/accounting/visit-drill.tsx` | Reuses `<VisitDetail>` from Phase 5 in read-only mode (accountant role) or with Void button (superadmin). |
| `/accounting/doctors` | `src/pages/accounting/doctors.tsx` | Per-doctor earnings aggregate (PRD §7.2.3). |
| `/accounting/doctors/:id` | `src/pages/accounting/doctor-detail.tsx` | Per-check breakdown + visit list. |
| `/accounting/operators` | `src/pages/accounting/operators.tsx` | Per-operator earnings (PRD §7.2.4). |
| `/accounting/operators/:id` | `src/pages/accounting/operator-detail.tsx` | Shifts in window + attributed visits. |
| `/accounting/daily-close` | `src/pages/accounting/daily-close.tsx` | Reconciliation summary + PDF export (PRD §7.2.5). |

Components:

| Component | File | Purpose |
|-|-|-|
| `<KpiCard>` | `src/components/accounting/kpi-card.tsx` | Single KPI with trend delta. |
| `<DateRangePicker>` | `src/components/accounting/date-range-picker.tsx` | today / yesterday / 7d / month / last month / custom. |
| `<VisitsReportFilters>` | `src/components/accounting/visits-report-filters.tsx` | Date / status / check / subtype / doctor / operator / dye / report. |
| `<VisitsReportTable>` | `src/components/accounting/visits-report-table.tsx` | Detailed table with aggregation footer. |
| `<CsvExportButton>` | `src/components/accounting/csv-export-button.tsx` | Invokes `tauri-plugin-dialog` save-as. |
| `<DoctorEarningsTable>` | `src/components/accounting/doctor-earnings-table.tsx` | Aggregate + house pseudo-row. |
| `<OperatorEarningsTable>` | `src/components/accounting/operator-earnings-table.tsx` | Aggregate + hours-on-shift join. |
| `<DailyCloseLayout>` | `src/components/accounting/daily-close-layout.tsx` | Today vs prior-day side-by-side panel. |

Zustand stores:

| Store | File | State |
|-|-|-|
| `useAccountingFiltersStore` | `src/stores/accounting-filters-store.ts` | Current filter window across pages; persisted per device (not synced). |

React Query keys and hooks:

| Hook | Key | Description |
|-|-|-|
| `useDashboardKpis(range)` | `['reports','dashboard', range]` | Aggregate KPIs from local SQL. |
| `useVisitsReport(filters)` | `['reports','visits', filters]` | Visit list per filter. |
| `useDoctorEarnings(range)` | `['reports','doctors', range]` | Per-doctor aggregate. |
| `useDoctor Drilldown(id, range)` | `['reports','doctor', id, range]` | Per-check + source visits. |
| `useOperatorEarnings(range)` | `['reports','operators', range]` | Per-operator aggregate. |
| `useOperatorDrilldown(id, range)` | `['reports','operator', id, range]` | Shifts + visits. |
| `useDailyClose(date)` | `['reports','dailyClose', date]` | Today vs prior-day artifact. |
| Mutations: `useExportVisitsCsv`, `useExportDailyClosePdf` | | IPC bindings. |

Zod schemas:

| Schema | File |
|-|-|
| `VisitsReportFiltersSchema` | `src/lib/schemas/reports.ts` |
| `DailyCloseSchema` | `src/lib/schemas/reports.ts` |

### Tauri / Rust

Domain entity: reports are read-only views; no entity. Implemented as services in `src-tauri/src/domains/reports/`.

```rust
pub struct ReportsService<'a> { /* repo handles, settings handle */ }
impl<'a> ReportsService<'a> {
  pub async fn dashboard_kpis(&self, range: DateRange) -> Result<DashboardKpis, AppError>;
  pub async fn visits_report(&self, filters: VisitsReportFilters) -> Result<VisitsReport, AppError>;
  pub async fn doctor_earnings(&self, range: DateRange) -> Result<Vec<DoctorEarnings>, AppError>;
  pub async fn doctor_drilldown(&self, doctor_id: Option<Uuid>, range: DateRange) -> Result<DoctorDrilldown, AppError>;
  pub async fn operator_earnings(&self, range: DateRange) -> Result<Vec<OperatorEarnings>, AppError>;
  pub async fn operator_drilldown(&self, operator_id: Uuid, range: DateRange) -> Result<OperatorDrilldown, AppError>;
  pub async fn daily_close(&self, date: NaiveDate) -> Result<DailyClose, AppError>;
}

pub struct CsvWriter;
impl CsvWriter {
  pub fn write_visits(&self, rows: &VisitsReport, path: &Path) -> Result<(), AppError>;
}

pub struct DailyCloseGenerator;
impl DailyCloseGenerator {
  pub fn render_pdf(&self, close: &DailyClose, path: &Path) -> Result<(), AppError>;
}
```

Repository trait: reuses `VisitRepo`, `DoctorRepo`, `OperatorRepo`, `OperatorShiftRepo`, `InventoryAdjustmentRepo` from earlier phases with new aggregation methods on each:

```rust
#[async_trait]
pub trait VisitReadModel {
  async fn list_with_filters(&self, f: VisitsReportFilters) -> Result<Vec<VisitRow>, AppError>;
  async fn aggregate_doctor_earnings(&self, range: DateRange) -> Result<Vec<DoctorAggregate>, AppError>;
  async fn aggregate_operator_earnings(&self, range: DateRange) -> Result<Vec<OperatorAggregate>, AppError>;
  async fn daily_aggregate(&self, date: NaiveDate) -> Result<DayAggregate, AppError>;
}
```

SQLite repo notes:

- All aggregations read from `visits.*_snapshot_iqd` exclusively. No live joins to current pricing tables for locked visits (per PRD §4.1).
- Operator hours-on-shift aggregation joins `operator_shifts` with `(check_out_at - check_in_at)` per shift.
- House row aggregation: WHERE `doctor_id IS NULL`.

Tauri commands:

| Command | Args | Returns | Description |
|-|-|-|-|
| `reports::dashboard_kpis` | `{ range }` | `DashboardKpis` | |
| `reports::visits` | `VisitsReportFilters` | `VisitsReport` | |
| `reports::doctor_earnings` | `{ range }` | `DoctorEarnings[]` | |
| `reports::doctor_drilldown` | `{ doctorId?, range }` | `DoctorDrilldown` | doctorId omitted = house. |
| `reports::operator_earnings` | `{ range }` | `OperatorEarnings[]` | |
| `reports::operator_drilldown` | `{ operatorId, range }` | `OperatorDrilldown` | |
| `reports::daily_close` | `{ date }` | `DailyClose` | In-memory v1 artifact. |
| `reports::export_visits_csv` | `{ filters, path }` | `()` | Writes CSV via `CsvWriter`. |
| `reports::export_daily_close_pdf` | `{ date, path }` | `()` | Writes PDF via `DailyCloseGenerator`. |

Register in `src-tauri/src/lib.rs::generate_handler!`.

### Sync Server (Fastify)

Entity class: no new domain entity; reports are query services.

```ts
class ReportsService {
  async visits(params: VisitsReportParams, tenantId: string): Promise<VisitsReportResponse> { /* aggregate via Prisma */ }
  async dailyClose(date: string, tenantId: string): Promise<DailyCloseResponse> { /* per PRD §7.2.5 */ }
}
```

Repository interface:

```ts
interface VisitReadRepository {
  list(params: VisitsReportParams, tenantId: string): Promise<VisitRow[]>;
  dailyAggregate(date: string, tenantId: string): Promise<DayAggregate>;
}
```

Prisma repo notes: uses raw SQL via `$queryRaw` for window aggregates; tenant filter applied via `entityId` on every aggregate.

TypeBox schemas:

| Schema | Purpose |
|-|-|
| `VisitsReportQuerySchema` | Filters as query params. |
| `VisitsReportResponseSchema` | Row list + totals. |
| `DailyCloseResponseSchema` | Today / prior-day blocks. |

Route table:

| Method | Path | Description |
|-|-|-|
| `GET` | `/reports/visits` | Server-side rollup when local query exceeds threshold. Authenticated; tenant-scoped. |
| `GET` | `/reports/daily-close/:date` | Authoritative daily totals. Authenticated; tenant-scoped. |

## §4 Business Logic

### Frontend

`<VisitsReportTable>` flow:

1. Filter form maps to `VisitsReportFilters` (Zod).
2. Hook chooses local vs remote: if `range` spans more than 90 days OR the user toggles "server data", call server endpoint; else call local `reports::visits`.
3. Aggregation footer sums `priceSnapshotIqd`, `doctorCutSnapshotIqd`, `operatorCutSnapshotIqd`, and `total - cuts` for net.
4. `<CsvExportButton>` calls `tauri-plugin-dialog` save-as to pick a path then dispatches `reports::export_visits_csv`.

`<DailyCloseLayout>` flow:

1. Fetch today's aggregate and prior-day aggregate via `reports::daily_close`.
2. Render KPIs side by side with deltas and percentages.
3. Show "Pending sync" count from `useSyncStatus`; the "Sign and freeze" button (placeholder for Horizon-1 signing) only enables when count is 0.
4. PDF export via `reports::export_daily_close_pdf`.

### Tauri / Rust

`ReportsService::visits_report(filters)`:

1. Compile WHERE clause from filters: date range on `locked_at`; status set; check type set; subtype set; doctor set (`doctor_id IN (...) OR (doctor_id IS NULL AND :house_included)`); operator set; dye / report.
2. Execute `SELECT` with the necessary joins (doctors, operators, check types, check subtypes) for display columns.
3. Compute totals server-side (Rust): sum the snapshot columns.
4. Return `VisitsReport{ rows, totals }`.

`ReportsService::daily_close(date)`:

1. Two aggregations: `date` and `date - 1`.
2. Per aggregation: sum locked visits' `total_amount_iqd_snapshot`, sum `doctor_cut_snapshot_iqd`, sum `operator_cut_snapshot_iqd`, count locked, count voided today, count voided from prior.
3. Inventory consumption value: `SELECT SUM(-delta) FROM inventory_adjustments WHERE reason='consume_visit' AND date(created_at)=:date`.
4. Compute deltas vs prior.
5. Return `DailyClose{ today, prior, deltas, pendingSync }` where `pendingSync` is `outbox` row count.

`CsvWriter::write_visits(rows, path)`:

1. Open file with UTF-8 BOM.
2. Write header per PRD §10.2.
3. Write rows with RFC 4180 quoting.

`DailyCloseGenerator::render_pdf(close, path)`:

1. Render via the same PDF engine used in Phase 5.
2. Layout per the §7.2.5 ASCII mock.

### Sync Server

`ReportsService.visits(params)`:

1. Validate query via TypeBox.
2. Run aggregate query; cap at 10000 rows (paginate beyond).
3. Return `{ rows, totals, nextCursor? }`.

`ReportsService.dailyClose(date)`:

1. Same aggregations as the Tauri implementation; server-side authority for cross-90-day reconciliation.

### Sync Semantics

No new sync contracts. Reports are read-only.

## §5 Infrastructure Updates

### TENANT_MODELS additions (server)

No changes (reports filter by tenant explicitly via JWT claim).

### Audit trigger additions

None.

### Local SQLite indexes

The existing indexes (`visits_status_date`, `visits_check_type`, `visits_doctor`, `visits_operator`, `inventory_adjustments_item`) suffice for the v1 row volumes. Implementation may add a `visits_locked_at_date` covering index if a benchmark warrants it.

### Tauri capabilities

Edit `src-tauri/capabilities/default.json` to include export scopes:

- `fs:scope: $APPDATA/idc-system/exports/**` (CSV + Daily Close PDF dumps).
- `dialog:save` already granted in Phase 1.

### Plugin registrations

None new.

### Fastify plugins / BullMQ queues

- BullMQ NOT introduced; reports are synchronous request-time queries.

### What this phase does NOT touch

- No new entities.
- No new sync contracts.
- No resolver UI (Phase 8).
- No audit UI (Phase 8).
- No signed `daily_close` entity (Horizon-1).

## §6 Verification

1. `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
2. `cd src-tauri && cargo test`; new tests cover aggregation correctness against a fixture of 30+ visits across multiple doctors, operators, dye states, and statuses; verify aggregation footer sums match per-row sums.
3. `pnpm lint && pnpm build`.
4. `pnpm tauri dev`:
   1. Navigate to `/accounting`; assert KPIs render and trend deltas compute against yesterday.
   2. Open `/accounting/visits`; apply filters; assert table updates; assert aggregation footer matches manual sums.
   3. Click CSV export; pick path; open file in a spreadsheet; verify BOM, columns, totals.
   4. Open `/accounting/doctors`; verify house row present; click a doctor; verify drill-down shows per-check rows and source visits.
   5. Open `/accounting/operators`; verify hours-on-shift column; click; verify drill-down lists shifts and attributed visits.
   6. Open `/accounting/daily-close`; verify side-by-side panel; verify "Sign and freeze" disabled when `pendingSync > 0` and enabled when zero.
   7. PDF export the daily close; verify content matches the on-screen panel.
5. `cd sync-server && pnpm test`: `/reports/visits` happy path; `/reports/daily-close/:date` happy path; auth-required negative test.
6. Cross-90-day: simulate a 120-day filter; assert the report hook calls the server endpoint and renders correctly.
7. Sync round-trip: lock a visit on device A; pull on device B; immediately run a report on B; assert the new visit appears within 5 seconds.
8. Localization: switch to ar; verify column labels and number rendering are localized (Eastern-Arabic digits when `settings.arabic_numerals = true`).
9. RTL: assert every accounting page mirrors correctly.
10. Run existing tests; no regressions.

## §7 PRD Gap Additions

_Pass 1 completed 2026-05-11. 13 gaps incorporated below._

### 7.1 Dashboard KPI inventory consumption + trend matrix enumeration
- **Gap:** HIGH | Missing UI Element | PRD §7.2.1
- §3.Frontend `<KpiCard>` is generic; PRD §7.2.1 mandates 5 KPIs plus 3 trend matrices.
- **Resolution:** Replace `<KpiCard>` row in §3.Frontend table with explicit KPI list. Add `<AccountingDashboard>` composition:
  > Renders five `<KpiCard>` tiles (Revenue, Doctor Cuts, Operator Cuts, Inventory Consumption Value, Net) and three `<TrendMatrix>` cards (Today vs Yesterday, This Week vs Last Week, This Month vs Last Month). Each `<KpiCard>` shows the integer value (locale-formatted via §7.12 from phase-02) plus a small delta chip. Each `<TrendMatrix>` shows a 5-row grid (KPI, current, prior, delta, % change).

### 7.2 Dashboard status toggle (locked / voided)
- **Gap:** MEDIUM | Missing UI Element | PRD §7.2.1
- PRD §7.2.1 requires a status toggle "locked-only default; include voided".
- **Resolution:** Add to `<AccountingDashboard>` header row: `<ToggleGroup>` with options `{ locked: 'Locked', all: 'Include voided' }`. Default is `locked`. The toggle updates a Zustand store `accountingFilters.includeVoided`; every report hook (`useVisitsReport`, `useDoctorEarnings`, `useOperatorEarnings`) reads from the store and forwards the flag to its IPC.

### 7.3 `<VisitsReportTable>` 13-column inventory
- **Gap:** HIGH | Missing UI Element | PRD §7.2.2
- PRD §7.2.2 enumerates 13 columns; component description omits them.
- **Resolution:** Extend `<VisitsReportTable>` row in §3.Frontend table:
  > Columns: `Date, Visit #, Patient, Check, Subtype, Doctor, Operator, Dye, Report, Price, Doctor Cut, Operator Cut, Net`. All money columns honor `formatIqd` from phase-02 §7.12. Click on a row navigates to `/accounting/visits/:id` (read-only `<VisitDetail>` per phase-05 §7.24).

### 7.4 `<DoctorEarningsTable>` columns
- **Gap:** MEDIUM | Missing UI Element | PRD §7.2.3
- Columns not enumerated; phase-07 says "Aggregate + house pseudo-row".
- **Resolution:** Extend `<DoctorEarningsTable>` row:
  > Columns: `Doctor (or "House" for internal), Specialty list, Visits count, Revenue, Doctor Cut Total, Avg Cut per Visit`. The "House" pseudo-row aggregates all `doctor_id IS NULL` rows using `internal_pct_snapshot`; specialty column is empty for the house row.

### 7.5 `<OperatorEarningsTable>` columns
- **Gap:** MEDIUM | Missing UI Element | PRD §7.2.4
- Columns not enumerated.
- **Resolution:** Extend `<OperatorEarningsTable>` row:
  > Columns: `Operator, Visits count, Visits with Dye, Operator Cut Total, Hours on Shift (sum of (check_out_at - check_in_at) for the period), Avg Cut per Hour`. Hours render as decimal (e.g., `7.5`).

### 7.6 CSV export for doctor / operator reports
- **Gap:** HIGH | Missing Setup | PRD §10.2
- §3.Tauri commands include `reports::export_visits_csv` but not the doctor / operator equivalents that PRD §10.2 explicitly authorizes.
- **Resolution:** Add to §3.Tauri commands table:
  ```
  | reports::export_doctors_csv   | { from, to, includeVoided } | { path } | Writes the doctor-earnings report to CSV at $APPDATA/.../exports/doctor-earnings_<from>_<to>.csv |
  | reports::export_operators_csv | { from, to, includeVoided } | { path } | Same shape for operators. |
  ```
  All three share the `CsvWriter` from §4. UTF-8 BOM + CRLF + RFC 4180 quoting (per research.md).

### 7.7 CSV column-header schema
- **Gap:** LOW | Incomplete Coverage | research.md
- `CsvWriter::write_visits` does not enumerate column headers.
- **Resolution:** Add to §4 CsvWriter:
  > Visits report headers: `Date,Visit #,Patient,Check,Subtype,Doctor,Operator,Dye,Report,Price (IQD),Doctor Cut (IQD),Operator Cut (IQD),Net (IQD)`.
  > Doctor earnings headers: `Doctor,Specialty,Visits,Revenue (IQD),Doctor Cut Total (IQD),Avg Cut Per Visit (IQD)`.
  > Operator earnings headers: `Operator,Visits,Visits With Dye,Operator Cut Total (IQD),Hours On Shift,Avg Cut Per Hour (IQD)`.
  Every money column carries the `(IQD)` suffix in the header per research.md.

### 7.8 Daily Close date boundary semantics
- **Gap:** HIGH | Missing Logic | PRD §8.4
- The `date(locked_at)` filter is ambiguous between local-day and UTC-day; PRD/research require RFC3339 UTC stamps but the user-facing "today" is local.
- **Resolution:** Add to §4 `DailyCloseGenerator::run(target_date: NaiveDate)`:
  1. Convert `target_date` (a local-tz calendar day) to a UTC range `[start, end)` using the OS timezone at run time.
  2. Aggregate visits with `locked_at >= start AND locked_at < end`.
  3. Server-side endpoint `/reports/daily-close/:date` accepts the date as ISO local-day plus an `?tz=` query param (default `Asia/Baghdad` for the IDC's single-site deployment).
  Document the offset explicitly: "Iraq operates UTC+03:00 year-round (no DST)".

### 7.9 Daily Close per-doctor / per-operator breakdown
- **Gap:** HIGH | Missing Logic | PRD §8.4
- PRD §8.4 requires per-doctor and per-operator breakdowns. §4 step 2 only sums totals.
- **Resolution:** Extend `DailyClose` struct in §4 to:
  ```rust
  struct DailyClose {
      tenant_id: Uuid,
      target_date: NaiveDate,
      tz_offset: String, // "+03:00"
      total_revenue_iqd: i64,
      total_doctor_cuts_iqd: i64,
      total_operator_cuts_iqd: i64,
      total_inventory_consumption_value_iqd: i64,
      net_iqd: i64,
      voided_count: u32,
      voided_value_iqd: i64,
      per_doctor: Vec<DoctorDailyRow>,    // (doctor_id, visits, revenue, cut)
      per_operator: Vec<OperatorDailyRow>, // (operator_id, visits, dye_visits, cut, hours)
      generated_at: DateTime<Utc>,
  }
  ```
  `<DailyClose>` page renders all three sections.

### 7.10 Voided-visit monetary aggregation on Daily Close
- **Gap:** MEDIUM | Incomplete Coverage | PRD §8.4 step 2
- §4 step 2 only counts voided visits.
- **Resolution:** Daily-close step 2 aggregates voided rows' `total_amount_iqd_snapshot` into `voided_value_iqd`; voided revenue is shown in the report as a negative-tinted row below the totals (informational; does NOT subtract from `total_revenue_iqd` because the void offset rows already affect inventory consumption value).

### 7.11 Daily Close `[Run close]` button
- **Gap:** MEDIUM | Missing UI Element | PRD §7.2.5
- PRD §7.2.5 layout shows `[ Run close ]` distinct from `[ Sign and freeze ]`.
- **Resolution:** Extend `<DailyClose>` row in §3.Frontend:
  > Page header has two actions: `[Run close]` (always enabled; recomputes the report) and `[Sign and freeze]` (disabled with tooltip "Available in v0.2"). Date picker defaults to today (local) with a one-click "Yesterday" shortcut.

### 7.12 Daily Close deterministic input-hash for Horizon-1 signing
- **Gap:** LOW | Incomplete Coverage | PRD §11.1
- v1 ships an in-memory daily close; when v0.2 introduces the signed entity, legacy PDFs need a deterministic hash to attach to.
- **Resolution:** `DailyCloseGenerator::run` produces a `DailyCloseArtifact` that includes `input_hash: String` (BLAKE3 of the canonicalized JSON of the aggregation inputs - per-visit IDs sorted, snapshot values, void rows, settings snapshot). Write the hash to the PDF footer in 6-char prefix form. Horizon-1 uses the hash as the freeze key.

### 7.13 `/accounting/visits/:id` read-only mode
- **Gap:** MEDIUM | Missing Page | phase-05 §7.24
- §3.Frontend declares `<VisitDrillDown>` page but no contract for read-only mode.
- **Resolution:** Cross-reference phase-05 §7.24. The `/accounting/visits/:id` route passes `mode='readonly'` to `<VisitDetail>`. Verification step 11 (added below) asserts no Edit/Void buttons render in this mode.

### Verification additions

Append to §6:

> 11. Read-only mode: navigate to `/accounting/visits/<locked_visit_id>` as an accountant; assert Edit and Void buttons are absent; Print buttons remain.
> 12. CSV export: trigger `reports::export_visits_csv` for a 30-day range; open the file in a text editor; assert the first three bytes are `EF BB BF` (UTF-8 BOM), header row matches §7.7, line endings are `\r\n`.
> 13. Daily Close tz boundary: lock a visit at 23:55 local time on day D; lock another at 00:05 local time on day D+1; run daily-close for D; assert only the first visit is included.

### 7.14 Visits-report `groupBy` parameter
- **Gap:** HIGH | Missing Group Param | PRD §7.2.2
- Pass-1 added no groupBy support; the report is row-per-visit only.
- **Resolution:** Add `groupBy` enum to `VisitsReportFilters` (Zod + TypeBox + Rust) with values `none` (default, row-per-visit), `by_date`, `by_doctor`, `by_operator`, `by_check_type`, `by_subtype`, `by_status`. `<VisitsReportFilters>` exposes the selector. `ReportsService::visits_report` switches SELECT/GROUP BY accordingly. Response shape becomes a tagged union: `{ mode: 'rows', rows: Vec<VisitRow>, totals } | { mode: 'groups', groups: Vec<{ key, label, count, revenue, doctor_cut, operator_cut, net }>, totals }`. Server `VisitsReportQuerySchema` mirrors the enum.

### 7.15 Doctor / Operator drill-down navigation contract
- **Gap:** MEDIUM | Missing Drill-Down | PRD §7.5
- `/accounting/doctors/:id` and `/accounting/operators/:id` lacked nav contracts.
- **Resolution:** Specify in §3 Frontend:
  - `<DoctorDrilldown>`: per-check breakdown rows link to `/accounting/visits?from=...&to=...&doctorId=...&checkTypeId=...&subtypeId=...`. Source-visit rows link to `/accounting/visits/:id`. The "House" pseudo-row routes to `/accounting/doctors/house` with `doctorId omitted`; the resulting visits filter sets `doctor_id IS NULL`.
  - `<OperatorDrilldown>`: per-shift rows link to `/reception/shifts?focus=<shift_id>`; attributed-visit rows link to `/accounting/visits/:id`.
  Each link uses React Router `<NavLink>` with proper RTL chevron flip.

### 7.16 Local vs server routing rule for non-visits reports
- **Gap:** HIGH | Inconsistency | PRD §4
- §4 documents the 90-day local-vs-server cutover only for `<VisitsReportTable>`. Doctor/operator earnings, dashboard KPIs, and daily close use the same rule but their server endpoints are deferred.
- **Resolution:** Add to §4: "Doctor earnings, operator earnings, and dashboard KPIs serve EXCLUSIVELY from local SQLite in v1 (server `/reports/doctors` and `/reports/operators` deferred to Horizon-1). If the user requests a range exceeding 90 days the UI clamps to the last 90 days and renders a banner `accounting.banner.long_range_local_only` ('Long-range cross-device totals require v0.2 server aggregates'). Daily Close routes to local first; if local rows are missing for the target date (the device was offline that day), falls back to `/reports/daily-close/:date` automatically. An explicit 'Authoritative' toggle on the Daily Close screen forces server-only mode."

### 7.17 Reports IPC role gating
- **Gap:** HIGH | Missing Permission | PRD §3 navigation tree
- PRD gates `/accounting` to `accountant, superadmin` but no Rust-side IPC enforcement was declared.
- **Resolution:** Add to §3 Tauri: every `reports::*` IPC opens with `require_role(&ctx, &[Role::Accountant, Role::Superadmin])?` (use the helper from phase-02 §7.28). The void button on `/accounting/visits/:id` requires `superadmin` (mirrors PRD §6.1.10 inv 8). Server `/reports/visits` and `/reports/daily-close/:date` enforce the same role set via Fastify auth pre-handler. Document the gate column in §3 route table.

### 7.18 Daily Close run audit row
- **Gap:** MEDIUM | Missing Audit | PRD §10.4
- §4 step 5 produces the artifact silently. Per PRD §10.4 every business write emits one audit_log row; "Run close" is a meaningful event.
- **Resolution:** Add to `DailyCloseGenerator::run`: emit one `audit_log` row with `action='daily_close_run'`, `entity='daily_close'`, `entity_id=<target_date as canonical string>`, `delta={ input_hash, generated_at, total_revenue_iqd, locked_count, voided_count, pending_sync_count, provisional }`. The `daily_close_run` value is added to the application-enforced audit-action enum (phase-01 §7.8 expanded by reference). Server-side: no corresponding server write (Daily Close is local-first in v1).

### 7.19 Daily Close idempotent re-run semantics
- **Gap:** MEDIUM | Missing Precondition | PRD §7.2.5
- `[Run close]` can be clicked repeatedly. Re-run semantics undefined.
- **Resolution:** Re-running for the same target date is a pure read. It never mutates `visits`; it produces the same `input_hash` if no new locks/voids occurred. The on-screen "Last run at" timestamp updates and a fresh audit row is written per §7.18 (one audit per run). If the result differs from the previous run (new locks landed), the UI surfaces a yellow chip `accounting.daily_close.recomputed_n_new_visits` with the count. The PDF filename embeds the `input_hash` prefix to avoid overwrites: `daily-close_<targetDate>_<inputHashPrefix>.pdf`.

### 7.20 Daily Close `pendingSync` provisional watermark
- **Gap:** MEDIUM | Missing Precondition | PRD §8.4
- `[Run close]` is always enabled, but pending outbox rows make the artifact unreliable.
- **Resolution:** When `pendingSync > 0`, the generated artifact is watermarked `PROVISIONAL — N pending ops` in the PDF footer and on screen; `audit_log.delta.provisional = true`. `[Sign and freeze]` remains hard-gated on `pendingSync === 0` (already covered by Pass-1 §7.11). The yellow chip cycles 'provisional' state independently of the §7.19 recomputed chip.

### 7.21 Daily Close per-check-type breakdown
- **Gap:** MEDIUM | Missing Column | PRD §7.2.5
- Pass-1 §7.9 added `per_doctor` and `per_operator` to `DailyClose`. Accountants need per-check-type totals.
- **Resolution:** Extend `DailyClose` struct with `per_check_type: Vec<CheckTypeDailyRow>` where row = `(check_type_id, name_ar, name_en, visits, revenue, doctor_cut, operator_cut)`. Render as a third breakdown section in the PDF and on screen below the per-doctor and per-operator sections.

### 7.22 Dashboard "Top 5" cards
- **Gap:** LOW | Missing UI Element | PRD §7.2.1
- Pass-1 §7.1 enumerated 5 KPIs and 3 trend matrices but did not add "Top" lists.
- **Resolution:** Add to `<AccountingDashboard>` three "Top 5" cards below the trend matrices: Top Doctors by Revenue, Top Operators by Visits, Top Check Types by Revenue. Each is a 5-row mini-table linking each row to the corresponding drill-down route (§7.15). Driven by new IPC `reports::dashboard_tops | { range: DateRange, include_voided: bool } | { top_doctors, top_operators, top_check_types } | Returns top-5 lists sorted by the respective metric.` All ranges clamp to 90 days max per §7.16.

### 7.23 CSV save-path scheme and filename convention
- **Gap:** LOW | Missing Setup | PRD §10
- §5 lists `fs:scope` for exports but no default filename convention.
- **Resolution:** Default filename for CSVs: `<report-slug>_<fromYYYY-MM-DD>_<toYYYY-MM-DD>_<HHmmss>.csv` (e.g., `visits_2026-05-01_2026-05-07_193045.csv`). User may rename in the Save As dialog. Daily-close PDF: `daily-close_<targetDate>_<inputHashPrefix>.pdf`. Slug inventory: `visits`, `doctor-earnings`, `operator-earnings`, `daily-close`.

### 7.24 Server `/reports/visits` schema completeness
- **Gap:** MEDIUM | Missing Group Param | §3 server
- §3 Server `VisitsReportQuerySchema` was missing the §7.14 `groupBy` enum and the cursor format.
- **Resolution:** Extend `VisitsReportQuerySchema`:
  ```ts
  Type.Object({
    groupBy: Type.Optional(Type.Union([Type.Literal('none'), Type.Literal('by_date'), Type.Literal('by_doctor'), Type.Literal('by_operator'), Type.Literal('by_check_type'), Type.Literal('by_subtype'), Type.Literal('by_status')])),
    from: Type.String({ format: 'date' }),
    to:   Type.String({ format: 'date' }),
    tz:   Type.Optional(Type.String({ default: 'Asia/Baghdad' })),
    statuses: Type.Optional(Type.Array(Type.Union([Type.Literal('draft'), Type.Literal('locked'), Type.Literal('voided')]))),
    checkTypeIds: Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
    subtypeIds:   Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
    doctorIds:    Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
    operatorIds:  Type.Optional(Type.Array(Type.String({ format: 'uuid' }))),
    dye:    Type.Optional(Type.Union([Type.Literal('y'), Type.Literal('n'), Type.Literal('all')])),
    report: Type.Optional(Type.Union([Type.Literal('y'), Type.Literal('n'), Type.Literal('all')])),
    includeHouse: Type.Optional(Type.Boolean()),
    cursor: Type.Optional(Type.String()),
    limit:  Type.Optional(Type.Integer({ minimum: 1, maximum: 10000, default: 1000 })),
  });
  ```
  Cursor opaque base64 of `{ lockedAt, visitId }`. Response `nextCursor` is null when fewer than `limit` rows remain.

### 7.25 CSV row sort order
- **Gap:** LOW | Missing Logic | §7.6, §7.7
- Pass-1 §7.7 enumerated CSV headers but not row order.
- **Resolution:** `CsvWriter::write_visits`: rows sorted by `locked_at ASC, visit_id ASC` (deterministic across reruns). Aggregation footer is a final row prefixed `TOTAL,,,,,,,,,<sum-price>,<sum-doctor-cut>,<sum-operator-cut>,<sum-net>`. Doctor and operator CSVs: sort by `<doctor_name|operator_name> ASC` with the `(house)` row last in the doctor CSV; their footer is `TOTAL,...` matching column count.

### 7.26 Earnings CSV export buttons wired
- **Gap:** LOW | Missing UI Element | §7.6
- §7.6 declared `reports::export_doctors_csv` and `reports::export_operators_csv` but `<DoctorEarningsTable>` and `<OperatorEarningsTable>` had no export buttons.
- **Resolution:** Add a `<CsvExportButton>` (already declared in §3 Frontend) to each table's toolbar. Each button binds to its respective IPC and respects the current filter state (from/to, status, etc.). Localized labels: `accounting.actions.export_csv`.

### 7.27 `handle.crumb` on accounting detail routes
- **Gap:** LOW | Missing Handshake | phase-01 §7.13
- Phase-01 `<Breadcrumbs>` reads `handle.crumb` per route. Accounting detail routes did not declare it.
- **Resolution:** Append to §3 Frontend routing block: `/accounting/visits/:id` exports `handle: { crumb: ({ data }) => data.patient_name_snapshot || 'visit' }`. `/accounting/doctors/:id` exports `handle: { crumb: ({ data }) => resolveLocaleName(data) }`. `/accounting/operators/:id` exports the same. Resolves via phase-03 §7.16 `resolveLocaleName`.

### 7.28 `/accounting/*` route role gate
- **Gap:** HIGH | Missing Role Guard | PRD §7.2 line 1738; Pass-3 GAP-E-6
- §7.17 added `require_role` on every `reports::*` IPC and on server endpoints. The `/accounting/*` route tree is not wrapped in `<RequireRole>`; non-accountant URLs return error toasts instead of redirecting to `/no-access`.
- **Resolution:** Append to §3 Frontend routing block: "The `/accounting/*` outlet is wrapped in `<RequireRole roles={['accountant','superadmin']}>` (component from phase-02 §7.8). Non-matching role redirects to `/no-access`. `<UserMenu>` hides the Accounting link based on the same role check."

### 7.29 CSV/bulk import deferral declaration
- **Gap:** LOW | Missing Scope Declaration | PRD §10.2 line 2097; Pass-3 GAP-D-2
- PRD §10.2 explicitly says "No bulk import in v1." No phase records the deferral.
- **Resolution:** Append to §5 "What this phase does NOT touch": "No bulk import / CSV import. The `imports::*` IPC namespace is reserved but unwired in v1; Horizon-1 introduces import flows for catalog reseed and historical visit backfill (PRD §11.1, §10.2)."

### 7.30 Doctor / Operator drill-down breakdown table columns
- **Gap:** MEDIUM | Missing UI Element | PRD §7.2.3 line 1785, §7.2.4 line 1793; Pass-3 GAP-E-11, E-12
- §3 declares `<DoctorDrilldown>` and `<OperatorDrilldown>` pages with no enumeration of breakdown columns or source-visit columns.
- **Resolution:** Add to §3 Frontend components:
  - `<DoctorPerCheckBreakdownTable>` columns: Check Type, Subtype, Visits, Revenue, Doctor Cut, Avg Cut. i18n keys under `accounting.doctors.breakdown.columns.*`.
  - `<DoctorSourceVisitsTable>` columns: Date, Visit #, Patient, Check, Subtype, Operator, Price, Doctor Cut. Each row links to `/accounting/visits/:id` (read-only Visit Detail per phase-05 §7.24).
  - `<OperatorShiftsInWindowTable>` columns: Date, Check-in, Check-out, Duration, Lines run, Cut earned. i18n keys under `accounting.operators.shifts.columns.*`.
  - `<OperatorAttributedVisitsTable>` columns: Date, Visit #, Patient, Check, Subtype, Doctor, Dye, Operator Cut. Each row links to `/accounting/visits/:id`.
