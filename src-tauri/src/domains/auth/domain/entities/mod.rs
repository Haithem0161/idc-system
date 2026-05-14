//! Auth-domain entities. `User` is the aggregate root.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::UserRole;
use crate::error::{AppError, AppResult};

/// Local mirror of the `users` row.
///
/// `password_hash` is never serialized to the frontend; see
/// `UserResponse::from(user)` in `commands.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: UserRole,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub dirty: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub origin_device_id: Option<String>,
    pub entity_id: String,
}

impl User {
    /// Domain factory. Validates email + name + role. The password is hashed
    /// in the service layer (the entity stays free of `argon2` to keep the
    /// domain dependency-free).
    pub fn try_new(
        email: &str,
        name: &str,
        role: UserRole,
        password_hash: String,
        entity_id: String,
        origin_device_id: Option<String>,
    ) -> AppResult<Self> {
        let email = normalize_email(email)?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("name required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            email,
            name,
            password_hash,
            role,
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            dirty: true,
            last_synced_at: None,
            origin_device_id,
            entity_id,
        })
    }

    pub fn with_updated_fields(
        mut self,
        name: Option<String>,
        email: Option<String>,
        role: Option<UserRole>,
    ) -> AppResult<Self> {
        if let Some(n) = name {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Err(AppError::Validation("name required".into()));
            }
            self.name = n;
        }
        if let Some(e) = email {
            self.email = normalize_email(&e)?;
        }
        if let Some(r) = role {
            self.role = r;
        }
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        Ok(self)
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

    pub fn with_new_password_hash(mut self, password_hash: String) -> Self {
        self.password_hash = password_hash;
        self.updated_at = Utc::now();
        self.version += 1;
        self.dirty = true;
        self
    }

    pub fn mark_logged_in(mut self) -> Self {
        let now = Utc::now();
        self.last_login_at = Some(now);
        self.updated_at = now;
        // Do not bump version for last_login_at -- not a syncable change.
        self.dirty = true;
        self
    }
}

/// Normalize an email: trim + lowercase. Rejects empty or missing `@`.
pub fn normalize_email(input: &str) -> AppResult<String> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return Err(AppError::Validation("valid email required".into()));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_hash() -> String {
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$placeholder".to_string()
    }

    #[test]
    fn try_new_normalizes_email_and_trims_name() {
        let u = User::try_new(
            "  TEST@Example.COM  ",
            "  Mariam  ",
            UserRole::Superadmin,
            fixed_hash(),
            "tenant-1".into(),
            Some("dev-A".into()),
        )
        .unwrap();
        assert_eq!(u.email, "test@example.com");
        assert_eq!(u.name, "Mariam");
        assert_eq!(u.role, UserRole::Superadmin);
        assert!(u.is_active);
        assert!(u.deleted_at.is_none());
        assert_eq!(u.version, 1);
        assert!(u.dirty);
        assert_eq!(u.entity_id, "tenant-1");
        assert_eq!(u.origin_device_id.as_deref(), Some("dev-A"));
    }

    #[test]
    fn try_new_rejects_email_without_at_sign() {
        let err = User::try_new(
            "not-an-email",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn try_new_rejects_empty_email() {
        let err = User::try_new(
            "   ",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn try_new_rejects_name_empty_after_trim() {
        let err = User::try_new(
            "u@x.io",
            "    ",
            UserRole::Accountant,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(msg) if msg.contains("name")));
    }

    #[test]
    fn try_new_accepts_each_of_three_roles() {
        for role in [
            UserRole::Superadmin,
            UserRole::Receptionist,
            UserRole::Accountant,
        ] {
            let u = User::try_new("a@b.io", "n", role, fixed_hash(), "t".into(), None).unwrap();
            assert_eq!(u.role, role);
        }
    }

    #[test]
    fn with_updated_fields_bumps_version_marks_dirty_and_updates_updated_at() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let original_updated_at = u.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let u2 = u
            .with_updated_fields(Some("New Name".into()), None, Some(UserRole::Accountant))
            .unwrap();
        assert_eq!(u2.version, 2);
        assert!(u2.dirty);
        assert_eq!(u2.name, "New Name");
        assert_eq!(u2.role, UserRole::Accountant);
        assert!(u2.updated_at > original_updated_at);
    }

    #[test]
    fn with_updated_fields_lowercases_new_email() {
        let u = User::try_new(
            "old@x.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let u2 = u
            .with_updated_fields(None, Some("NEW@X.io".into()), None)
            .unwrap();
        assert_eq!(u2.email, "new@x.io");
    }

    #[test]
    fn with_updated_fields_rejects_empty_name_after_trim() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let err = u
            .with_updated_fields(Some("   ".into()), None, None)
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn soft_deleted_sets_deleted_at_and_is_active_false_atomically() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let v0 = u.version;
        let d = u.soft_deleted();
        assert!(d.deleted_at.is_some());
        assert!(!d.is_active);
        assert_eq!(d.version, v0 + 1);
        assert!(d.dirty);
    }

    #[test]
    fn with_new_password_hash_rotates_hash_and_bumps_version() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let v0 = u.version;
        let h0 = u.password_hash.clone();
        let u2 = u.with_new_password_hash("$argon2id$NEW".into());
        assert_ne!(u2.password_hash, h0);
        assert_eq!(u2.version, v0 + 1);
        assert!(u2.dirty);
    }

    #[test]
    fn mark_logged_in_sets_last_login_at_without_bumping_version() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Receptionist,
            fixed_hash(),
            "t".into(),
            None,
        )
        .unwrap();
        let v0 = u.version;
        let u2 = u.mark_logged_in();
        assert!(u2.last_login_at.is_some());
        assert_eq!(u2.version, v0);
    }

    #[test]
    fn serialized_user_skips_password_hash_at_ipc_boundary() {
        let u = User::try_new(
            "a@b.io",
            "n",
            UserRole::Superadmin,
            "$argon2id$SENSITIVE".into(),
            "t".into(),
            None,
        )
        .unwrap();
        let v = serde_json::to_value(&u).unwrap();
        assert!(v.get("password_hash").is_none());
        assert!(!serde_json::to_string(&u)
            .unwrap()
            .contains("$argon2id$SENSITIVE"));
    }

    #[test]
    fn normalize_email_rejects_empty_and_missing_at() {
        assert!(normalize_email("").is_err());
        assert!(normalize_email("   ").is_err());
        assert!(normalize_email("noat").is_err());
        assert_eq!(normalize_email("  A@B.IO  ").unwrap(), "a@b.io");
    }
}
