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
