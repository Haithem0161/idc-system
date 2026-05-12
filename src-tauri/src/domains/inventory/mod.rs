//! Inventory operations bounded context (Phase 6 of the IDC plan).
//!
//! This domain owns the operational layer over `inventory_adjustments`:
//! receive / writeoff / count_correction. The `consume_visit` reason is
//! emitted from the visits bounded context (Phase 5 `Visit::lock`).
//!
//! Catalog CRUD for `inventory_items` and `inventory_consumption_map` remains
//! in `domains::catalog`. We re-use the `InventoryAdjustment` aggregate and
//! its repository trait from `domains::visits` instead of re-defining a
//! parallel set, keeping a single source of truth for the entity.

pub mod commands;
pub mod service;

pub use service::{
    AdjustmentInput, InventoryAdjustmentService, InventoryAdjustmentServiceConfig,
    InventoryItemWithStatus, ItemDetail, StockStatus,
};
