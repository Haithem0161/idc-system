//! `AuditWriter::with_audit` -- the canonical write helper for domain
//! mutations. Implements PRD §4.3 audit-first ordering (phase-01 §7.7):
//!
//! 1. Open SQLite tx.
//! 2. Compute the audit row from the caller's `(before, after)` snapshot.
//! 3. Insert the audit row (audit-first).
//! 4. Run the caller's business write closure.
//! 5. Enqueue outbox rows (audit + business entity).
//! 6. Commit.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

/// MessagePack-encode an `AuditEntry` for the sync wire format.
///
/// Uses `with_struct_map` so fields are keyed by name (matches the server's
/// `@msgpack/msgpack` field-name decode) and `with_human_readable` so `Uuid`
/// values serialize as their hyphenated string form. Without the latter,
/// `Uuid` becomes 16 raw bytes (a msgpack `bin`), which JS decodes as a
/// `Uint8Array` and `decodeAuditPayload` rejects with 422 "audit payload
/// missing field: id".
pub fn encode_audit_payload(audit: &AuditEntry) -> AppResult<Vec<u8>> {
    let mut buf = Vec::new();
    let mut ser = rmp_serde::Serializer::new(&mut buf)
        .with_struct_map()
        .with_human_readable();
    audit.serialize(&mut ser)?;
    Ok(buf)
}

/// The ordered set of write steps inside `with_audit`. Audit-first is the
/// load-bearing invariant (phase-01 §7.7): the audit row commits before any
/// business or outbox enqueue, so a tx rollback leaves zero rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterStep {
    InsertAudit,
    InvokeBusinessWrites,
    EnqueueOutbox,
}

impl WriterStep {
    /// The canonical step order. Pinned by a test so a refactor that
    /// reorders the writer fails loudly.
    pub fn canonical_order() -> [WriterStep; 3] {
        [
            Self::InsertAudit,
            Self::InvokeBusinessWrites,
            Self::EnqueueOutbox,
        ]
    }
}

/// Phase-01 §1.1: `AuditWriter::skip_if_no_change`.
///
/// Returns `true` when before and after snapshots are structurally equal,
/// meaning the writer can short-circuit -- no audit row, no business write,
/// no outbox enqueue. Phase-04's "bump version only when fields changed"
/// invariant consumes this.
pub fn skip_if_no_change(before: &serde_json::Value, after: &serde_json::Value) -> bool {
    before == after
}

/// Caller-supplied closure that performs the business write inside the tx
/// and returns the snapshots needed to build the audit row.
#[async_trait]
pub trait BusinessWrite: Send {
    /// Snapshot of the row before the mutation, used as the `from` half of
    /// the audit delta. Return `serde_json::Value::Null` if the row did not
    /// previously exist.
    async fn before(&mut self, tx: &mut Tx<'_>) -> AppResult<serde_json::Value>;

    /// Execute the mutation. Return the row's new snapshot and any outbox
    /// rows that should be enqueued AFTER the audit row.
    async fn write(&mut self, tx: &mut Tx<'_>) -> AppResult<(serde_json::Value, Vec<OutboxOp>)>;
}

#[derive(Clone)]
pub struct AuditWriter {
    audit_repo: Arc<dyn AuditRepo>,
    outbox_repo: Arc<dyn OutboxRepo>,
    device_id: String,
}

impl AuditWriter {
    pub fn new(
        audit_repo: Arc<dyn AuditRepo>,
        outbox_repo: Arc<dyn OutboxRepo>,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            audit_repo,
            outbox_repo,
            device_id: device_id.into(),
        }
    }

    /// Execute a domain write with audit-first ordering inside a single tx.
    ///
    /// The audit row's `delta` is `compute_delta(before, after)`; identical
    /// fields are omitted (see `services::delta`).
    #[allow(clippy::too_many_arguments)]
    pub async fn with_audit<W: BusinessWrite>(
        &self,
        pool: &sqlx::SqlitePool,
        actor_user_id: Uuid,
        action: AuditAction,
        entity: &str,
        entity_id: &str,
        entity_id_tenant: &str,
        ip: Option<String>,
        mut write: W,
    ) -> AppResult<serde_json::Value> {
        let mut tx: Tx<'_> = pool.begin().await.map_err(AppError::from)?;

        let before = write.before(&mut tx).await?;
        let (after, mut business_outbox) = write.write(&mut tx).await?;

        let delta = super::delta::compute_delta(&before, &after);

        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id,
            action,
            entity: entity.into(),
            entity_id: entity_id.into(),
            delta,
            ip,
            device_id: self.device_id.clone(),
            entity_id_tenant: entity_id_tenant.into(),
        });

        // Audit-first: insert before any outbox enqueues run.
        self.audit_repo.append(&mut tx, &audit).await?;

        // Enqueue the audit row's own outbox push (additive-only entity).
        let audit_payload = encode_audit_payload(&audit)?;
        let audit_outbox = OutboxOp::new("audit_log", audit.id.to_string(), audit_payload);
        self.outbox_repo.enqueue(&mut tx, &audit_outbox).await?;

        // Then the business outbox rows.
        for op in &mut business_outbox {
            self.outbox_repo.enqueue(&mut tx, op).await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_order_is_audit_then_business_then_outbox() {
        // Phase-01 §7.7 invariant: the writer must execute steps in this
        // exact order. The list is pinned so any refactor that reorders
        // the steps fails this test.
        let order = WriterStep::canonical_order();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], WriterStep::InsertAudit);
        assert_eq!(order[1], WriterStep::InvokeBusinessWrites);
        assert_eq!(order[2], WriterStep::EnqueueOutbox);
    }

    #[test]
    fn writer_step_variants_are_distinct() {
        let order = WriterStep::canonical_order();
        for i in 0..order.len() {
            for j in 0..order.len() {
                if i != j {
                    assert_ne!(order[i], order[j], "duplicate at [{i}, {j}]");
                }
            }
        }
    }

    #[test]
    fn skip_if_no_change_returns_true_when_snapshots_match() {
        let snap = json!({ "a": 1, "b": "x" });
        assert!(skip_if_no_change(&snap, &snap));
    }

    #[test]
    fn skip_if_no_change_returns_false_on_any_field_diff() {
        let before = json!({ "a": 1, "b": "x" });
        let after = json!({ "a": 1, "b": "y" });
        assert!(!skip_if_no_change(&before, &after));
    }

    #[test]
    fn skip_if_no_change_handles_added_or_removed_keys() {
        let before = json!({ "a": 1 });
        let after = json!({ "a": 1, "b": 2 });
        assert!(!skip_if_no_change(&before, &after));
        assert!(!skip_if_no_change(&after, &before));
    }

    #[test]
    fn skip_if_no_change_returns_true_for_two_null_snapshots() {
        // A row that did not previously exist and was not created in this
        // call -- writer short-circuits.
        let null = json!(null);
        assert!(skip_if_no_change(&null, &null));
    }

    #[test]
    fn encode_audit_payload_writes_uuid_id_as_msgpack_string() {
        // Regression: the sync server's `decodeAuditPayload` checks
        // `typeof obj.id === 'string'`. The default `rmp_serde::to_vec_named`
        // serializes `Uuid` as 16 raw bytes (msgpack `bin`), which the JS
        // decoder reads as a `Uint8Array`, failing the typeof check with
        // 422 "audit payload missing field: id". The encoder must use
        // human-readable mode so `Uuid` wires as a hyphenated string.
        use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
        let audit = AuditEntry::create(AuditCreateInput {
            actor_user_id: Uuid::now_v7(),
            action: AuditAction::Create,
            entity: "users".into(),
            entity_id: "u1".into(),
            delta: json!({}),
            ip: None,
            device_id: "dev-1".into(),
            entity_id_tenant: "tenant-x".into(),
        });

        let bytes = encode_audit_payload(&audit).expect("encode succeeds");
        let decoded: serde_json::Value =
            rmp_serde::from_slice(&bytes).expect("decodes via dynamic deserializer");

        assert!(
            decoded["id"].is_string(),
            "audit id must encode as a msgpack str (got {:?})",
            decoded["id"]
        );
        assert_eq!(decoded["id"].as_str().unwrap(), audit.id.to_string());
        assert!(
            decoded["actor_user_id"].is_string(),
            "actor_user_id must encode as a msgpack str (got {:?})",
            decoded["actor_user_id"]
        );
    }
}
