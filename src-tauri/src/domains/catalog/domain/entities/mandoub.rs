//! `Mandoub` (representative) entity (Phase 12).
//!
//! A simple catalog record modeled on `Operator` but WITHOUT a stored cut: the
//! مندوب's per-visit cut (500 or 1000 IQD) is chosen on the visit, not on this
//! row. Name / phone / notes / is_active CRUD; superadmin-gated writes;
//! soft-delete; syncable with last-write-wins.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mandoub {
    pub id: Uuid,
    pub name: String,
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
pub struct MandoubNewInput {
    pub name: String,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MandoubUpdate {
    pub name: Option<String>,
    pub phone: Option<Option<String>>,
    pub notes: Option<Option<String>>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl Mandoub {
    pub fn try_new(input: MandoubNewInput) -> AppResult<Self> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("mandoub name required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name,
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

    pub fn with_updated_fields(mut self, patch: MandoubUpdate) -> AppResult<Self> {
        if let Some(name) = patch.name {
            let n = name.trim().to_string();
            if n.is_empty() {
                return Err(AppError::Validation("name required".into()));
            }
            self.name = n;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> MandoubNewInput {
        MandoubNewInput {
            name: name.into(),
            phone: None,
            notes: None,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_requires_non_empty_name() {
        assert!(Mandoub::try_new(input("")).is_err());
        assert!(Mandoub::try_new(input("   ")).is_err());
        assert!(Mandoub::try_new(input("X")).is_ok());
    }

    #[test]
    fn try_new_seeds_sync_columns_and_active() {
        let m = Mandoub::try_new(input("X")).unwrap();
        assert_eq!(m.version, 1);
        assert!(m.dirty);
        assert!(m.is_active);
        assert!(m.deleted_at.is_none());
        assert_eq!(m.id.get_version_num(), 7);
    }

    #[test]
    fn try_new_cleans_optional_fields() {
        let m = Mandoub::try_new(MandoubNewInput {
            name: "X".into(),
            phone: Some("  ".into()),
            notes: Some("  ".into()),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert!(m.phone.is_none());
        assert!(m.notes.is_none());
    }

    #[test]
    fn with_updated_fields_bumps_version_and_dirty() {
        let m = Mandoub::try_new(input("X")).unwrap();
        let v0 = m.version;
        let after = m
            .with_updated_fields(MandoubUpdate {
                name: Some("Y".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(after.name, "Y");
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn with_active_flips_flag_and_bumps_version() {
        let m = Mandoub::try_new(input("X")).unwrap();
        let v0 = m.version;
        let after = m.with_active(false);
        assert!(!after.is_active);
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn soft_deleted_marks_tombstone_and_inactive() {
        let m = Mandoub::try_new(input("X")).unwrap();
        let after = m.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert!(!after.is_active);
        assert!(after.dirty);
    }
}
