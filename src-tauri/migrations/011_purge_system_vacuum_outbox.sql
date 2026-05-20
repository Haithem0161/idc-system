-- Phase 9: one-time purge of stuck system-actor vacuum rows from `outbox`.
--
-- Before migration 010, the `AuditVacuumJob` enqueued every vacuum audit row
-- for sync push, including the daily scheduler runs whose `actor_user_id` is
-- the synthetic zero UUID (`SYSTEM_ACTOR_ID`). The server's `audit_log`
-- foreign key requires `actor_user_id` to reference an existing `users.id`,
-- so those rows 500'd repeatedly and blocked every later push from this
-- device (the engine drains the outbox in order, halts on the first failure,
-- and retries the same batch until the parked-at-10 cap).
--
-- The accompanying code change skips the enqueue when the actor is the
-- system zero UUID. This migration cleans up the rows that the old code
-- already wrote. It is idempotent: on a fresh DB the subquery returns no
-- rows, so DELETE is a no-op.
--
-- We match outbox rows by (entity = 'audit_log', entity_id = audit_log.id)
-- WHERE the audit row's actor is the zero UUID -- this is precise enough
-- that we never touch a real-user audit row.

DELETE FROM outbox
WHERE entity = 'audit_log'
  AND entity_id IN (
    SELECT id FROM audit_log
    WHERE actor_user_id = '00000000-0000-0000-0000-000000000000'
  );
