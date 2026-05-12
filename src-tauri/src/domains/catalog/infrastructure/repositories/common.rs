//! Shared helpers for catalog SQLite repositories.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub fn parse_dt(s: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("datetime: {e}")))
}

pub fn parse_dt_opt(s: Option<&str>) -> AppResult<Option<DateTime<Utc>>> {
    s.map(parse_dt).transpose()
}

pub fn parse_uuid(s: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|e| AppError::Validation(format!("uuid: {e}")))
}

pub fn parse_uuid_opt(s: Option<&str>) -> AppResult<Option<Uuid>> {
    s.map(parse_uuid).transpose()
}

pub fn dt_to_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

pub fn dt_opt_to_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

/// Translate a search query string into a SQLite LIKE-friendly pattern,
/// escaping `%` and `_`. Returns `"<query>%"` so we get prefix-match.
pub fn like_prefix(query: &str) -> String {
    let escaped: String = query
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c].into_iter(),
            _ => vec![c].into_iter(),
        })
        .collect();
    format!("{escaped}%")
}
