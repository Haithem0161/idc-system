//! `InventoryAdjustment` aggregate (PRD §6.1.14).
//!
//! Append-only: rows are never edited or hard-deleted (§7.33 enforces at
//! the SQLite layer with a BEFORE UPDATE trigger).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentReason {
    Receive,
    Writeoff,
    CountCorrection,
    ConsumeVisit,
}

impl AdjustmentReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receive => "receive",
            Self::Writeoff => "writeoff",
            Self::CountCorrection => "count_correction",
            Self::ConsumeVisit => "consume_visit",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "receive" => Some(Self::Receive),
            "writeoff" => Some(Self::Writeoff),
            "count_correction" => Some(Self::CountCorrection),
            "consume_visit" => Some(Self::ConsumeVisit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryAdjustment {
    pub id: Uuid,
    pub item_id: Uuid,
    pub delta: i64,
    pub reason: AdjustmentReason,
    pub visit_id: Option<Uuid>,
    pub note: Option<String>,
    pub by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

#[derive(Debug, Clone)]
pub struct AdjustmentNewInput {
    pub item_id: Uuid,
    pub delta: i64,
    pub reason: AdjustmentReason,
    pub visit_id: Option<Uuid>,
    pub note: Option<String>,
    pub by_user_id: Uuid,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

impl InventoryAdjustment {
    pub fn try_new(input: AdjustmentNewInput) -> AppResult<Self> {
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id required".into()));
        }
        match input.reason {
            AdjustmentReason::Receive if input.delta <= 0 => {
                return Err(AppError::Validation(
                    "receive adjustments must have positive delta".into(),
                ));
            }
            AdjustmentReason::Writeoff if input.delta >= 0 => {
                return Err(AppError::Validation(
                    "writeoff adjustments must have negative delta".into(),
                ));
            }
            AdjustmentReason::ConsumeVisit if input.visit_id.is_none() => {
                return Err(AppError::Validation(
                    "consume_visit adjustments require visit_id".into(),
                ));
            }
            _ => {}
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            item_id: input.item_id,
            delta: input.delta,
            reason: input.reason,
            visit_id: input.visit_id,
            note: input.note,
            by_user_id: input.by_user_id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id: input.origin_device_id,
            entity_id: input.entity_id,
        })
    }
}
