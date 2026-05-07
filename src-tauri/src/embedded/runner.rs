//! Embedded mode runner.
//!
//! Coordinates HTTP server and IPC client for Business OS integration.

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::error::AppResult;
use crate::state::AppState;

use super::http_server::start_http_server;
use super::ipc_client::IpcClient;
use super::EmbeddedConfig;

/// Run the application in embedded mode.
///
/// This function orchestrates:
/// 1. AppState creation
/// 2. HTTP server for frontend assets + auth endpoint
/// 3. Business OS IPC client connection and message loop
pub async fn run_embedded(embedded_config: EmbeddedConfig) -> AppResult<()> {
    tracing::info!("Starting app in embedded mode");
    tracing::info!("  Run ID: {}", embedded_config.run_id);
    tracing::info!("  Business OS IPC Port: {}", embedded_config.ipc_port);

    // Create shutdown broadcast channel for coordinated shutdown
    let (shutdown_tx, _) = broadcast::channel::<()>(16);

    // Create app state
    let app_state = Arc::new(AppState::new());

    // Start HTTP server for frontend assets + auth endpoint
    let http_port = start_http_server(app_state.clone(), shutdown_tx.subscribe()).await?;
    tracing::info!("Frontend available at http://127.0.0.1:{}", http_port);

    // Connect to Business OS IPC
    let mut ipc_client = IpcClient::connect(
        embedded_config.ipc_port,
        app_state.clone(),
        shutdown_tx.clone(),
    )
    .await?;

    // Perform handshake with Business OS
    ipc_client
        .handshake(&embedded_config.run_id, http_port)
        .await?;

    // Handle SIGTERM/SIGINT for graceful shutdown
    let shutdown_signal_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT (Ctrl+C), initiating shutdown");
            }
            _ = async {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                        sigterm.recv().await;
                    }
                }
                #[cfg(not(unix))]
                {
                    // On Windows, just wait forever (Ctrl+C will handle shutdown)
                    std::future::pending::<()>().await;
                }
            } => {
                tracing::info!("Received SIGTERM, initiating shutdown");
            }
        }
        let _ = shutdown_signal_tx.send(());
    });

    // Run IPC message loop (blocks until shutdown)
    ipc_client.run_message_loop().await?;

    tracing::info!("Embedded mode shutdown complete");
    Ok(())
}
