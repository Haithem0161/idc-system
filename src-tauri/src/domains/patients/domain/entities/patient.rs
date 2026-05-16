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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> PatientNewInput {
        PatientNewInput {
            name: name.into(),
            entity_id: "t".into(),
            origin_device_id: Some("dev".into()),
        }
    }

    #[test]
    fn try_new_rejects_empty_or_whitespace_only_after_trim() {
        assert!(Patient::try_new(input("")).is_err());
        assert!(Patient::try_new(input("   ")).is_err());
    }

    #[test]
    fn try_new_trims_leading_and_trailing_whitespace() {
        let p = Patient::try_new(input("  Layla  ")).unwrap();
        assert_eq!(p.name, "Layla");
    }

    #[test]
    fn try_new_accepts_arabic_and_mixed_scripts_byte_for_byte() {
        let mixed = "Layla هاشم";
        let p = Patient::try_new(input(mixed)).unwrap();
        assert_eq!(p.name, mixed);
    }

    #[test]
    fn try_new_rejects_empty_entity_id() {
        let mut i = input("Layla");
        i.entity_id = "  ".into();
        assert!(Patient::try_new(i).is_err());
    }

    #[test]
    fn try_new_seeds_uuid_v7_and_version_1_dirty_true() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let bytes = p.id.as_bytes();
        assert_eq!((bytes[6] & 0xF0) >> 4, 7);
        assert_eq!(p.version, 1);
        assert!(p.dirty);
    }

    #[test]
    fn rename_bumps_version_and_updated_at() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let renamed = p.clone().rename("Layla H.").unwrap();
        assert_eq!(renamed.name, "Layla H.");
        assert_eq!(renamed.version, p.version + 1);
        assert!(renamed.updated_at >= p.updated_at);
    }

    #[test]
    fn rename_is_no_op_when_name_unchanged() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let renamed = p.clone().rename("Layla").unwrap();
        assert_eq!(renamed.version, p.version);
    }

    #[test]
    fn rename_rejects_empty_after_trim() {
        let p = Patient::try_new(input("Layla")).unwrap();
        assert!(p.rename("   ").is_err());
    }

    #[test]
    fn rename_rejects_when_deleted() {
        let p = Patient::try_new(input("Layla")).unwrap().soft_delete();
        assert!(p.rename("New").is_err());
    }

    #[test]
    fn soft_delete_marks_deleted_at_and_bumps_version() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let deleted = p.clone().soft_delete();
        assert!(deleted.deleted_at.is_some());
        assert_eq!(deleted.version, p.version + 1);
        assert!(deleted.dirty);
    }
}
