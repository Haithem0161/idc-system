//! Phase-09 §9 / phase-08 §7.16 -- offline soak harness.
//!
//! Authors the soak test surface so the nightly cron runner has a
//! concrete entry-point. The real 8-hour run executes against this
//! file (`cargo test --test soak -- --ignored`); CI skips by default.
//!
//! Quantitative criteria the soak must honor (phase-08 §7.16):
//!
//! | Surface              | SLO                                       |
//! |----------------------|-------------------------------------------|
//! | Outbox drain         | >= 50 ops/sec sustained                   |
//! | Outbox depth         | <= 800 rows steady-state                  |
//! | Visit lock p95       | < 30 s                                    |
//! | Memory growth        | < 50 MB across the window                 |
//! | Audit vacuum         | < 10 s per run                            |
//! | Conflicts            | zero `auto_resolved`                      |
//!
//! ## Configuration
//!
//! The harness reads three env vars:
//!
//! | Var                  | Default | Use                                |
//! |----------------------|---------|------------------------------------|
//! | `SOAK_DURATION_SECS` | 300     | Window in seconds (5 min smoke;    |
//! |                      |         | nightly cron sets `28_800` for 8h) |
//! | `SOAK_OUTBOX_TARGET` | 50      | ops/sec floor                      |
//! | `SOAK_REPORT_PATH`   | unset   | Optional MD report destination.    |
//! |                      |         | When set, the harness writes       |
//! |                      |         | `target/soak-report.md` per phase  |
//! |                      |         | -09 §9.                            |
//!
//! ## Crash protocol
//!
//! All soak tests carry `#[ignore]` so the regular `cargo test`
//! invocation skips them. The nightly runner opts in with `-- --ignored`.
//! Phase-09 forbids running the full `cargo test` (it crashes the
//! IDE); always target this binary explicitly:
//!
//! ```bash
//! cargo test --test soak -- --ignored --nocapture
//! ```
//!
//! ## Status (2026-05-18)
//!
//! - **Scaffold landed**: harness + 1 smoke soak (`outbox_drain_sustains_fifty_ops_per_second_over_window`)
//!   verifying the throughput floor over the configured window.
//! - **Deferred**: the full 6-assertion battery (steady-state depth,
//!   visit-lock p95, memory growth, vacuum latency, zero auto-resolved
//!   conflicts) needs the full SyncEngine + Visit/Inventory writer
//!   wiring and an external memory sampler. Authored as TODO comments
//!   below for the nightly runner to expand against; see the per-
//!   assertion section in this file.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_lib::db::migrations;
use app_lib::domains::sync::domain::entities::OutboxOp;
use app_lib::domains::sync::domain::repositories::OutboxRepo;
use app_lib::domains::sync::infrastructure::SqliteOutboxRepo;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

fn soak_duration() -> Duration {
    let secs: u64 = std::env::var("SOAK_DURATION_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    Duration::from_secs(secs)
}

fn soak_outbox_target() -> f64 {
    std::env::var("SOAK_OUTBOX_TARGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50.0)
}

async fn fresh_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    pool
}

// =========================================================================
// §9.1 Outbox drain throughput soak
//
// Pin the >= 50 ops/sec SLO sustained over the configured window. The
// nightly cron sets SOAK_DURATION_SECS=28800 (8h); local smoke runs at
// the 300s default. The harness loops "enqueue 200 ops -> ack-drain
// the batch -> repeat" while tracking elapsed time + cumulative ops.
// =========================================================================

#[tokio::test]
#[ignore = "soak: opt-in via `cargo test --test soak -- --ignored`"]
async fn outbox_drain_sustains_fifty_ops_per_second_over_window() {
    let pool = fresh_pool().await;
    let repo: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let duration = soak_duration();
    let target = soak_outbox_target();

    let start = Instant::now();
    let mut total_ops: u64 = 0;
    let mut batches: u64 = 0;
    const BATCH_SIZE: usize = 200;

    while start.elapsed() < duration {
        let mut ids = Vec::with_capacity(BATCH_SIZE);
        let mut tx = pool.begin().await.unwrap();
        for i in 0..BATCH_SIZE {
            let op = OutboxOp::new(
                "audit_log",
                format!("soak-{batches}-{i}"),
                b"soak-payload".to_vec(),
            );
            repo.enqueue(&mut tx, &op).await.unwrap();
            ids.push(op.op_id);
        }
        tx.commit().await.unwrap();
        repo.delete_acked(&ids).await.unwrap();
        total_ops += BATCH_SIZE as u64;
        batches += 1;
    }

    let elapsed = start.elapsed();
    let throughput = total_ops as f64 / elapsed.as_secs_f64();

    eprintln!(
        "[soak] outbox_drain: {batches} batches, {total_ops} ops, \
         {throughput:.1} ops/sec over {} s (target >= {target:.0} ops/sec)",
        elapsed.as_secs(),
    );

    assert!(
        throughput >= target,
        "outbox drain throughput {throughput:.1} ops/sec fell below \
         the soak floor of {target:.0} ops/sec after {batches} batches \
         in {} s",
        elapsed.as_secs(),
    );

    // Outbox steady-state depth invariant: at every point in the loop
    // we drained the batch we enqueued, so depth returns to zero at
    // the end of each iteration. Pin that contract.
    let depth = repo.pending_count().await.unwrap();
    assert_eq!(
        depth, 0,
        "outbox depth must return to 0 after each batch drains",
    );
}

// =========================================================================
// §9.2 -- TODO assertions for the nightly runner.
//
// The full 6-assertion battery needs:
// - SyncEngine spinning with real push/pull http (wiremocked or against
//   a containerized sync-server) so `auto_resolved` conflicts can be
//   counted -- requires phase-09 §4 testcontainers infra.
// - Visit/InventoryAdjustment writers driven through `with_audit` so
//   the lock p95 measurement covers a realistic write path (not a
//   bare outbox loop).
// - An external memory sampler. `procfs` (Linux only) reads
//   `/proc/self/status::VmRSS` at a 1 Hz cadence; macOS would need
//   `mach_task_info`. Phase-09 spec assumes Linux-only nightly runner.
// - The `audit_vacuum_now` IPC + a soak-clock that triggers it at
//   03:00 local each "day" of the soak window. Phase-08 already pins
//   single-run vacuum < 10 s in `audit_perf_phase08`; the soak just
//   reasserts it survives ~8 vacuum runs.
// - A `target/soak-report.md` writer. Once the metrics gather, emit:
//     | metric                     | observed | SLO     | pass/fail |
//     |----------------------------|----------|---------|-----------|
//     | outbox drain throughput    | X ops/s  | >= 50   |           |
//     | outbox depth (max steady)  | X rows   | <= 800  |           |
//     | visit lock p95             | X ms     | < 30000 |           |
//     | memory growth              | X MB     | < 50    |           |
//     | audit vacuum (worst run)   | X s      | < 10    |           |
//     | auto_resolved conflicts    | X count  | == 0    |           |
// =========================================================================

#[tokio::test]
#[ignore = "soak: outbox depth steady-state -- needs full SyncEngine wiring"]
async fn outbox_depth_steady_state_under_eight_hundred() {
    // Scaffold: the test runs the same enqueue/drain loop but with a
    // CONTENDING producer (e.g. 3 concurrent tasks) so depth can
    // temporarily exceed the batch size before drain catches up.
    // The 800-row ceiling is the steady-state floor that the
    // outbox_full_lights signal flips at (phase-08 §7.17).
    //
    // Pending: introduce a tokio::spawn that enqueues at >= 60 ops/s
    // while a separate drain task pulls 200 at a time. Pin
    // `max(pending_count_sample)` over the window.
    eprintln!("[soak] outbox_depth_steady_state: SCAFFOLD (no-op until SyncEngine wiring lands)");
}

#[tokio::test]
#[ignore = "soak: visit lock p95 < 30s -- needs Visit + Inventory writer wiring"]
async fn visit_lock_p95_under_thirty_seconds() {
    // Scaffold: drive `Visit::lock` repeatedly through `VisitService`
    // with fixture-loaded patient + doctor + check_type rows.
    // Collect per-lock wall-clock; bucket; assert p95.
    //
    // Pending: factory-load a realistic visit setup and a way to
    // trigger 1000+ locks over the soak window without exhausting
    // the in-memory pool.
    eprintln!("[soak] visit_lock_p95: SCAFFOLD (no-op until VisitService wiring lands)");
}

#[tokio::test]
#[ignore = "soak: memory growth < 50 MB -- needs procfs sampler (Linux-only nightly)"]
async fn memory_growth_under_fifty_megabytes() {
    // Scaffold: poll /proc/self/status::VmRSS at 1 Hz; record min/max;
    // assert (max - start) < 50 MB.
    //
    // Pending: gate behind `#[cfg(target_os = "linux")]` so macOS
    // dev machines don't trip on missing /proc.
    eprintln!("[soak] memory_growth: SCAFFOLD (no-op until procfs sampler lands)");
}

#[tokio::test]
#[ignore = "soak: audit vacuum < 10 s per run -- needs audit_vacuum_now driver"]
async fn audit_vacuum_under_ten_seconds_per_daily_run() {
    // Scaffold: trigger audit_vacuum_now ~8 times across the window
    // (simulating one per 'day' of the 8-hour soak); collect elapsed;
    // assert max < 10 s.
    //
    // Pending: pre-seed audit_log + metrics_events with ~90 days of
    // data so vacuum has real work to do on each invocation.
    eprintln!("[soak] audit_vacuum: SCAFFOLD (no-op until vacuum driver wiring lands)");
}

#[tokio::test]
#[ignore = "soak: zero auto_resolved conflicts -- needs SyncEngine + server stub"]
async fn zero_auto_resolved_conflicts_over_window() {
    // Scaffold: SyncEngine receives a 409 envelope on every Nth push;
    // assert all parked rows have `auto_resolved IS NULL` -- phase-08
    // §7.17 invariant: NO conflict resolves itself silently.
    //
    // Pending: wiremock server stub that returns canned 409 envelopes
    // on configured frequencies.
    eprintln!("[soak] zero_auto_resolved: SCAFFOLD (no-op until SyncEngine+stub lands)");
}
