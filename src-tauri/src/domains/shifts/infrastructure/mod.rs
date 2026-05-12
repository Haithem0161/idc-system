//! Shifts infrastructure layer. SQLite-backed implementation of
//! `OperatorShiftRepo`.

pub mod repositories;

pub use repositories::SqliteOperatorShiftRepo;
