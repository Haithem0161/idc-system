//! `Patient` aggregate (PRD §6.1.9).
//!
//! Identity-only entity: the only mutable field is `name`. Invariant 1
//! (`name` non-empty after trim, §7.9). Deletes are soft (tombstone).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patient {
    pub id: Uuid,
    pub name: String,
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
pub struct PatientNewInput {
    pub name: String,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

fn clean_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("patient name required".into()));
    }
    Ok(trimmed.to_string())
}

impl Patient {
    pub fn try_new(input: PatientNewInput) -> AppResult<Self> {
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id required".into()));
        }
        let name = clean_name(&input.name)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name,
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

    pub fn rename(mut self, new_name: &str) -> AppResult<Self> {
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("patient is deleted".into()));
        }
        let cleaned = clean_name(new_name)?;
        if cleaned == self.name {
            return Ok(self);
        }
        self.name = cleaned;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn soft_delete(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
    }
}
