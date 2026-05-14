//! `Operator` entity (PRD §6.1.6).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operator {
    pub id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub base_cut_per_check_iqd: i64,
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
pub struct OperatorNewInput {
    pub name: String,
    pub phone: Option<String>,
    pub base_cut_per_check_iqd: i64,
    pub notes: Option<String>,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OperatorUpdate {
    pub name: Option<String>,
    pub phone: Option<Option<String>>,
    pub base_cut_per_check_iqd: Option<i64>,
    pub notes: Option<Option<String>>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl Operator {
    pub fn try_new(input: OperatorNewInput) -> AppResult<Self> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("operator name required".into()));
        }
        if input.base_cut_per_check_iqd < 0 {
            return Err(AppError::Validation(
                "base_cut_per_check_iqd must be non-negative".into(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name,
            phone: clean_optional(input.phone),
            base_cut_per_check_iqd: input.base_cut_per_check_iqd,
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

    pub fn with_updated_fields(mut self, patch: OperatorUpdate) -> AppResult<Self> {
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
        if let Some(b) = patch.base_cut_per_check_iqd {
            if b < 0 {
                return Err(AppError::Validation(
                    "base_cut_per_check_iqd must be non-negative".into(),
                ));
            }
            self.base_cut_per_check_iqd = b;
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

    fn input(name: &str, cut: i64) -> OperatorNewInput {
        OperatorNewInput {
            name: name.into(),
            phone: None,
            base_cut_per_check_iqd: cut,
            notes: None,
            entity_id: "t".into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn try_new_requires_non_empty_name() {
        assert!(Operator::try_new(input("", 0)).is_err());
        assert!(Operator::try_new(input("   ", 0)).is_err());
        assert!(Operator::try_new(input("X", 0)).is_ok());
    }

    #[test]
    fn try_new_rejects_negative_base_cut() {
        assert!(Operator::try_new(input("X", -1)).is_err());
        assert!(Operator::try_new(input("X", 0)).is_ok());
    }

    #[test]
    fn try_new_seeds_sync_columns_and_active() {
        let o = Operator::try_new(input("X", 100)).unwrap();
        assert_eq!(o.version, 1);
        assert!(o.dirty);
        assert!(o.is_active);
        assert!(o.deleted_at.is_none());
        assert_eq!(o.id.get_version_num(), 7);
    }

    #[test]
    fn try_new_cleans_optional_fields() {
        let o = Operator::try_new(OperatorNewInput {
            name: "X".into(),
            phone: Some("  ".into()),
            base_cut_per_check_iqd: 100,
            notes: Some("  ".into()),
            entity_id: "t".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert!(o.phone.is_none());
        assert!(o.notes.is_none());
    }

    #[test]
    fn with_updated_fields_revalidates_base_cut() {
        let o = Operator::try_new(input("X", 100)).unwrap();
        let res = o.with_updated_fields(OperatorUpdate {
            base_cut_per_check_iqd: Some(-1),
            ..Default::default()
        });
        assert!(res.is_err());
    }

    #[test]
    fn with_updated_fields_bumps_version_and_dirty() {
        let o = Operator::try_new(input("X", 100)).unwrap();
        let v0 = o.version;
        let after = o
            .with_updated_fields(OperatorUpdate {
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
        let o = Operator::try_new(input("X", 100)).unwrap();
        let v0 = o.version;
        let after = o.with_active(false);
        assert!(!after.is_active);
        assert_eq!(after.version, v0 + 1);
        assert!(after.dirty);
    }

    #[test]
    fn soft_deleted_marks_tombstone_and_inactive() {
        let o = Operator::try_new(input("X", 100)).unwrap();
        let after = o.soft_deleted();
        assert!(after.deleted_at.is_some());
        assert!(!after.is_active);
        assert!(after.dirty);
    }
}
