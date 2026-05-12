//! Settings bounded context.

pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod service;

pub use domain::value_objects::SettingValue;
