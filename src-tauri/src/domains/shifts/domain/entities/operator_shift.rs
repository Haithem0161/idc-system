//! `OperatorShift` entity (PRD §6.1.8).
//!
//! Lifecycle: open -> close (-> retroactive edit) -> soft_delete. Each
//! mutator returns a fresh value (no in-place writes). State invariants are
//! checked here and again by the SQLite CHECK.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Domain entity. Pure data; no I/O.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorShift {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub check_in_by_user_id: Uuid,
    pub check_out_by_user_id: Option<Uuid>,
    pub note: Option<String>,
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
pub struct OperatorShiftOpenInput {
    pub operator_id: Uuid,
    pub by_user_id: Uuid,
    pub note: Option<String>,
    pub entity_id: String,
    pub origin_device_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OperatorShiftEditInput {
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub note: Option<Option<String>>,
}

fn clean_optional(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl OperatorShift {
    /// Open a new shift `now`. Caller is responsible for verifying the
    /// operator is active and that no other shift is open for them.
    pub fn open(input: OperatorShiftOpenInput) -> AppResult<Self> {
        let now = Utc::now();
        if input.entity_id.trim().is_empty() {
            return Err(AppError::Validation("entity_id required".into()));
        }
        Ok(Self {
            id: Uuid::now_v7(),
            operator_id: input.operator_id,
            check_in_at: now,
            check_out_at: None,
            check_in_by_user_id: input.by_user_id,
            check_out_by_user_id: None,
            note: clean_optional(input.note),
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

    /// Close an open shift `at`. Rejects double-close, soft-deleted shifts,
    /// and out-of-order timestamps.
    pub fn close(mut self, by_user_id: Uuid, at: DateTime<Utc>) -> AppResult<Self> {
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("shift is deleted".into()));
        }
        if self.check_out_at.is_some() {
            return Err(AppError::Conflict(
                "shift already closed; reopen via retroactive edit".into(),
            ));
        }
        if at < self.check_in_at {
            return Err(AppError::Validation(
                "check_out_at must be >= check_in_at".into(),
            ));
        }
        self.check_out_at = Some(at);
        self.check_out_by_user_id = Some(by_user_id);
        self.updated_at = at;
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    /// Apply a retroactive `(check_in_at, check_out_at)` edit. Caller must
    /// already have enforced the role gate and overlap check.
    pub fn edit_times(mut self, input: OperatorShiftEditInput) -> AppResult<Self> {
        if self.deleted_at.is_some() {
            return Err(AppError::Validation("shift is deleted".into()));
        }
        if let Some(out_at) = input.check_out_at {
            if out_at < input.check_in_at {
                return Err(AppError::Validation(
                    "check_out_at must be >= check_in_at".into(),
                ));
            }
        }
        let now = Utc::now();
        if input.check_in_at > now {
            return Err(AppError::Validation(
                "check_in_at cannot be in the future".into(),
            ));
        }
        self.check_in_at = input.check_in_at;
        self.check_out_at = input.check_out_at;
        if let Some(note) = input.note {
            self.note = clean_optional(note);
        }
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        Ok(self)
    }

    pub fn soft_deleted(mut self) -> Self {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        self.version += 1;
        self.dirty = true;
        self
    }

    pub fn is_open(&self) -> bool {
        self.check_out_at.is_none() && self.deleted_at.is_none()
    }

    /// Duration in seconds for closed shifts; `None` while open.
    pub fn duration_seconds(&self) -> Option<i64> {
        self.check_out_at
            .map(|out| (out - self.check_in_at).num_seconds())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OperatorShift {
        OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: Some("morning shift".into()),
            entity_id: "tenant-x".into(),
            origin_device_id: Some("dev-1".into()),
        })
        .unwrap()
    }

    #[test]
    fn open_emits_an_open_shift() {
        let s = sample();
        assert!(s.is_open());
        assert_eq!(s.version, 1);
        assert!(s.dirty);
    }

    #[test]
    fn open_sets_check_in_now_and_clears_check_out() {
        let before = Utc::now();
        let s = sample();
        let after = Utc::now();
        assert!(s.check_in_at >= before && s.check_in_at <= after);
        assert!(s.check_out_at.is_none());
        assert!(s.check_out_by_user_id.is_none());
        assert!(s.deleted_at.is_none());
        assert!(s.last_synced_at.is_none());
        assert_eq!(s.entity_id, "tenant-x");
    }

    #[test]
    fn open_rejects_empty_entity_id() {
        let err = OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: None,
            entity_id: "   ".into(),
            origin_device_id: None,
        })
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn open_normalizes_whitespace_only_note_to_none() {
        let s = OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: Some("   ".into()),
            entity_id: "tenant-x".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert!(s.note.is_none());
    }

    #[test]
    fn open_trims_note() {
        let s = OperatorShift::open(OperatorShiftOpenInput {
            operator_id: Uuid::now_v7(),
            by_user_id: Uuid::now_v7(),
            note: Some("  morning  ".into()),
            entity_id: "tenant-x".into(),
            origin_device_id: None,
        })
        .unwrap();
        assert_eq!(s.note.as_deref(), Some("morning"));
    }

    #[test]
    fn close_sets_check_out_at_and_bumps_version() {
        let s = sample();
        let by = Uuid::now_v7();
        let v = s.version;
        let at = s.check_in_at + chrono::Duration::minutes(5);
        let closed = s.close(by, at).unwrap();
        assert_eq!(closed.check_out_at, Some(at));
        assert_eq!(closed.check_out_by_user_id, Some(by));
        assert_eq!(closed.version, v + 1);
        assert!(closed.dirty);
    }

    #[test]
    fn close_rejects_out_of_order() {
        let s = sample();
        let past = s.check_in_at - chrono::Duration::minutes(1);
        let err = s.close(Uuid::now_v7(), past).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn close_then_close_fails() {
        let s = sample();
        let by = Uuid::now_v7();
        let closed = s
            .clone()
            .close(by, s.check_in_at + chrono::Duration::minutes(5))
            .unwrap();
        let err = closed
            .close(by, s.check_in_at + chrono::Duration::minutes(10))
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[test]
    fn close_rejects_when_soft_deleted() {
        let s = sample().soft_deleted();
        let err = s
            .close(Uuid::now_v7(), Utc::now() + chrono::Duration::hours(1))
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn edit_rejects_future_check_in() {
        let s = sample();
        let future = Utc::now() + chrono::Duration::days(1);
        let err = s
            .edit_times(OperatorShiftEditInput {
                check_in_at: future,
                check_out_at: None,
                note: None,
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn edit_rejects_inverted_window() {
        let s = sample();
        let now = s.check_in_at;
        let earlier = now - chrono::Duration::minutes(10);
        let err = s
            .edit_times(OperatorShiftEditInput {
                check_in_at: now,
                check_out_at: Some(earlier),
                note: None,
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn edit_allows_reopen_when_check_out_at_none() {
        let by = Uuid::now_v7();
        let closed = sample()
            .close(by, Utc::now() + chrono::Duration::minutes(5))
            .unwrap();
        let reopened = closed
            .edit_times(OperatorShiftEditInput {
                check_in_at: Utc::now() - chrono::Duration::minutes(20),
                check_out_at: None,
                note: None,
            })
            .unwrap();
        assert!(reopened.check_out_at.is_none());
    }

    #[test]
    fn edit_replace_note_overwrites_keeping_none_when_clearing() {
        let s = sample();
        let v = s.version;
        let edited = s
            .edit_times(OperatorShiftEditInput {
                check_in_at: Utc::now() - chrono::Duration::minutes(5),
                check_out_at: None,
                note: Some(None),
            })
            .unwrap();
        assert!(edited.note.is_none());
        assert_eq!(edited.version, v + 1);
    }

    #[test]
    fn edit_keeps_note_when_input_note_is_none() {
        let s = sample();
        let edited = s
            .clone()
            .edit_times(OperatorShiftEditInput {
                check_in_at: Utc::now() - chrono::Duration::minutes(5),
                check_out_at: None,
                note: None,
            })
            .unwrap();
        assert_eq!(edited.note, s.note);
    }

    #[test]
    fn edit_replaces_note_with_some_some_value() {
        let s = sample();
        let edited = s
            .edit_times(OperatorShiftEditInput {
                check_in_at: Utc::now() - chrono::Duration::minutes(5),
                check_out_at: None,
                note: Some(Some("evening".into())),
            })
            .unwrap();
        assert_eq!(edited.note.as_deref(), Some("evening"));
    }

    #[test]
    fn edit_rejects_when_soft_deleted() {
        let s = sample().soft_deleted();
        let err = s
            .edit_times(OperatorShiftEditInput {
                check_in_at: Utc::now() - chrono::Duration::minutes(5),
                check_out_at: None,
                note: None,
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn soft_delete_bumps_version_and_clears_open_flag() {
        let s = sample();
        let v = s.version;
        let deleted = s.soft_deleted();
        assert!(!deleted.is_open());
        assert_eq!(deleted.version, v + 1);
        assert!(deleted.deleted_at.is_some());
        assert!(deleted.dirty);
    }

    #[test]
    fn is_open_matrix() {
        // open + live -> true
        let live_open = sample();
        assert!(live_open.is_open());
        // closed + live -> false
        let by = Uuid::now_v7();
        let closed = sample()
            .close(by, Utc::now() + chrono::Duration::minutes(5))
            .unwrap();
        assert!(!closed.is_open());
        // open + deleted -> false
        let open_deleted = sample().soft_deleted();
        assert!(!open_deleted.is_open());
        // closed + deleted -> false
        let closed_deleted = closed.soft_deleted();
        assert!(!closed_deleted.is_open());
    }

    #[test]
    fn duration_seconds_open_returns_none() {
        let s = sample();
        assert!(s.duration_seconds().is_none());
    }

    #[test]
    fn duration_seconds_closed_returns_positive() {
        let by = Uuid::now_v7();
        let s = sample();
        let closed = s
            .clone()
            .close(by, s.check_in_at + chrono::Duration::minutes(7))
            .unwrap();
        assert_eq!(closed.duration_seconds(), Some(7 * 60));
    }

    #[test]
    fn serde_round_trip_through_json_preserves_all_fields() {
        let by = Uuid::now_v7();
        let s = sample()
            .close(by, Utc::now() + chrono::Duration::minutes(3))
            .unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let back: OperatorShift = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, s.id);
        assert_eq!(back.operator_id, s.operator_id);
        assert_eq!(back.version, s.version);
        assert_eq!(back.check_out_at, s.check_out_at);
        assert_eq!(back.entity_id, s.entity_id);
        assert_eq!(back.note, s.note);
    }
}
