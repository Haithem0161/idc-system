//! Audit DTOs returned to the IPC boundary.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// One row in the audit table (frontend `<AuditTable>`).
///
/// Carries `dirty` so the Pending-sync column from phase-05 §7.29 can render
/// without a second fetch (phase-08 §7.15).
#[derive(Debug, Clone, Serialize)]
pub struct AuditRowDto {
    pub id: String,
    pub at: DateTime<Utc>,
    pub actor_user_id: String,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub delta: serde_json::Value,
    pub device_id: String,
    pub version: i64,
    pub dirty: bool,
    pub source: AuditSource,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditSource {
    Local,
    Server,
}

/// Page returned by `audit::query`. `mode` tells the UI whether to render
/// the `<ServerBackedBadge>` (phase-08 §3 Frontend, §7.25).
#[derive(Debug, Clone, Serialize)]
pub struct AuditPage {
    pub rows: Vec<AuditRowDto>,
    pub mode: AuditQueryMode,
    pub next_offset: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditQueryMode {
    Local,
    Server,
    Merged,
}

/// `diagnostics::summary` payload (phase-08 §7.17).
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiagnosticsSummaryDto {
    pub lock_latency_p95_ms: Option<i64>,
    pub outbox_depth: u32,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub conflict_count_7d: u32,
    pub receipt_print_success_rate_30d: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_source_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_value(AuditSource::Local).unwrap(),
            json!("local")
        );
        assert_eq!(
            serde_json::to_value(AuditSource::Server).unwrap(),
            json!("server")
        );
    }

    #[test]
    fn audit_query_mode_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_value(AuditQueryMode::Local).unwrap(),
            json!("local")
        );
        assert_eq!(
            serde_json::to_value(AuditQueryMode::Server).unwrap(),
            json!("server")
        );
        assert_eq!(
            serde_json::to_value(AuditQueryMode::Merged).unwrap(),
            json!("merged")
        );
    }

    #[test]
    fn audit_page_serializes_with_rows_mode_next_offset_keys() {
        let page = AuditPage {
            rows: vec![],
            mode: AuditQueryMode::Local,
            next_offset: Some(50),
        };
        let v = serde_json::to_value(&page).unwrap();
        assert!(v.get("rows").is_some());
        assert!(v.get("mode").is_some());
        assert!(v.get("next_offset").is_some());
        assert_eq!(v["mode"], json!("local"));
        assert_eq!(v["next_offset"], json!(50));
    }

    #[test]
    fn audit_page_next_offset_null_when_none() {
        let page = AuditPage {
            rows: vec![],
            mode: AuditQueryMode::Local,
            next_offset: None,
        };
        let v = serde_json::to_value(&page).unwrap();
        assert!(v["next_offset"].is_null());
    }

    #[test]
    fn diagnostics_summary_dto_default_is_safe_for_empty_databases() {
        let s = DiagnosticsSummaryDto::default();
        assert!(s.lock_latency_p95_ms.is_none());
        assert_eq!(s.outbox_depth, 0);
        assert!(s.last_sync_at.is_none());
        assert_eq!(s.conflict_count_7d, 0);
        assert!(s.receipt_print_success_rate_30d.is_none());
    }

    #[test]
    fn diagnostics_summary_dto_serializes_with_all_five_keys() {
        let s = DiagnosticsSummaryDto::default();
        let v = serde_json::to_value(&s).unwrap();
        for key in [
            "lock_latency_p95_ms",
            "outbox_depth",
            "last_sync_at",
            "conflict_count_7d",
            "receipt_print_success_rate_30d",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn audit_row_dto_round_trips_dirty_flag() {
        let row = AuditRowDto {
            id: "id-1".into(),
            at: Utc::now(),
            actor_user_id: "actor-1".into(),
            action: "create".into(),
            entity: "doctors".into(),
            entity_id: "ent-1".into(),
            delta: json!({"k": "v"}),
            device_id: "dev-1".into(),
            version: 1,
            dirty: true,
            source: AuditSource::Local,
        };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["dirty"], json!(true));
        assert_eq!(v["source"], json!("local"));
    }
}
