# Phase 7: Accounting Reports & Daily Close

**Goal:** Land the read-only Accounting module with deep-filtered visit reports, doctor / operator earnings, daily close reconciliation, and CSV export. Aggregations run locally for snappy UX; server-side fallback exists for queries that exceed local retention or scale.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 6.
**Complexity:** L
**PRD references:** §7.2 (Accounting module), §8.4 (Daily Close), §10.2 (Export), §10.5 (Multi-Currency).
**Decisions consumed:** D-006 (operator cut math), D-017 (snapshot-based reporting), D-026 (Daily Close v1 = on-demand).

---

## Section 1: Local Schema Changes (Tauri SQLite)

**No new tables.** Reports read from `visits`, `inventory_adjustments`, `operator_shifts`, `audit_log`, and reference data — all introduced in earlier phases.

A small materialized helper view is created for performance:

```sql
-- Materialized via trigger after lock/void; kept in sync by VisitService.
CREATE TABLE IF NOT EXISTS visit_daily_rollup (
  date            TEXT NOT NULL,                        -- YYYY-MM-DD
  check_type_id   TEXT NOT NULL,
  doctor_id       TEXT NULL,
  operator_id     TEXT NOT NULL,
  visits_count    INTEGER NOT NULL DEFAULT 0,
  revenue_iqd     INTEGER NOT NULL DEFAULT 0,
  doctor_cut_iqd  INTEGER NOT NULL DEFAULT 0,
  operator_cut_iqd INTEGER NOT NULL DEFAULT 0,
  voided_count    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, check_type_id, COALESCE(doctor_id, ''), operator_id)
);
CREATE INDEX visit_daily_rollup_date ON visit_daily_rollup(date);
```

This is a **local-only** denormalization for the Dashboard's KPI cards; report screens read directly from `visits` (the snapshot fields are everything they need). The rollup is recomputed by a service method invoked at lock, void, and on initial pull catch-up.

### What this phase does NOT touch
No PRD entity tables. No FTS. No sync columns.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

**No new models.** The server-side report routes aggregate over existing models.

A server-only materialized view (Postgres `MATERIALIZED VIEW`) is added for the daily-close endpoint:

```sql
-- File: sync-server/prisma/init-custom-sql.sql (appended)
CREATE MATERIALIZED VIEW IF NOT EXISTS visits_daily AS
  SELECT
    DATE(locked_at AT TIME ZONE 'Asia/Baghdad') AS date,
    entity_id,
    check_type_id,
    doctor_id,
    operator_id,
    COUNT(*) FILTER (WHERE status = 'locked')                                       AS visits_count,
    COUNT(*) FILTER (WHERE status = 'voided')                                       AS voided_count,
    COALESCE(SUM(price_snapshot_iqd) FILTER (WHERE status = 'locked'), 0)           AS revenue_iqd,
    COALESCE(SUM(doctor_cut_snapshot_iqd) FILTER (WHERE status = 'locked'), 0)      AS doctor_cut_iqd,
    COALESCE(SUM(operator_cut_snapshot_iqd) FILTER (WHERE status = 'locked'), 0)    AS operator_cut_iqd
  FROM visits
  WHERE deleted_at IS NULL
  GROUP BY 1, 2, 3, 4, 5;

CREATE UNIQUE INDEX visits_daily_pk ON visits_daily (date, entity_id, check_type_id, COALESCE(doctor_id, ''), operator_id);
```

Refreshed nightly (server cron job; in v1 a simple `setInterval` calls `REFRESH MATERIALIZED VIEW visits_daily;`).

---

## Section 3: DDD Implementation

### Frontend (React)

#### New routes (`/accounting/*`)

| Path | File | Description |
|-|-|-|
| `/accounting` | `src/pages/accounting/dashboard.tsx` | KPI cards, trend cards, top filters. |
| `/accounting/visits` | `src/pages/accounting/visits-report.tsx` | Deep-filtered visits table; CSV export. |
| `/accounting/visits/:id` | redirect to `/reception/visits/:id` | Reuse Visit Detail page. |
| `/accounting/doctors` | `src/pages/accounting/doctor-earnings.tsx` | Per-doctor aggregates. |
| `/accounting/doctors/:id` | `src/pages/accounting/doctor-detail.tsx` | Per-(doctor, check) breakdown + visit list. |
| `/accounting/operators` | `src/pages/accounting/operator-earnings.tsx` | Per-operator aggregates. |
| `/accounting/operators/:id` | `src/pages/accounting/operator-detail.tsx` | Operator detail + shifts in window. |
| `/accounting/daily-close` | `src/pages/accounting/daily-close.tsx` | End-of-day reconciliation. |

#### React Query hooks
- `useDashboardKpis(range)` — KPIs from local rollup + live count.
- `useVisitsReport(filter, cursor)` — paged visits.
- `useDoctorEarnings(range, filter)`.
- `useOperatorEarnings(range, filter)`.
- `useDailyClose(date)` — local first; falls back to `GET /reports/daily-close/:date` if stale.

#### Zod schemas
`accounting-filter.ts`, `daily-close.ts`.

#### i18n
`accounting.json` namespace (~120 keys: KPI labels, filter labels, column headers, export prompts).

#### CSV export utility
`src/lib/csv.ts` — UTF-8 BOM + `Intl.NumberFormat('en')` for IQD integers (no commas in machine fields). Save-as via `tauri-plugin-dialog`'s `save` dialog filtered to `.csv`.

### Tauri/Rust

#### Domain `accounting/`

`src-tauri/src/domains/accounting/services/reporting_service.rs`. Methods:
- `dashboard_kpis(range) -> DashboardKpi` — reads `visit_daily_rollup`.
- `visits_report(filter) -> Vec<VisitReportRow>` — joins `visits` + reference data; respects all filters from PRD §7.2.2.
- `doctor_earnings(range, filter) -> Vec<DoctorEarningsRow>`.
- `operator_earnings(range, filter) -> Vec<OperatorEarningsRow>` — joins `operator_shifts` for hours-on-shift.
- `daily_close(date) -> DailyCloseArtifact` — aggregates, compares vs prior day.

#### Tauri commands

| Command | Args | Returns |
|-|-|-|
| `accounting_dashboard` | `{ range: DateRange }` | `DashboardKpi` |
| `accounting_visits_report` | `{ filter, cursor?, limit }` | paged `Vec<VisitReportRow>` |
| `accounting_doctor_earnings` | `{ range, filter }` | `Vec<DoctorEarningsRow>` |
| `accounting_operator_earnings` | `{ range, filter }` | `Vec<OperatorEarningsRow>` |
| `accounting_daily_close` | `{ date: String }` | `DailyCloseArtifact` |
| `accounting_export_csv` | `{ kind, filter }` | `String` (file path) |

6 IPC commands.

`accounting_export_csv` runs the same query then writes a UTF-8 BOM CSV to a `tauri-plugin-dialog`-picked path.

### Sync Server (Fastify)

Domain `reports/`. Routes:

| Method | Path | Description |
|-|-|-|
| `GET` | `/reports/visits` | server-side equivalent of the visits report; supports the same filters; for queries that exceed local retention. |
| `GET` | `/reports/doctor-earnings` | `?range=...&filter=...`. |
| `GET` | `/reports/operator-earnings` | `?range=...&filter=...`. |
| `GET` | `/reports/daily-close/:date` | Authoritative daily totals; signed with the same RS256 key as JWT (signed string in response, future-proof for the Horizon-1 `daily_close` entity). |

4 routes.

### Frontend → Server fallback rule

Local first. Fall back to server when:
- Range exceeds local 90-day retention.
- Local result reports `staleness > 60s` (i.e. dirty rows in the window).

UI surfaces a "querying server" pill while a server-backed query is in flight.

---

## Section 4: Business Logic

### `ReportingService` (Tauri)

#### `dashboard_kpis(range) -> DashboardKpi`

```sql
SELECT
  COALESCE(SUM(revenue_iqd), 0)        AS revenue,
  COALESCE(SUM(doctor_cut_iqd), 0)     AS doctor_cuts,
  COALESCE(SUM(operator_cut_iqd), 0)   AS operator_cuts,
  COALESCE(SUM(visits_count), 0)       AS visits,
  COALESCE(SUM(voided_count), 0)       AS voided
FROM visit_daily_rollup
WHERE date BETWEEN ? AND ?;
```

Plus a parallel "yesterday" / "last week" / "last month" pair for trend cards.

#### `visits_report(filter) -> ...`

Direct `SELECT v.* FROM visits v JOIN ...` with the filter set from PRD §7.2.2 applied as `WHERE` clauses. Cursor pagination on `(locked_at DESC, id DESC)`.

#### `doctor_earnings(range, filter) -> ...`

```sql
SELECT
  d.id AS doctor_id, d.name AS doctor_name, d.specialty,
  COUNT(v.id) AS visits,
  COALESCE(SUM(v.price_snapshot_iqd), 0) AS revenue,
  COALESCE(SUM(v.doctor_cut_snapshot_iqd), 0) AS doctor_cut_total,
  CASE WHEN COUNT(v.id) > 0 THEN SUM(v.doctor_cut_snapshot_iqd) / COUNT(v.id) ELSE 0 END AS avg_cut_per_visit
FROM visits v
LEFT JOIN doctors d ON d.id = v.doctor_id
WHERE v.status = 'locked'
  AND v.locked_at BETWEEN ? AND ?
  AND v.deleted_at IS NULL
GROUP BY d.id, d.name, d.specialty
UNION ALL
-- House aggregate for v.doctor_id IS NULL
SELECT NULL, '(house)', NULL, COUNT(*), SUM(price_snapshot_iqd), SUM(doctor_cut_snapshot_iqd), AVG(doctor_cut_snapshot_iqd)
FROM visits
WHERE status = 'locked' AND doctor_id IS NULL AND locked_at BETWEEN ? AND ? AND deleted_at IS NULL;
```

#### `operator_earnings(range, filter) -> ...`

```sql
SELECT
  o.id, o.name,
  COUNT(v.id) AS visits,
  COUNT(v.id) FILTER (WHERE v.dye = 1) AS visits_with_dye,
  COALESCE(SUM(v.operator_cut_snapshot_iqd), 0) AS operator_cut_total,
  COALESCE(SUM(julianday(s.check_out_at) - julianday(s.check_in_at)) * 24, 0) AS hours_on_shift,
  ...
FROM operators o
LEFT JOIN visits v ON v.operator_id = o.id AND v.locked_at BETWEEN ? AND ?
LEFT JOIN operator_shifts s ON s.operator_id = o.id AND s.check_in_at BETWEEN ? AND ?
WHERE v.deleted_at IS NULL
GROUP BY o.id, o.name;
```

#### `daily_close(date) -> DailyCloseArtifact`

```rust
pub struct DailyCloseArtifact {
    pub date: NaiveDate,
    pub today: DailyTotals,
    pub prior: DailyTotals,
    pub pending_sync_count: i64,
    pub matches: bool,                      // true if no manual adjustments needed
}
```

Step sequence (PRD §8.4):
1. Aggregate today's locked visits + cuts.
2. Aggregate today's voided visits.
3. Aggregate inventory consumption today (sum of `consume_visit` adjustments today).
4. Compute prior-day deltas.
5. Render printable PDF artifact via `ReceiptRenderer::render_daily_close(...)`.
6. Return path + summary.

### `ReportsService` (server)

Mirror queries on Postgres against the materialized view + raw `visits` for live windows. Daily-close endpoint signs the response with the JWT private key — this future-proofs the Horizon-1 `daily_close` entity (PRD §11.1) so a client signing-key-rotation doesn't break archived signatures.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
None.

### Audit triggers
None (reports are read-only).

### Local SQLite indexes
- `visit_daily_rollup_date`.

### Tauri capabilities
- `dialog:save` with `csv` filter (already covered by P1's `dialog:save`).

### Tauri plugins
None.

### Fastify additions
- `node-cron` (or `BullMQ` if Phase 9 decides to go full BullMQ) for the nightly `REFRESH MATERIALIZED VIEW`.

Per `dev-workflow.md`: `pnpm add node-cron && pnpm add -D @types/node-cron` in `sync-server/`.

---

## Section 6: Verification

1. Lint / build / test on all surfaces.
2. **Dashboard KPIs** match raw SQL totals (`SELECT SUM(price_snapshot_iqd) FROM visits WHERE status='locked' AND locked_at BETWEEN ? AND ?`) within the 60s rollup latency.
3. **Visits report.** Filter by check type + dye=1 + date range; CSV export opens cleanly in LibreOffice with Arabic glyphs intact.
4. **Doctor earnings** sums match `SUM(doctor_cut_snapshot_iqd)` per doctor.
5. **Operator earnings** include hours-on-shift; per-hour avg matches `cut_total / hours`.
6. **Daily close.** End-of-day artifact prints cleanly in both locales; `pending_sync_count` reflects the outbox.
7. **Server fallback.** Pull a 6-month range; UI shows "querying server" pill; results stream from `/reports/visits`.
8. **Daily-close signature** verifies via the public key fetched at login.
9. **i18n + RTL** on every accounting page.
10. **Pre-push composite.**

### What this phase does NOT verify
- Void workflow (P8) — voided visits already render in the report; the void *action* lands in P8.
- Audit page (P9).
- Backup (P10).

### Summary update
Bump `status.md` row 7 to `Completed`. Add 7 routes + accounting hooks + namespace to `frontend-summary.md`. Note CSV export utility under conventions.

---

## Section 7: PRD Gap Additions

### 7.1 Daily-close template RTL fidelity — LOW
**Gap:** PRD §10.6 requires receipts and printed reports to mirror layout for RTL. Phase 7 mentions `ReceiptRenderer::render_daily_close` but doesn't enumerate the RTL handling specifically (the Phase 5 receipt handles RTL; daily close is similar but distinct).
**Category:** Missing Logic.
**Remediation:** In `ReceiptRenderer::render_daily_close(date, totals, locale)`:
- Header in active locale; clinic name on the right (RTL) or left (LTR) edge.
- Two-column layout (Today vs Prior) with `dir`-aware column ordering.
- Numeric columns always right-aligned in LTR, left-aligned in RTL (so the integer's most significant digit is closest to the column header).
- Test with both locales; add screenshot fixtures to the verification suite.

### 7.2 Daily close PDF-only export — LOW
**Gap:** PRD §10.3: "Daily-close: PDF only." Phase 7 §4 mentions PDF generation but doesn't pin "PDF only — no thermal" anywhere.
**Category:** Missing Logic.
**Remediation:** Document explicitly in `ReceiptRenderer::render_daily_close` — only the PDF code path is wired; no thermal alternative. The `daily-close` command does not emit a `.txt` file. Add a unit test that verifies the thermal renderer panics or returns `Err(NotApplicable)` for the daily-close kind.

### 7.3 Trend-card lookback bounds — LOW
**Gap:** Phase 7 dashboard mentions "today vs yesterday, this week vs last, this month vs last" trend cards. The `prior` query for "this month vs last" can extend beyond local 90-day retention near month boundaries.
**Category:** Missing Integration.
**Remediation:** When the prior-period query extends past `now - 90d`, route through the server's `/reports/visits` endpoint (already present) for the prior numbers; live numbers stay local. UI surfaces a tiny tooltip "prior numbers from server" when this happens.
