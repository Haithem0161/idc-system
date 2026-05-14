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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> DoctorNewInput {
        DoctorNewInput {
            name: name.into(),
            specialty: None,
            phone: None,
            notes: None,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_requires_non_empty_name_after_trim() {
        assert!(Doctor::try_new(input("")).is_err());
        assert!(Doctor::try_new(input("   ")).is_err());
        assert!(Doctor::try_new(input("Dr. X")).is_ok());
    }

    #[test]
    fn try_new_accepts_unicode_arabic_and_mixed_scripts() {
        let d = Doctor::try_new(input("د. Layla هاشم")).unwrap();
        assert_eq!(d.name, "د. Layla هاشم");
    }

    #[test]
    fn try_new_trims_optional_fields_and_drops_when_empty() {
        let d = Doctor::try_new(DoctorNewInput {
            name: "Dr. X".into(),
            specialty: Some("  ".into()),
            phone: Some("  ".into()),
            notes: Some("  ".into()),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert!(d.specialty.is_none());
        assert!(d.phone.is_none());
        assert!(d.notes.is_none());
    }

    #[test]
    fn try_new_preserves_non_empty_optionals_after_trim() {
        let d = Doctor::try_new(DoctorNewInput {
            name: "Dr. X".into(),
            specialty: Some("  Cardio  ".into()),
            phone: Some("  555  ".into()),
            notes: Some("  test  ".into()),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert_eq!(d.specialty.as_deref(), Some("Cardio"));
        assert_eq!(d.phone.as_deref(), Some("555"));
        assert_eq!(d.notes.as_deref(), Some("test"));
    }

    #[test]
    fn try_new_seeds_sync_columns_and_active() {
        let d = Doctor::try_new(input("X")).unwrap();
        assert_eq!(d.version, 1);
        assert!(d.dirty);
        assert!(d.is_active);
        assert!(d.deleted_at.is_none());
        assert_eq!(d.id.get_version_num(), 7);
    }

    #[test]
    fn with_updated_fields_bumps_version_and_dirty() {
        let d = Doctor::try_new(input("X")).unwrap();
        let v0 = d.version;
        let updated = d
            .with_updated_fields(DoctorUpdate {
                name: Some("Y".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(updated.name, "Y");
        assert_eq!(updated.version, v0 + 1);
        assert!(updated.dirty);
    }

    #[test]
    fn with_updated_fields_rejects_empty_name_patch() {
        let d = Doctor::try_new(input("X")).unwrap();
        let res = d.with_updated_fields(DoctorUpdate {
            name: Some("   ".into()),
            ..Default::default()
        });
        assert!(res.is_err());
    }

    #[test]
    fn with_updated_fields_can_clear_optional_fields_by_setting_empty_value() {
        let d = Doctor::try_new(DoctorNewInput {
            name: "X".into(),
            specialty: Some("Cardio".into()),
            phone: Some("555".into()),
            notes: Some("note".into()),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        let updated = d
            .with_updated_fields(DoctorUpdate {
                specialty: Some(None),
                phone: Some(None),
                notes: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert!(updated.specialty.is_none());
        assert!(updated.phone.is_none());
        assert!(updated.notes.is_none());
    }

    #[test]
    fn with_active_bumps_version_and_marks_dirty() {
        let d = Doctor::try_new(input("X")).unwrap();
        let v0 = d.version;
        let toggled = d.with_active(false);
        assert!(!toggled.is_active);
        assert!(toggled.dirty);
        assert_eq!(toggled.version, v0 + 1);
    }

    #[test]
    fn soft_deleted_clears_active_and_sets_tombstone() {
        let d = Doctor::try_new(input("X")).unwrap();
        let v0 = d.version;
        let after = d.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert!(!after.is_active);
        assert!(after.dirty);
        assert_eq!(after.version, v0 + 1);
    }
}
