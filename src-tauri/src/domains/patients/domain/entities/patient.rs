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
    /// Optional demographics. The new-visit flow leaves all of these `None`;
    /// they are captured/edited only from the Patients archive.
    pub phone: Option<String>,
    /// `"M"` or `"F"` when set; validated by `clean_sex`.
    pub sex: Option<String>,
    /// ISO `YYYY-MM-DD`. Age is derived in the UI.
    pub birth_date: Option<String>,
    /// Clinic file / medical-record number.
    pub file_no: Option<String>,
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
pub struct PatientNewInput {
    pub name: String,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

/// Demographic edit payload for `Patient::update_demographics`. Every field is
/// optional; an explicit `Some("")` clears the field (normalized to `None`),
/// while `None` is also treated as "clear". The command layer passes the full
/// set on every save, so this is a replace, not a partial patch.
#[derive(Debug, Clone, Default)]
pub struct PatientDemographicsInput {
    pub phone: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<String>,
    pub file_no: Option<String>,
    pub notes: Option<String>,
}

fn clean_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("patient name required".into()));
    }
    Ok(trimmed.to_string())
}

/// Trim an optional free-text field; empty-after-trim collapses to `None` so a
/// cleared input is stored as NULL rather than an empty string.
fn clean_opt(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Validate `sex`: accepts `M`/`F` (any case, trimmed); empty -> `None`;
/// anything else is a validation error.
fn clean_sex(raw: Option<&str>) -> AppResult<Option<String>> {
    match clean_opt(raw) {
        None => Ok(None),
        Some(s) => match s.to_ascii_uppercase().as_str() {
            "M" => Ok(Some("M".into())),
            "F" => Ok(Some("F".into())),
            _ => Err(AppError::Validation("sex must be 'M' or 'F'".into())),
        },
    }
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
            phone: None,
            sex: None,
            birth_date: None,
            file_no: None,
            notes: None,
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

    /// Replace the demographic fields. Validates `sex`, normalizes blanks to
    /// `None`, and bumps the sync columns. Refuses on a tombstoned patient.
    pub fn update_demographics(mut self, input: PatientDemographicsInput) -> AppResult<Self> {
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("patient is deleted".into()));
        }
        self.phone = clean_opt(input.phone.as_deref());
        self.sex = clean_sex(input.sex.as_deref())?;
        self.birth_date = clean_opt(input.birth_date.as_deref());
        self.file_no = clean_opt(input.file_no.as_deref());
        self.notes = clean_opt(input.notes.as_deref());
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    /// Inverse of `soft_delete`: clear the tombstone and bump the sync columns
    /// so the un-delete propagates.
    pub fn restore(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = None;
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
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

    #[test]
    fn try_new_leaves_demographics_none() {
        let p = Patient::try_new(input("Layla")).unwrap();
        assert!(p.phone.is_none());
        assert!(p.sex.is_none());
        assert!(p.birth_date.is_none());
        assert!(p.file_no.is_none());
        assert!(p.notes.is_none());
    }

    fn demo(sex: Option<&str>) -> PatientDemographicsInput {
        PatientDemographicsInput {
            phone: Some("0770 000 1234".into()),
            sex: sex.map(str::to_string),
            birth_date: Some("1990-05-01".into()),
            file_no: Some("F-42".into()),
            notes: Some("  recurring patient ".into()),
        }
    }

    #[test]
    fn update_demographics_sets_fields_trims_and_bumps_version() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let v0 = p.version;
        let u = p.update_demographics(demo(Some("f"))).unwrap();
        assert_eq!(u.phone.as_deref(), Some("0770 000 1234"));
        assert_eq!(u.sex.as_deref(), Some("F")); // lowercased input normalized
        assert_eq!(u.birth_date.as_deref(), Some("1990-05-01"));
        assert_eq!(u.file_no.as_deref(), Some("F-42"));
        assert_eq!(u.notes.as_deref(), Some("recurring patient")); // trimmed
        assert_eq!(u.version, v0 + 1);
        assert!(u.dirty);
    }

    #[test]
    fn update_demographics_blanks_collapse_to_none() {
        let p = Patient::try_new(input("Layla")).unwrap();
        let u = p
            .update_demographics(PatientDemographicsInput {
                phone: Some("   ".into()),
                sex: Some("".into()),
                birth_date: None,
                file_no: Some("".into()),
                notes: None,
            })
            .unwrap();
        assert!(u.phone.is_none());
        assert!(u.sex.is_none());
        assert!(u.file_no.is_none());
    }

    #[test]
    fn update_demographics_rejects_invalid_sex() {
        let p = Patient::try_new(input("Layla")).unwrap();
        assert!(p.update_demographics(demo(Some("X"))).is_err());
    }

    #[test]
    fn update_demographics_rejects_when_deleted() {
        let p = Patient::try_new(input("Layla")).unwrap().soft_delete();
        assert!(p.update_demographics(demo(Some("M"))).is_err());
    }

    #[test]
    fn restore_clears_deleted_at_and_bumps_version() {
        let deleted = Patient::try_new(input("Layla")).unwrap().soft_delete();
        let v = deleted.version;
        let restored = deleted.restore();
        assert!(restored.deleted_at.is_none());
        assert_eq!(restored.version, v + 1);
        assert!(restored.dirty);
    }
}
