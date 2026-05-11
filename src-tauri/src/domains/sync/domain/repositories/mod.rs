//! Repository ports (traits) for the sync bounded context.
//!
//! Implementations live in `infrastructure/repositories/`. Pure trait
//! definitions only -- no `sqlx`, no `tauri`.

pub mod audit_repo;
pub mod outbox_repo;
pub mod sync_state_repo;

pub use audit_repo::AuditRepo;
pub use outbox_repo::OutboxRepo;
pub use sync_state_repo::SyncStateRepo;
