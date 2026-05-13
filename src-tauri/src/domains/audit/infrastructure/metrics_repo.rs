//! sqlx-backed `MetricsRepo`.
//!
//! Reads from `metrics_events` (phase-01 §7.28). Lock latency comes from
//! paired `lock_start` / `lock_end` rows keyed by `payload_json.visit_id`.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use crate::domains::audit::domain::repositories::MetricsRepo;
use crate::error::AppResult;

#[derive(Clone)]
pub struct SqliteMetricsRepo {
    pool: SqlitePool,
}

impl SqliteMetricsRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetricsRepo for SqliteMetricsRepo {
    async fn vacuum_older_than(&self, cutoff: DateTime<Utc>) -> AppResult<u64> {
        let res = sqlx::query("DELETE FROM metrics_events WHERE at < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    async fn lock_latency_p95_ms(
        &self,
        entity_id_tenant: &str,
        window: Duration,
    ) -> AppResult<Option<i64>> {
        let cutoff = (Utc::now() - window).to_rfc3339();
        // Pair lock_start / lock_end rows by visit_id from payload_json.
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT kind, at, COALESCE(payload_json, '{}') \
             FROM metrics_events \
             WHERE entity_id = ? AND at >= ? \
               AND kind IN ('lock_start', 'lock_end') \
             ORDER BY at ASC",
        )
        .bind(entity_id_tenant)
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;

        let mut starts: std::collections::HashMap<String, DateTime<Utc>> =
            std::collections::HashMap::new();
        let mut durations: Vec<i64> = Vec::new();
        for (kind, at, payload) in rows {
            let dt = match chrono::DateTime::parse_from_rfc3339(&at) {
                Ok(d) => d.with_timezone(&Utc),
                Err(_) => continue,
            };
            let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
            let visit_id = v
                .get("visit_id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if visit_id.is_empty() {
                continue;
            }
            match kind.as_str() {
                "lock_start" => {
                    starts.insert(visit_id, dt);
                }
                "lock_end" => {
                    if let Some(start) = starts.remove(&visit_id) {
                        let dur = (dt - start).num_milliseconds().max(0);
                        durations.push(dur);
                    }
                }
                _ => {}
            }
        }

        if durations.len() < 5 {
            return Ok(None);
        }
        durations.sort_unstable();
        let idx = ((durations.len() as f64) * 0.95).ceil() as usize - 1;
        let idx = idx.min(durations.len() - 1);
        Ok(Some(durations[idx]))
    }

    async fn receipt_print_success_rate(
        &self,
        entity_id_tenant: &str,
        window: Duration,
    ) -> AppResult<Option<f64>> {
        let cutoff = (Utc::now() - window).to_rfc3339();
        let (ok, fail): (i64, i64) = sqlx::query_as(
            "SELECT \
                SUM(CASE WHEN kind = 'receipt_print_ok' THEN 1 ELSE 0 END), \
                SUM(CASE WHEN kind = 'receipt_print_fail' THEN 1 ELSE 0 END) \
             FROM metrics_events \
             WHERE entity_id = ? AND at >= ? \
               AND kind IN ('receipt_print_ok', 'receipt_print_fail')",
        )
        .bind(entity_id_tenant)
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0));
        let total = ok + fail;
        if total == 0 {
            return Ok(None);
        }
        let rate = (ok as f64) / (total as f64);
        Ok(Some((rate * 10_000.0).round() / 10_000.0))
    }

    async fn conflict_count(&self, entity_id_tenant: &str, window: Duration) -> AppResult<u32> {
        let cutoff = (Utc::now() - window).to_rfc3339();
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM metrics_events \
             WHERE entity_id = ? AND at >= ? AND kind = 'sync_conflict'",
        )
        .bind(entity_id_tenant)
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0,));
        Ok(n.max(0) as u32)
    }
}
