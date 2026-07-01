//! sqlx-backed implementation of `UserRepo`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::Tx;
use crate::domains::auth::domain::entities::User;
use crate::domains::auth::domain::repositories::{UserListFilter, UserRepo};
use crate::domains::auth::domain::value_objects::UserRole;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct SqliteUserRepo {
    pool: SqlitePool,
}

impl SqliteUserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepo for SqliteUserRepo {
    async fn upsert(&self, tx: &mut Tx<'_>, user: &User) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO users (\
                id, email, name, password_hash, role, is_active, last_login_at, \
                created_at, updated_at, deleted_at, version, dirty, last_synced_at, \
                origin_device_id, entity_id\
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET \
               email = excluded.email, \
               name = excluded.name, \
               password_hash = excluded.password_hash, \
               role = excluded.role, \
               is_active = excluded.is_active, \
               last_login_at = excluded.last_login_at, \
               updated_at = excluded.updated_at, \
               deleted_at = excluded.deleted_at, \
               version = excluded.version, \
               dirty = excluded.dirty",
        )
        .bind(user.id.to_string())
        .bind(&user.email)
        .bind(&user.name)
        .bind(&user.password_hash)
        .bind(user.role.as_str())
        .bind(user.is_active as i64)
        .bind(user.last_login_at.map(|d| d.to_rfc3339()))
        .bind(user.created_at.to_rfc3339())
        .bind(user.updated_at.to_rfc3339())
        .bind(user.deleted_at.map(|d| d.to_rfc3339()))
        .bind(user.version)
        .bind(user.dirty as i64)
        .bind(user.last_synced_at.map(|d| d.to_rfc3339()))
        .bind(user.origin_device_id.as_deref())
        .bind(&user.entity_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<User>> {
        let row: Option<UserRow> =
            sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(UserRow::into_domain).transpose()
    }

    async fn get_by_email(&self, email: &str, entity_id: &str) -> AppResult<Option<User>> {
        let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT * FROM users WHERE email = ? AND entity_id = ? AND deleted_at IS NULL",
        )
        .bind(email.to_lowercase())
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(UserRow::into_domain).transpose()
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        // Fetch up to two matches so we can detect (and refuse) an ambiguous
        // email that exists under more than one tenant. Exactly one match ->
        // resolve it; zero or many -> None (the caller treats that as "not
        // authenticated" rather than guessing a tenant).
        let rows: Vec<UserRow> = sqlx::query_as::<_, UserRow>(
            "SELECT * FROM users WHERE email = ? AND deleted_at IS NULL LIMIT 2",
        )
        .bind(email.to_lowercase())
        .fetch_all(&self.pool)
        .await?;
        if rows.len() == 1 {
            rows.into_iter()
                .next()
                .map(UserRow::into_domain)
                .transpose()
        } else {
            Ok(None)
        }
    }

    async fn list(&self, filter: UserListFilter) -> AppResult<Vec<User>> {
        let rows: Vec<UserRow> = if filter.include_inactive {
            sqlx::query_as::<_, UserRow>(
                "SELECT * FROM users WHERE deleted_at IS NULL ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, UserRow>(
                "SELECT * FROM users WHERE deleted_at IS NULL AND is_active = 1 ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(UserRow::into_domain).collect()
    }

    async fn count(&self) -> AppResult<u32> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(n.max(0) as u32)
    }

    async fn list_all_for_resync(&self) -> AppResult<Vec<User>> {
        let rows: Vec<UserRow> =
            sqlx::query_as::<_, UserRow>("SELECT * FROM users ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(UserRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    email: String,
    name: String,
    password_hash: String,
    role: String,
    is_active: i64,
    last_login_at: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
    dirty: i64,
    last_synced_at: Option<String>,
    origin_device_id: Option<String>,
    entity_id: String,
}

impl UserRow {
    fn into_domain(self) -> AppResult<User> {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("datetime: {e}")))
        };
        let role = UserRole::parse(&self.role)
            .ok_or_else(|| AppError::Validation(format!("invalid role: {}", self.role)))?;
        Ok(User {
            id: Uuid::parse_str(&self.id)?,
            email: self.email,
            name: self.name,
            password_hash: self.password_hash,
            role,
            is_active: self.is_active != 0,
            last_login_at: self.last_login_at.as_deref().map(parse_dt).transpose()?,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
            deleted_at: self.deleted_at.as_deref().map(parse_dt).transpose()?,
            version: self.version,
            dirty: self.dirty != 0,
            last_synced_at: self.last_synced_at.as_deref().map(parse_dt).transpose()?,
            origin_device_id: self.origin_device_id,
            entity_id: self.entity_id,
        })
    }
}
