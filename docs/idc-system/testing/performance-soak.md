# Performance & Soak Plan

Cross-cutting plan for performance SLOs, the 8-hour soak procedure, scale drills, and cold-start budgets. See `.claude/rules/testing.md` §9 + §6.6, and `docs/idc-system/phase-08.md` (which declared the original 8h soak requirement).

Performance failures are real failures. A flaky perf test is a real bug -- fix the variance, do not raise the threshold. This plan aggregates the SLO table from `testing.md` §9 and adds the soak procedure that runs once per release cycle.

## SLO Aggregate Table

Default SLOs (from `.claude/rules/testing.md` §9 -- canonical there). Per-phase plans may add rows but may not relax any value silently.

| Surface | Operation | Threshold | Measurement vehicle |
|-|-|-|-|
| Tauri (SQLite) | Single-record read by PK | < 5 ms p99 | Rust integration with `criterion` |
| Tauri (SQLite) | List query (typical filtered, 50 rows) | < 30 ms p99 | Rust integration with `criterion` |
| Tauri (SQLite) | Visit lock transaction (full) | < 200 ms p99 | Rust integration on `visit_lock` |
| Tauri (SQLite) | FTS5 patient search (200 chars/word) | < 50 ms p99 | Rust integration on `patients_fts` |
| Tauri (cold start) | First paint after launch | < 3 s p99 | E2E via WebdriverIO, measured from spawn to root-element-visible |
| Sync engine | Outbox drain throughput | >= 50 ops/sec | E2E with synthetic backlog |
| Sync engine | Push round-trip (single op) | < 1 s p95 | Rust integration with mock server / E2E with real server |
| Sync engine | Pull (typical batch of 100 ops) | < 2 s p95 | E2E |
| Sync engine | 8-hour soak steady-state outbox depth | <= 800 rows | Soak run (see below) |
| Sync server | Single-route handler latency | < 200 ms p95 | Node test with k6 or autocannon |
| Reports | 90-day visits report | < 1 s p95 | Rust integration with scaled fixture |
| Reports | Daily-close PDF generation | < 3 s p95 | Rust integration |

## Measurement Discipline

- p95 and p99 percentiles, not averages.
- Each test runs N >= 100 iterations to make the percentile meaningful.
- Tests record raw samples to `target/perf/<test>.csv` for trend tracking.
- A perf test that exceeds threshold fails CI hard. A perf test that runs 10% faster than threshold is healthy headroom; a perf test that runs at the threshold is a yellow flag.
- Variance > 30% between consecutive runs is itself a defect (P2 by default) -- file in `defects.md`.

## Soak Procedure (8-hour)

Run once per minor release (v0.1.x -> v0.2.x). Re-run on demand when sync-engine code changes.

### Setup
1. Build the binary in release mode (`pnpm tauri build`).
2. Bring up the sync server in Docker (`docker compose up -d`).
3. Load `clinical-day.sql` into both local SQLite and the Postgres test DB.
4. Configure two devices via `MULTI_DEVICE=true`.
5. Start the perf monitoring sidecar that samples outbox depth, IPC queue depth, memory, CPU every 10 seconds.

### Run
1. Driver script (`scripts/soak.ts`) issues, for 8 hours:
   - Visit creation + lock at 10/min on Device A.
   - Visit creation + lock at 5/min on Device B.
   - Random inventory adjustments (receive/writeoff) at 1/min total.
   - Periodic 60-second offline windows on Device B every 30 minutes.
   - 1 forced conflict every 60 minutes (same patient edited on both devices during an offline window).
2. The monitoring sidecar writes 2880+ samples to `target/perf/soak-<iso>.csv`.

### Pass Criteria
- Steady-state outbox depth on either device: <= 800 rows. Spikes during offline windows are expected; recovery to baseline within 5 minutes of reconnection.
- Memory usage: monotonic growth < 20MB over 8 hours (a memory leak is a P0 defect).
- CPU usage: < 30% sustained on the Tauri process; sync server < 15% sustained.
- Zero crashes on either device.
- Zero data loss: all locked visits exist on the server at the end of the run; outbox is empty within 10 minutes of run end; conflict count on server matches the number of forced conflicts.
- Sync server logs show zero `5xx` responses except those intentionally injected.
- Snapshot the soak CSV; commit summary to `target/perf/soak-summary-<iso>.md` with charts.

## Scale Drills

Synthetic-fixture drills, distinct from the soak. Each runs in CI on a smaller cadence (weekly or pre-release).

| Drill | Fixture | Assertion | Owner |
|-|-|-|-|
| 10k visits report | `fixtures/scale-10k.sql` | `/accounting/visits` 90-day range renders in < 1s p95 | phase-07-test §6.6 |
| 1k patients FTS | `fixtures/scale-fts.sql` | Patient FTS search < 50ms p99 across 1k rows | phase-05-test §6.6 |
| Outbox drain 5k | Synthetic: 5k queued ops | Drain to empty within 100s (50 ops/sec) | phase-01-test §6.6 |
| 90-day report cold | Cold cache, scaled fixture | < 1s p95 first-run, < 500ms p95 warm | phase-07-test §6.6 |
| 12-month doctor earnings | Scaled fixture | < 4s p95 (scaled override of 90d SLO) | phase-07-test §7 |
| Audit query 12 months | Scaled fixture | < 2s p95 | phase-08-test §6.6 |

## Cold-Start Budget

Cold start is measured from process spawn to first paint of the lock screen. Budget tiers:

| Tier | Threshold | Action |
|-|-|-|
| Healthy | < 2 s p99 | No action |
| Yellow | 2-3 s p99 | Log; investigate when convenient |
| Red | > 3 s p99 | Block release |

Cold-start tests run on the persona P3 (Mariam, fresh install) script as well as on existing-install personas (P1, P2). Both must pass.

## Memory Budget

Long-running stability is a sync-engine concern. Budget:

| Surface | Steady-state RSS | Max RSS during sync |
|-|-|-|
| Tauri main process | < 150 MB | < 300 MB |
| Sync server (per worker) | < 200 MB | < 400 MB |

Memory leaks (monotonic growth) are P0. Recoverable spikes are P2.

## Continuous Perf Tracking

- CI posts perf deltas vs the last 7 main-branch runs to every PR.
- Regressions > 10% on any SLO row block the merge unless the PR explicitly acknowledges and explains.
- Improvements > 20% also flagged for review (often masks a missed code path).
