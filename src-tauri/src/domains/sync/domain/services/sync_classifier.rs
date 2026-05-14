//! Pure classifiers for the sync engine push / pull loops.
//!
//! These helpers exist as standalone functions so the engine's behavioural
//! invariants (phase-01 §1.1) can be verified by fast, deterministic unit
//! tests without standing up a Tauri AppHandle, an HTTP server, or a SQLite
//! pool. The engine itself wraps these calls.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domains::sync::infrastructure::ServerConflict;

/// The action the engine should take for a single outbox op after a push
/// attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushAction {
    /// 2xx ack -- delete the local outbox row.
    Ack,
    /// 5xx transient error -- bump attempts and back off exponentially.
    Backoff,
    /// 401 -- refresh the token and retry the same op once.
    RefreshAndRetry,
    /// Server returned a conflict envelope -- park the row pending manual
    /// resolution. Returns the parking op_id so the engine can flip
    /// `outbox.parked = 1` for that exact row.
    Park { op_id: Uuid },
    /// Unsupported op encountered locally (e.g. `op = 'delete'` in v1).
    /// Engine should NOT ship it; instead surface a hard error.
    UnsupportedOp,
}

/// Phase-01 §7.17 + §4 push-step 4/5 classifier for an HTTP push response.
///
/// The caller passes the response's status code and the parsed list of
/// server-reported conflicts (may be empty even on 2xx). The function does
/// NOT touch the network, the DB, or any clock; it is a pure projection.
///
/// Behaviour:
/// - `409 CONFLICT` or any conflicts whose `op_id` matches `op_id` -> `Park`.
/// - `401 UNAUTHORIZED` -> `RefreshAndRetry`.
/// - `>=500` server error -> `Backoff`.
/// - `2xx` with no matching conflict -> `Ack`.
/// - Anything else -> `Backoff` (conservative).
pub fn classify_push_response(
    status_code: u16,
    op_id: Uuid,
    server_conflicts: &[ServerConflict],
) -> PushAction {
    if server_conflicts
        .iter()
        .any(|c| c.op_id == op_id.to_string())
    {
        return PushAction::Park { op_id };
    }
    match status_code {
        200..=299 => PushAction::Ack,
        401 => PushAction::RefreshAndRetry,
        409 => PushAction::Park { op_id },
        500..=599 => PushAction::Backoff,
        _ => PushAction::Backoff,
    }
}

/// Phase-01 §7.17 narrow helper: returns `Some(op_id)` when the conflict
/// envelope names this outbox row, otherwise `None`.
pub fn should_park_outbox_row(server_conflicts: &[ServerConflict], op_id: Uuid) -> Option<Uuid> {
    server_conflicts
        .iter()
        .any(|c| c.op_id == op_id.to_string())
        .then_some(op_id)
}

/// Phase-01 §7.15 guard: any local outbox row carrying an op other than
/// `upsert` must be rejected at the engine boundary (defence-in-depth next
/// to the SQL CHECK + entity-level construction guard).
pub fn handle_unsupported_op(op_kind: &str) -> Option<PushAction> {
    if op_kind == "upsert" {
        None
    } else {
        Some(PushAction::UnsupportedOp)
    }
}

/// Phase-01 §7.20 startup reconcile classifier.
///
/// Given a local outbox snapshot (list of `(op_id, attempts)` pairs) and the
/// server's `/sync/lookup-op` response (list of acked op_ids), return the
/// set of local rows whose ack we already missed and which can be deleted
/// without another push. Rows not in the server's `found` list are left
/// untouched (the regular retry loop handles them).
pub fn reconcile_outbox_lookup_response(
    local_rows: &[(Uuid, i32)],
    found_on_server: &[Uuid],
) -> Vec<Uuid> {
    let mut to_delete = Vec::new();
    for (op_id, attempts) in local_rows {
        // Only rows that have been pushed at least once can be acked --
        // attempts == 0 means the row was never sent.
        if *attempts > 0 && found_on_server.contains(op_id) {
            to_delete.push(*op_id);
        }
    }
    to_delete
}

/// Outcome of a delete-vs-edit reconciliation between a local row and an
/// incoming pull row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteVsEditOutcome {
    /// Keep the local row; do not apply the incoming change.
    KeepLocal,
    /// Apply the incoming change; the local row is overwritten.
    ApplyIncoming,
    /// Park the conflict for manual resolution.
    Park,
}

/// Phase-01 §7.16 delete-vs-edit reconciliation.
///
/// Inputs:
/// - `local_updated_at`, `local_deleted_at`: the local row's stamps.
/// - `incoming_updated_at`, `incoming_deleted_at`: the pulled row's stamps.
/// - `policy_is_manual`: whether the entity declares the `manual` conflict
///   policy (e.g. phase-02 `settings`, phase-05 `visits`). Manual policy
///   always parks regardless of timestamps.
///
/// Tie-breaking rule (per §7.16): when `updated_at` is equal, the side with
/// a non-null `deleted_at` wins (deletion preserved).
pub fn reconcile_delete_vs_edit_lww(
    local_updated_at: DateTime<Utc>,
    local_deleted_at: Option<DateTime<Utc>>,
    incoming_updated_at: DateTime<Utc>,
    incoming_deleted_at: Option<DateTime<Utc>>,
    policy_is_manual: bool,
) -> DeleteVsEditOutcome {
    if policy_is_manual {
        return DeleteVsEditOutcome::Park;
    }
    use std::cmp::Ordering;
    match local_updated_at.cmp(&incoming_updated_at) {
        Ordering::Greater => DeleteVsEditOutcome::KeepLocal,
        Ordering::Less => DeleteVsEditOutcome::ApplyIncoming,
        Ordering::Equal => match (local_deleted_at.is_some(), incoming_deleted_at.is_some()) {
            (true, false) => DeleteVsEditOutcome::KeepLocal,
            (false, true) => DeleteVsEditOutcome::ApplyIncoming,
            // Both deleted or both alive at the same instant -- keep local
            // (idempotent: the incoming will be re-pulled later if it has
            // a strictly newer version).
            _ => DeleteVsEditOutcome::KeepLocal,
        },
    }
}

/// Phase-01 §7.21 audit-log immutability guard for pulled audit_log rows.
/// The server should NEVER ship an audit row with `deleted_at != null`,
/// but the client defends in depth.
pub fn reconcile_audit_log(incoming_deleted_at: Option<DateTime<Utc>>) -> Result<(), &'static str> {
    if incoming_deleted_at.is_some() {
        Err("audit_log row carries deleted_at != null (AUDIT_IMMUTABLE)")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn conflict(op_id: &str) -> ServerConflict {
        ServerConflict {
            op_id: op_id.into(),
            entity: "audit_log".into(),
            entity_id: "row-1".into(),
            server_payload: serde_json::json!({}),
            local_payload: serde_json::json!({}),
            reason: "AUDIT_IMMUTABLE".into(),
        }
    }

    #[test]
    fn classify_push_response_acks_on_2xx_with_no_conflicts() {
        let op = Uuid::now_v7();
        assert_eq!(classify_push_response(200, op, &[]), PushAction::Ack);
        assert_eq!(classify_push_response(204, op, &[]), PushAction::Ack);
    }

    #[test]
    fn classify_push_response_parks_when_op_id_in_conflict_envelope() {
        let op = Uuid::now_v7();
        let action = classify_push_response(200, op, &[conflict(&op.to_string())]);
        assert_eq!(action, PushAction::Park { op_id: op });
    }

    #[test]
    fn classify_push_response_does_not_park_on_unrelated_conflict() {
        let op = Uuid::now_v7();
        let other = Uuid::now_v7();
        let action = classify_push_response(200, op, &[conflict(&other.to_string())]);
        assert_eq!(action, PushAction::Ack);
    }

    #[test]
    fn classify_push_response_refresh_and_retry_on_401() {
        assert_eq!(
            classify_push_response(401, Uuid::now_v7(), &[]),
            PushAction::RefreshAndRetry,
        );
    }

    #[test]
    fn classify_push_response_backoff_on_5xx() {
        for code in [500u16, 502, 503, 504] {
            assert_eq!(
                classify_push_response(code, Uuid::now_v7(), &[]),
                PushAction::Backoff,
            );
        }
    }

    #[test]
    fn classify_push_response_409_parks_even_without_envelope_entry() {
        let op = Uuid::now_v7();
        assert_eq!(
            classify_push_response(409, op, &[]),
            PushAction::Park { op_id: op },
        );
    }

    #[test]
    fn should_park_returns_some_op_id_on_match() {
        let op = Uuid::now_v7();
        let parked = should_park_outbox_row(&[conflict(&op.to_string())], op);
        assert_eq!(parked, Some(op));
    }

    #[test]
    fn should_park_returns_none_on_no_match() {
        let op = Uuid::now_v7();
        assert_eq!(should_park_outbox_row(&[], op), None);
    }

    #[test]
    fn handle_unsupported_op_passes_through_upsert() {
        assert_eq!(handle_unsupported_op("upsert"), None);
    }

    #[test]
    fn handle_unsupported_op_rejects_delete_in_v1() {
        // Phase-01 §7.15: `delete` reserved for Horizon-2 PII purge; never
        // valid on the v1 wire.
        assert_eq!(
            handle_unsupported_op("delete"),
            Some(PushAction::UnsupportedOp)
        );
        assert_eq!(
            handle_unsupported_op("noop"),
            Some(PushAction::UnsupportedOp)
        );
    }

    #[test]
    fn reconcile_lookup_deletes_pushed_rows_acked_by_server() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        let local = vec![(a, 1), (b, 1), (c, 1)];
        let to_delete = reconcile_outbox_lookup_response(&local, &[a, c]);
        assert_eq!(to_delete, vec![a, c]);
    }

    #[test]
    fn reconcile_lookup_skips_rows_with_zero_attempts() {
        // A row that has never been sent cannot have been acked by the
        // server; defending the optimisation that the engine skips
        // /sync/lookup-op when all attempts are zero (per §7.20).
        let a = Uuid::now_v7();
        let to_delete = reconcile_outbox_lookup_response(&[(a, 0)], &[a]);
        assert!(to_delete.is_empty());
    }

    #[test]
    fn reconcile_lookup_returns_empty_when_server_found_nothing() {
        let a = Uuid::now_v7();
        let to_delete = reconcile_outbox_lookup_response(&[(a, 5)], &[]);
        assert!(to_delete.is_empty());
    }

    #[test]
    fn delete_vs_edit_local_wins_when_local_updated_at_is_later() {
        let t1 = Utc.with_ymd_and_hms(2026, 5, 13, 10, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 5, 13, 11, 0, 0).unwrap();
        let outcome = reconcile_delete_vs_edit_lww(t2, Some(t2), t1, None, false);
        assert_eq!(outcome, DeleteVsEditOutcome::KeepLocal);
    }

    #[test]
    fn delete_vs_edit_incoming_wins_when_incoming_updated_at_is_later() {
        let t1 = Utc.with_ymd_and_hms(2026, 5, 13, 10, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 5, 13, 11, 0, 0).unwrap();
        let outcome = reconcile_delete_vs_edit_lww(t1, None, t2, Some(t2), false);
        assert_eq!(outcome, DeleteVsEditOutcome::ApplyIncoming);
    }

    #[test]
    fn delete_vs_edit_tie_goes_to_the_deleted_side() {
        // Phase-01 §7.16: equal updated_at, one has deleted_at != null ->
        // deletion wins (preserve the tombstone).
        let t = Utc.with_ymd_and_hms(2026, 5, 13, 10, 0, 0).unwrap();

        // Local has the deletion -> KeepLocal.
        let outcome = reconcile_delete_vs_edit_lww(t, Some(t), t, None, false);
        assert_eq!(outcome, DeleteVsEditOutcome::KeepLocal);

        // Incoming has the deletion -> ApplyIncoming.
        let outcome = reconcile_delete_vs_edit_lww(t, None, t, Some(t), false);
        assert_eq!(outcome, DeleteVsEditOutcome::ApplyIncoming);
    }

    #[test]
    fn delete_vs_edit_manual_policy_parks_regardless_of_timestamps() {
        let t1 = Utc.with_ymd_and_hms(2026, 5, 13, 10, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 5, 13, 11, 0, 0).unwrap();
        let outcome = reconcile_delete_vs_edit_lww(t2, None, t1, None, true);
        assert_eq!(outcome, DeleteVsEditOutcome::Park);
    }

    #[test]
    fn reconcile_audit_log_accepts_when_deleted_at_is_none() {
        assert!(reconcile_audit_log(None).is_ok());
    }

    #[test]
    fn reconcile_audit_log_rejects_when_deleted_at_set() {
        let t = Utc.with_ymd_and_hms(2026, 5, 13, 10, 0, 0).unwrap();
        let err = reconcile_audit_log(Some(t)).expect_err("must reject");
        assert!(err.contains("AUDIT_IMMUTABLE"));
    }
}
