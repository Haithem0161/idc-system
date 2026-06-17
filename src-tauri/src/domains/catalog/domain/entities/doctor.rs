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
    /// Doctor-level default cut kind: `"pct"` or `"fixed"` when set. Applied by
    /// the money engine when no per-check `DoctorCheckPricing` row matches.
    /// `None` means the doctor has no default (cut falls to 0 without a
    /// per-check row). Always set/cleared together with `default_cut_value`.
    pub default_cut_kind: Option<String>,
    /// Doctor-level default cut value: a percentage (0..=100) when kind is
    /// `"pct"`, or an absolute IQD amount (>= 0) when kind is `"fixed"`.
    pub default_cut_value: Option<i64>,
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
    pub default_cut_kind: Option<String>,
    pub default_cut_value: Option<i64>,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorUpdate {
    pub name: Option<String>,
    pub specialty: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    /// Outer `Some` = caller is changing the default cut; inner pair is the new
    /// `(kind, value)` (or `None` to clear it). Outer `None` = leave as-is.
    pub default_cut: Option<Option<(String, i64)>>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Validate and normalize a doctor default cut. Accepts `(kind, value)` where
/// kind is `pct` (value 0..=100) or `fixed` (value >= 0; IQD). Both halves are
/// required together. Returns the normalized `(kind, value)` or a validation
/// error. Mirrors the per-check cut rules in `money_math` so the default and
/// the override share one contract.
fn clean_default_cut(
    kind: Option<String>,
    value: Option<i64>,
) -> AppResult<(Option<String>, Option<i64>)> {
    match (kind, value) {
        (None, None) => Ok((None, None)),
        (Some(k), Some(v)) => match k.trim().to_lowercase().as_str() {
            "pct" => {
                if !(0..=100).contains(&v) {
                    return Err(AppError::Validation(
                        "default cut percentage must be 0..=100".into(),
                    ));
                }
                Ok((Some("pct".into()), Some(v)))
            }
            "fixed" => {
                if v < 0 {
                    return Err(AppError::Validation(
                        "default cut amount must be non-negative".into(),
                    ));
                }
                Ok((Some("fixed".into()), Some(v)))
            }
            _ => Err(AppError::Validation(
                "default cut kind must be 'pct' or 'fixed'".into(),
            )),
        },
        _ => Err(AppError::Validation(
            "default cut requires both a kind and a value".into(),
        )),
    }
}

impl Doctor {
    pub fn try_new(input: DoctorNewInput) -> AppResult<Self> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("doctor name required".into()));
        }
        let (default_cut_kind, default_cut_value) =
            clean_default_cut(input.default_cut_kind, input.default_cut_value)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            name,
            specialty: clean_optional(input.specialty),
            phone: clean_optional(input.phone),
            is_active: true,
            notes: clean_optional(input.notes),
            default_cut_kind,
            default_cut_value,
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
        if let Some(cut) = patch.default_cut {
            let (kind, value) = match cut {
                Some((k, v)) => (Some(k), Some(v)),
                None => (None, None),
            };
            let (k, v) = clean_default_cut(kind, value)?;
            self.default_cut_kind = k;
            self.default_cut_value = v;
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
            default_cut_kind: None,
            default_cut_value: None,
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
            default_cut_kind: None,
            default_cut_value: None,
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
            default_cut_kind: None,
            default_cut_value: None,
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
            default_cut_kind: None,
            default_cut_value: None,
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

    fn input_with_cut(name: &str, kind: Option<&str>, value: Option<i64>) -> DoctorNewInput {
        DoctorNewInput {
            default_cut_kind: kind.map(str::to_string),
            default_cut_value: value,
            ..input(name)
        }
    }

    #[test]
    fn try_new_accepts_and_normalizes_pct_default_cut() {
        let d = Doctor::try_new(input_with_cut("X", Some("PCT"), Some(15))).unwrap();
        assert_eq!(d.default_cut_kind.as_deref(), Some("pct"));
        assert_eq!(d.default_cut_value, Some(15));
    }

    #[test]
    fn try_new_accepts_fixed_default_cut() {
        let d = Doctor::try_new(input_with_cut("X", Some("fixed"), Some(20000))).unwrap();
        assert_eq!(d.default_cut_kind.as_deref(), Some("fixed"));
        assert_eq!(d.default_cut_value, Some(20000));
    }

    #[test]
    fn try_new_rejects_pct_out_of_range() {
        assert!(Doctor::try_new(input_with_cut("X", Some("pct"), Some(101))).is_err());
        assert!(Doctor::try_new(input_with_cut("X", Some("pct"), Some(-1))).is_err());
    }

    #[test]
    fn try_new_rejects_negative_fixed_cut() {
        assert!(Doctor::try_new(input_with_cut("X", Some("fixed"), Some(-1))).is_err());
    }

    #[test]
    fn try_new_rejects_unknown_cut_kind() {
        assert!(Doctor::try_new(input_with_cut("X", Some("flat"), Some(10))).is_err());
    }

    #[test]
    fn try_new_rejects_partial_default_cut() {
        // kind without value, or value without kind, are both invalid.
        assert!(Doctor::try_new(input_with_cut("X", Some("pct"), None)).is_err());
        assert!(Doctor::try_new(input_with_cut("X", None, Some(10))).is_err());
    }

    #[test]
    fn try_new_allows_no_default_cut() {
        let d = Doctor::try_new(input_with_cut("X", None, None)).unwrap();
        assert!(d.default_cut_kind.is_none());
        assert!(d.default_cut_value.is_none());
    }

    #[test]
    fn with_updated_fields_sets_and_clears_default_cut() {
        let d = Doctor::try_new(input("X")).unwrap();
        let set = d
            .with_updated_fields(DoctorUpdate {
                default_cut: Some(Some(("pct".into(), 30))),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(set.default_cut_kind.as_deref(), Some("pct"));
        assert_eq!(set.default_cut_value, Some(30));

        let cleared = set
            .with_updated_fields(DoctorUpdate {
                default_cut: Some(None),
                ..Default::default()
            })
            .unwrap();
        assert!(cleared.default_cut_kind.is_none());
        assert!(cleared.default_cut_value.is_none());
    }

    #[test]
    fn with_updated_fields_rejects_invalid_default_cut() {
        let d = Doctor::try_new(input("X")).unwrap();
        let res = d.with_updated_fields(DoctorUpdate {
            default_cut: Some(Some(("pct".into(), 200))),
            ..Default::default()
        });
        assert!(res.is_err());
    }
}
