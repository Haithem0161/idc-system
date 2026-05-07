//! Business OS IPC message types.
//!
//! Wire format: 4-byte big-endian length prefix followed by MessagePack payload.
//! All messages have a common envelope with version and payload.

use serde::{Deserialize, Serialize};

/// Protocol version (currently 1).
pub const PROTOCOL_VERSION: u8 = 1;

/// Envelope for all IPC messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope {
    pub version: u8,
    pub payload: IpcPayload,
}

/// Union of all possible IPC payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcPayload {
    // Outbound messages (app -> Business OS)
    Connect {
        run_id: String,
    },
    EmbeddedReady {
        port: u16,
        base_path: String,
    },
    Pong {
        timestamp: i64,
    },
    NavigationChanged {
        path: String,
        title: String,
    },
    /// Request a fresh token from Business OS.
    TokenRefreshRequest {},

    // Inbound messages (Business OS -> app)
    ConnectAck {
        success: bool,
        error: Option<String>,
    },
    EmbeddedAck {
        success: bool,
        webview_label: Option<String>,
        error: Option<String>,
    },
    Ping {
        timestamp: i64,
    },
    Shutdown {
        reason: String,
    },
    NavigateTo {
        path: String,
    },
    AuthToken {
        token: String,
        expires_at: i64,
    },
    EntityContext {
        entity_id: String,
        entity_name: String,
        #[serde(default)]
        role: Option<String>,
    },
    /// Response to TokenRefreshRequest.
    TokenRefreshResponse {
        token: Option<String>,
        expires_at: Option<i64>,
        error: Option<String>,
    },
    /// Error notification from Business OS (e.g., SESSION_EXPIRED).
    Error {
        code: String,
        message: String,
    },
}

impl IpcEnvelope {
    /// Create a new envelope with the current protocol version.
    pub fn new(payload: IpcPayload) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }

    /// Create a Connect message.
    pub fn connect(run_id: impl Into<String>) -> Self {
        Self::new(IpcPayload::Connect {
            run_id: run_id.into(),
        })
    }

    /// Create an EmbeddedReady message.
    pub fn embedded_ready(port: u16, base_path: impl Into<String>) -> Self {
        Self::new(IpcPayload::EmbeddedReady {
            port,
            base_path: base_path.into(),
        })
    }

    /// Create a Pong message.
    pub fn pong(timestamp: i64) -> Self {
        Self::new(IpcPayload::Pong { timestamp })
    }

    /// Create a TokenRefreshRequest message.
    pub fn token_refresh_request() -> Self {
        Self::new(IpcPayload::TokenRefreshRequest {})
    }
}
