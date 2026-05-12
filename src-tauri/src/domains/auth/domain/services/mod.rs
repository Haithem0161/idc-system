//! Domain services for the auth bounded context.

pub mod password;

pub use password::{hash_password, verify_password};
