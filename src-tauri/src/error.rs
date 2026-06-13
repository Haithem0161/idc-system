//! Application error types.
//!
//! One `AppError` enum is the top-level error for every Tauri command and
//! background task. Serializes to the canonical `{ code, message, details? }`
//! shape so the frontend gets a stable contract (see PRD §5.2 /
//! `ErrorResponseSchema`).

use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not authenticated")]
    NotAuthenticated,

    #[error("session expired")]
    SessionExpired,

    #[error("validation error: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("sync server unavailable: {0}")]
    SyncUnavailable(String),

    /// The server rejected this app version with HTTP 426. The UI must prompt
    /// the user to upgrade; retrying the same version will never succeed.
    #[error("app upgrade required: {0}")]
    UpgradeRequired(String),

    /// DEF-007 G31: an operation that REQUIRES online connectivity
    /// (e.g. `auth::change_password`) was invoked while the device is
    /// offline. Distinct from `Network` (which means the call was attempted
    /// and the server was unreachable) -- `OfflineNotAllowed` means the
    /// caller decided the call MUST NOT be attempted at all because we
    /// already know we are offline. No HTTP round-trip is made.
    #[error("operation requires online connectivity")]
    OfflineNotAllowed,

    #[error("database error: {0}")]
    Database(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Canonical error code used by the frontend `errors:*` i18n keys and the
    /// server `ErrorResponseSchema.code` field.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAuthenticated => "NOT_AUTHENTICATED",
            Self::SessionExpired => "SESSION_EXPIRED",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Conflict(_) => "CONFLICT_PARKED",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Network(_) => "NETWORK_OFFLINE",
            Self::SyncUnavailable(_) => "SERVER_UNAVAILABLE",
            Self::UpgradeRequired(_) => "UPGRADE_REQUIRED",
            Self::OfflineNotAllowed => "OFFLINE_NOT_ALLOWED",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Configuration(_) => "CONFIGURATION_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => AppError::NotFound("row".into()),
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message().to_string();
                if msg.to_lowercase().contains("unique")
                    || msg.to_lowercase().contains("constraint")
                {
                    AppError::Conflict(msg)
                } else {
                    AppError::Database(msg)
                }
            }
            _ => AppError::Database(e.to_string()),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() || e.is_connect() {
            AppError::Network(e.to_string())
        } else {
            AppError::SyncUnavailable(e.to_string())
        }
    }
}

impl From<rmp_serde::encode::Error> for AppError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        AppError::Internal(format!("msgpack encode: {e}"))
    }
}

impl From<rmp_serde::decode::Error> for AppError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        AppError::Validation(format!("msgpack decode: {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("json: {e}"))
    }
}

impl From<uuid::Error> for AppError {
    fn from(e: uuid::Error) -> Self {
        AppError::Validation(format!("uuid: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_code_and_message() {
        let err = AppError::Conflict("op_id parked".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "CONFLICT_PARKED");
        assert_eq!(json["message"], "conflict: op_id parked");
    }
}
