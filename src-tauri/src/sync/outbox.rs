//! Outbox queue for pending mutations.
//!
//! Schema lives in a migration; columns: `op_id`, `entity`, `entity_id`,
//! `op` (`upsert`/`delete`), `payload` (MessagePack), `created_at`,
//! `attempts`, `next_attempt_at`, `last_error`.
