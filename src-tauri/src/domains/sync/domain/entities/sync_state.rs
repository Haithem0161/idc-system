//! Singleton row in the `sync_state` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub pull_cursor: Option<String>,
    pub last_pulled_at: Option<DateTime<Utc>>,
    pub last_pushed_at: Option<DateTime<Utc>>,
    pub device_id: String,
}
