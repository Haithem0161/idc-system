//! Application state for embedded mode.
//!
//! Stores auth token and user context received from Business OS via IPC.

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// User context received from Business OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub entity_id: String,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
}

/// Shared application state.
///
/// Thread-safe via `RwLock`. Stores the auth token and user context
/// provided by Business OS through IPC messages.
pub struct AppState {
    token: RwLock<Option<String>>,
    expires_at: RwLock<Option<i64>>,
    user: RwLock<Option<UserContext>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            token: RwLock::new(None),
            expires_at: RwLock::new(None),
            user: RwLock::new(None),
        }
    }

    /// Store the auth token received from Business OS.
    pub async fn set_current_token(&self, token: String, expires_at: i64) {
        *self.token.write().await = Some(token);
        *self.expires_at.write().await = Some(expires_at);
    }

    /// Get the current auth token.
    pub async fn get_current_token(&self) -> Option<String> {
        self.token.read().await.clone()
    }

    /// Get token expiry timestamp.
    pub async fn get_token_expires_at(&self) -> Option<i64> {
        *self.expires_at.read().await
    }

    /// Set the current user context.
    pub async fn set_current_user(&self, user: UserContext) {
        *self.user.write().await = Some(user);
    }

    /// Get the current user context.
    pub async fn get_current_user(&self) -> Option<UserContext> {
        self.user.read().await.clone()
    }

    /// Clear all auth state (on session expiry).
    pub async fn clear_auth(&self) {
        *self.token.write().await = None;
        *self.expires_at.write().await = None;
        *self.user.write().await = None;
    }

    /// Check if a user is currently authenticated.
    pub async fn is_authenticated(&self) -> bool {
        self.user.read().await.is_some()
    }
}
