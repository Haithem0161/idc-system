//! Writes into `metrics_events`. Non-syncable; bounded retention (Phase-8).
//!
//! Helpers intentionally swallow errors so the engine does not fail on a
//! telemetry write failure -- the soak harness uses `metrics_events` only
//! for post-hoc analysis.

use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum MetricKind {
    SyncPushOk,
    SyncPushFail,
    SyncPullOk,
    SyncPullFail,
    SyncConflict,
}

impl MetricKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SyncPushOk => "sync_push_ok",
            Self::SyncPushFail => "sync_push_fail",
            Self::SyncPullOk => "sync_pull_ok",
            Self::SyncPullFail => "sync_pull_fail",
            Self::SyncConflict => "sync_conflict",
        }
    }
}

pub async fn write<P: Serialize>(pool: &SqlitePool, entity_id: &str, kind: MetricKind, payload: P) {
    let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
    let _ = sqlx::query(
        "INSERT INTO metrics_events (id, kind, at, payload_json, entity_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(kind.as_str())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(payload_json)
    .bind(entity_id)
    .execute(pool)
    .await;
}

/// Convenience for ad-hoc untyped payloads.
pub async fn write_value(pool: &SqlitePool, entity_id: &str, kind: MetricKind, payload: Value) {
    write(pool, entity_id, kind, payload).await
}
