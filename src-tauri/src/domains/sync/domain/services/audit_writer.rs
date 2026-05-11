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
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::sync::domain::entities::audit_entry::AuditCreateInput;
use crate::domains::sync::domain::entities::{AuditEntry, OutboxOp};
use crate::domains::sync::domain::repositories::{AuditRepo, OutboxRepo};
use crate::domains::sync::domain::value_objects::AuditAction;
use crate::error::{AppError, AppResult};

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
        let audit_payload = rmp_serde::to_vec_named(&audit)?;
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
