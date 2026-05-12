//! Auth bounded context: users, sessions, login (online + offline), refresh.

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;
pub mod user_service;

pub use domain::value_objects::{LoginMode, UserRole};
pub use service::AuthService;
pub use user_service::{UserCreateInput, UserService, UserUpdateInput};
