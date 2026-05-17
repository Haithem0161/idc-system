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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use uuid::Uuid;

    async fn fresh_pool() -> sqlx::SqlitePool {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        crate::db::migrations::run(&pool).await.unwrap();
        pool
    }

    async fn insert(pool: &sqlx::SqlitePool, kind: &str, at: DateTime<Utc>, tenant: &str) {
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(kind)
        .bind(at.to_rfc3339())
        .bind("{}")
        .bind(tenant)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn vacuum_older_than_returns_deleted_row_count() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        let cutoff = now - Duration::days(30) - Duration::hours(1);
        insert(&pool, "sync_push_ok", cutoff, "t-1").await;
        insert(&pool, "sync_push_ok", cutoff, "t-1").await;
        insert(&pool, "sync_push_ok", now, "t-1").await;
        let removed = repo
            .vacuum_older_than(now - Duration::days(30))
            .await
            .unwrap();
        assert_eq!(removed, 2);
    }

    #[tokio::test]
    async fn vacuum_older_than_returns_zero_when_no_match() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        insert(&pool, "sync_push_ok", now, "t-1").await;
        let removed = repo
            .vacuum_older_than(now - Duration::days(30))
            .await
            .unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn receipt_print_success_rate_returns_none_when_no_print_events() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        // Insert unrelated kinds.
        insert(&pool, "sync_push_ok", Utc::now(), "t-1").await;
        let r = repo
            .receipt_print_success_rate("t-1", Duration::days(30))
            .await
            .unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn receipt_print_success_rate_rounds_to_four_decimals() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        // 7 / 11 = 0.636363... -> 0.6364 after 4-decimal rounding.
        for _ in 0..7 {
            insert(&pool, "receipt_print_ok", now, "t-1").await;
        }
        for _ in 0..4 {
            insert(&pool, "receipt_print_fail", now, "t-1").await;
        }
        let r = repo
            .receipt_print_success_rate("t-1", Duration::days(30))
            .await
            .unwrap()
            .unwrap();
        assert!((r - 0.6364).abs() < 1e-6, "got {r}");
    }

    #[tokio::test]
    async fn conflict_count_filters_by_tenant_and_window() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        for _ in 0..3 {
            insert(&pool, "sync_conflict", now, "t-1").await;
        }
        insert(&pool, "sync_conflict", now - Duration::days(8), "t-1").await;
        insert(&pool, "sync_conflict", now, "t-other").await;
        let n = repo.conflict_count("t-1", Duration::days(7)).await.unwrap();
        assert_eq!(n, 3);
    }

    #[tokio::test]
    async fn lock_latency_p95_ms_returns_none_below_five_pairs() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        for i in 0..3 {
            let vid = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_start")
            .bind((now - Duration::minutes(i + 1)).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_end")
            .bind((now - Duration::minutes(i + 1) + Duration::milliseconds(50)).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
        }
        let r = repo
            .lock_latency_p95_ms("t-1", Duration::days(7))
            .await
            .unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn lock_latency_p95_ms_returns_value_with_five_or_more_pairs() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        for i in 0..6 {
            let vid = Uuid::now_v7().to_string();
            let start = now - Duration::minutes(i + 1);
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_start")
            .bind(start.to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_end")
            .bind((start + Duration::milliseconds(10 * (i + 1))).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
        }
        let r = repo
            .lock_latency_p95_ms("t-1", Duration::days(7))
            .await
            .unwrap();
        assert!(r.is_some());
        // Six pairs at 10,20,30,40,50,60ms -> p95 index = ceil(6*0.95)-1 = 5 -> 60.
        assert_eq!(r.unwrap(), 60);
    }

    #[tokio::test]
    async fn lock_latency_p95_ms_drops_unpaired_starts() {
        let pool = fresh_pool().await;
        let repo = SqliteMetricsRepo::new(pool.clone());
        let now = Utc::now();
        // Seed 6 paired samples.
        for i in 0..6 {
            let vid = Uuid::now_v7().to_string();
            let start = now - Duration::minutes(i + 1);
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_start")
            .bind(start.to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind("lock_end")
            .bind((start + Duration::milliseconds(20)).to_rfc3339())
            .bind(format!("{{\"visit_id\":\"{vid}\"}}"))
            .bind("t-1")
            .execute(&pool)
            .await
            .unwrap();
        }
        // Add an unpaired lock_start.
        sqlx::query(
            "INSERT INTO metrics_events (id,kind,at,payload_json,entity_id) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind("lock_start")
        .bind(now.to_rfc3339())
        .bind("{\"visit_id\":\"unpaired\"}")
        .bind("t-1")
        .execute(&pool)
        .await
        .unwrap();
        let r = repo
            .lock_latency_p95_ms("t-1", Duration::days(7))
            .await
            .unwrap()
            .unwrap();
        // Unpaired start should not poison the result; all 6 paired = 20ms.
        assert_eq!(r, 20);
    }
}
