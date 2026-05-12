//! Catalog domain services. Pure helpers; service-layer orchestration that
//! touches I/O lives under `domains/catalog/service`.

pub mod pricing_resolver;

pub use pricing_resolver::{EffectivePriceQuery, PricingResolver};
