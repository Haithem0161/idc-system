//! Business OS IPC client.
//!
//! Connects to Business OS via TCP and handles the MessagePack protocol.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;

use crate::error::{AppError, AppResult};
use crate::state::{AppState, UserContext};

use super::messages::{IpcEnvelope, IpcPayload};

/// Maximum message size (1MB) to prevent memory exhaustion.
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// IPC client for Business OS communication.
pub struct IpcClient {
    stream: TcpStream,
    app_state: Arc<AppState>,
    shutdown_tx: broadcast::Sender<()>,
}

impl IpcClient {
    /// Connect to Business OS IPC server.
    pub async fn connect(
        ipc_port: u16,
        app_state: Arc<AppState>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> AppResult<Self> {
        let addr = format!("127.0.0.1:{}", ipc_port);
        tracing::info!("Connecting to Business OS IPC at {}", addr);

        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            AppError::Internal(format!("Failed to connect to Business OS IPC: {}", e))
        })?;

        tracing::info!("Connected to Business OS IPC");

        Ok(Self {
            stream,
            app_state,
            shutdown_tx,
        })
    }

    /// Send a message to Business OS.
    pub async fn send(&mut self, msg: &IpcEnvelope) -> AppResult<()> {
        let bytes = rmp_serde::to_vec(msg)
            .map_err(|e| AppError::Internal(format!("MessagePack encode error: {}", e)))?;

        let len = (bytes.len() as u32).to_be_bytes();
        self.stream.write_all(&len).await.map_err(|e| {
            AppError::Internal(format!("Failed to write message length: {}", e))
        })?;
        self.stream.write_all(&bytes).await.map_err(|e| {
            AppError::Internal(format!("Failed to write message body: {}", e))
        })?;
        self.stream.flush().await.map_err(|e| {
            AppError::Internal(format!("Failed to flush stream: {}", e))
        })?;

        Ok(())
    }

    /// Receive a message from Business OS.
    pub async fn receive(&mut self) -> AppResult<IpcEnvelope> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await.map_err(|e| {
            AppError::Internal(format!("Failed to read message length: {}", e))
        })?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_MESSAGE_SIZE {
            return Err(AppError::Internal(format!(
                "IPC message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            )));
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await.map_err(|e| {
            AppError::Internal(format!("Failed to read message body: {}", e))
        })?;

        rmp_serde::from_slice(&buf)
            .map_err(|e| AppError::Internal(format!("MessagePack decode error: {}", e)))
    }

    /// Perform the connection handshake with Business OS.
    pub async fn handshake(&mut self, run_id: &str, http_port: u16) -> AppResult<()> {
        // Send Connect message
        self.send(&IpcEnvelope::connect(run_id)).await?;
        tracing::debug!("Sent Connect message with run_id: {}", run_id);

        // Wait for ConnectAck
        let ack = self.receive().await?;
        match ack.payload {
            IpcPayload::ConnectAck { success, error } => {
                if !success {
                    return Err(AppError::Internal(
                        error.unwrap_or_else(|| "Connection rejected by Business OS".to_string()),
                    ));
                }
                tracing::info!("Received ConnectAck from Business OS");
            }
            other => {
                return Err(AppError::Internal(format!(
                    "Expected ConnectAck, got {:?}",
                    other
                )));
            }
        }

        // Send EmbeddedReady
        self.send(&IpcEnvelope::embedded_ready(http_port, "/"))
            .await?;
        tracing::info!(
            "Sent EmbeddedReady - HTTP server on port {}",
            http_port
        );

        Ok(())
    }

    /// Run the message loop, handling incoming messages until shutdown.
    pub async fn run_message_loop(mut self) -> AppResult<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = self.receive() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_message(msg).await {
                                tracing::error!("Error handling IPC message: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("IPC read error: {}", e);
                            // Connection lost, trigger shutdown
                            let _ = self.shutdown_tx.send(());
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("IPC client received shutdown signal");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single incoming message from Business OS.
    async fn handle_message(&mut self, msg: IpcEnvelope) -> AppResult<()> {
        tracing::info!("IPC message received: {}", match &msg.payload {
            IpcPayload::Ping { .. } => "Ping",
            IpcPayload::Shutdown { .. } => "Shutdown",
            IpcPayload::NavigateTo { .. } => "NavigateTo",
            IpcPayload::AuthToken { .. } => "AuthToken",
            IpcPayload::EntityContext { .. } => "EntityContext",
            IpcPayload::TokenRefreshResponse { .. } => "TokenRefreshResponse",
            IpcPayload::Error { .. } => "Error",
            IpcPayload::EmbeddedAck { .. } => "EmbeddedAck",
            IpcPayload::ConnectAck { .. } => "ConnectAck",
            _ => "Other",
        });

        match msg.payload {
            IpcPayload::Ping { timestamp } => {
                tracing::debug!("Received Ping (timestamp: {}), sending Pong", timestamp);
                self.send(&IpcEnvelope::pong(timestamp)).await?;
            }

            IpcPayload::Shutdown { reason } => {
                tracing::info!("Received Shutdown from Business OS: {}", reason);
                let _ = self.shutdown_tx.send(());
            }

            IpcPayload::NavigateTo { path } => {
                tracing::info!("Received NavigateTo: {}", path);
                // Navigation is handled by Business OS webview directly.
            }

            IpcPayload::AuthToken { token, expires_at } => {
                tracing::info!(
                    "Received AuthToken from Business OS (len={}, expires_at={})",
                    token.len(),
                    expires_at
                );
                // Store the token for the /api/auth endpoint
                self.app_state
                    .set_current_token(token, expires_at)
                    .await;
                tracing::info!("Auth token stored from Business OS");
            }

            IpcPayload::EntityContext {
                entity_id,
                entity_name,
                role,
            } => {
                tracing::info!(
                    "Received EntityContext: {} ({}) role={:?}",
                    entity_name,
                    entity_id,
                    role
                );

                let user_context = UserContext {
                    user_id: String::new(), // Will be set from JWT if needed
                    entity_id,
                    email: entity_name.clone(),
                    name: Some(entity_name),
                    role: role.unwrap_or_else(|| "member".to_string()),
                };

                self.app_state.set_current_user(user_context).await;
                tracing::info!("User context set from EntityContext");
            }

            IpcPayload::TokenRefreshResponse {
                token,
                expires_at,
                error,
            } => {
                if let Some(err) = error {
                    tracing::error!("Token refresh failed from Business OS: {}", err);
                    if err == "NOT_AUTHENTICATED" {
                        tracing::warn!("Business OS reports not authenticated, clearing auth state");
                        self.app_state.clear_auth().await;
                    }
                } else if let Some(new_token) = token {
                    tracing::info!("Token refreshed from Business OS");
                    let exp = expires_at.unwrap_or(0);
                    self.app_state
                        .set_current_token(new_token, exp)
                        .await;
                }
            }

            IpcPayload::Error { code, message } => {
                tracing::error!(
                    "Error from Business OS: {} - {}",
                    code,
                    message
                );
                if code == "SESSION_EXPIRED" {
                    tracing::warn!(
                        "Session expired: {}. Clearing auth state, waiting for re-authentication.",
                        message
                    );
                    self.app_state.clear_auth().await;
                }
            }

            IpcPayload::EmbeddedAck {
                success,
                webview_label,
                error,
            } => {
                if success {
                    tracing::info!(
                        "Business OS acknowledged embedded mode, webview: {:?}",
                        webview_label
                    );
                } else {
                    tracing::error!(
                        "Business OS rejected embedded mode: {:?}",
                        error.unwrap_or_else(|| "Unknown error".to_string())
                    );
                }
            }

            IpcPayload::ConnectAck { .. } => {
                tracing::debug!("Received unexpected ConnectAck in message loop");
            }

            // Outbound message types should not be received
            IpcPayload::Connect { .. }
            | IpcPayload::EmbeddedReady { .. }
            | IpcPayload::Pong { .. }
            | IpcPayload::NavigationChanged { .. }
            | IpcPayload::TokenRefreshRequest { .. } => {
                tracing::warn!("Received unexpected outbound message type from Business OS");
            }
        }

        Ok(())
    }
}
