//! `Doctor` entity (PRD §6.1.4).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doctor {
    pub id: Uuid,
    pub name: String,
    pub specialty: Option<String>,
    pub phone: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
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
pub struct DoctorNewInput {
    pub name: String,
    pub specialty: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorUpdate {
    pub name: Option<String>,
    pub specialty: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub notes: Option<Option<String>>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl Doctor {
    pub fn try_new(input: DoctorNewInput) -> AppResult<Self> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("doctor name required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name,
            specialty: clean_optional(input.specialty),
            phone: clean_optional(input.phone),
            is_active: true,
            notes: clean_optional(input.notes),
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

    pub fn with_updated_fields(mut self, patch: DoctorUpdate) -> AppResult<Self> {
        if let Some(name) = patch.name {
            let n = name.trim().to_string();
            if n.is_empty() {
                return Err(AppError::Validation("name required".into()));
            }
            self.name = n;
        }
        if let Some(s) = patch.specialty {
            self.specialty = clean_optional(s);
        }
        if let Some(p) = patch.phone {
            self.phone = clean_optional(p);
        }
        if let Some(n) = patch.notes {
            self.notes = clean_optional(n);
        }
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn with_active(mut self, is_active: bool) -> Self {
        self.is_active = is_active;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }

    pub fn soft_deleted(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.is_active = false;
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
    }
}
