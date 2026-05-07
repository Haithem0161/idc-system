//! Per-entity conflict resolution policies.
//!
//! Policies: `last-write-wins`, `field-merge`, `additive-only`, `manual`.
//! Each entity registers its policy at startup; the resolver dispatches by
//! entity name.
