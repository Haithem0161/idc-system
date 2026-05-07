//! HTTP server for serving frontend assets and auth status in embedded mode.
//!
//! In embedded mode, the Tauri IPC bridge is unavailable. This server provides:
//! 1. Static file serving for the Vite `dist/` build
//! 2. `/api/auth` endpoint for frontend to poll auth status

use axum::extract::State as AxumState;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

// =============================================================================
// Shared State
// =============================================================================

/// Shared application state for Axum handlers.
#[derive(Clone)]
pub struct HttpState {
    pub app_state: Arc<AppState>,
}

// =============================================================================
// Auth Route
// =============================================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    authenticated: bool,
    token: Option<String>,
    expires_at: Option<i64>,
    user: Option<AuthUserInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthUserInfo {
    user_id: String,
    entity_id: String,
    email: String,
    name: Option<String>,
    entity_role: String,
    permissions: Vec<String>,
}

async fn get_auth(AxumState(state): AxumState<HttpState>) -> Json<AuthResponse> {
    let user = state.app_state.get_current_user().await;
    let token = state.app_state.get_current_token().await;
    let expires_at = state.app_state.get_token_expires_at().await;

    let response = match user {
        Some(ctx) => AuthResponse {
            authenticated: true,
            token,
            expires_at,
            user: Some(AuthUserInfo {
                user_id: ctx.user_id.clone(),
                entity_id: ctx.entity_id.clone(),
                email: ctx.email.clone(),
                name: ctx.name.clone(),
                entity_role: ctx.role.clone(),
                permissions: Vec::new(),
            }),
        },
        None => AuthResponse {
            authenticated: false,
            token: None,
            expires_at: None,
            user: None,
        },
    };

    Json(response)
}

// =============================================================================
// Frontend Discovery
// =============================================================================

/// Find the frontend build directory (Vite `dist/`).
fn find_frontend_dir() -> AppResult<PathBuf> {
    // 1. Explicit frontend path override (highest priority)
    if let Ok(path) = std::env::var("TORCH_FRONTEND_PATH") {
        let p = PathBuf::from(&path);
        if p.join("index.html").exists() {
            tracing::info!("Using frontend path from TORCH_FRONTEND_PATH: {}", path);
            return Ok(p);
        }
        tracing::warn!(
            "TORCH_FRONTEND_PATH set to {} but index.html not found",
            path
        );
    }

    // 2. TORCH_INSTALL_PATH from Business OS (embedded mode)
    if let Ok(install_path) = std::env::var("TORCH_INSTALL_PATH") {
        let base = PathBuf::from(&install_path);
        let candidates = [
            base.join("usr/lib/Torch App Template/frontend/dist"),
            base.join("frontend/dist"),
            base.join("dist"),
        ];
        for candidate in &candidates {
            if candidate.join("index.html").exists() {
                tracing::info!(
                    "Found frontend at TORCH_INSTALL_PATH: {}",
                    candidate.display()
                );
                return Ok(candidate.clone());
            }
        }
        tracing::warn!(
            "TORCH_INSTALL_PATH set to {} but frontend build not found",
            install_path
        );
    }

    // 3. Next to executable (including Tauri .deb resource paths)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidates = vec![
                exe_dir.join("frontend/dist"),
                exe_dir.join("dist"),
                exe_dir.join("../frontend/dist"),
                exe_dir.join("../../frontend/dist"),
                exe_dir.join("../lib/Torch App Template/frontend/dist"),
                exe_dir.join("../lib/Torch App Template/dist"),
            ];

            for candidate in candidates {
                if candidate.join("index.html").exists() {
                    tracing::info!(
                        "Found frontend at: {}",
                        candidate
                            .canonicalize()
                            .unwrap_or(candidate.clone())
                            .display()
                    );
                    return Ok(candidate);
                }
            }
        }
    }

    // 4. Development path (relative to src-tauri)
    let dev_path = PathBuf::from("../dist");
    if dev_path.join("index.html").exists() {
        tracing::info!("Using development frontend path: ../dist");
        return Ok(dev_path);
    }

    // 5. Try current working directory
    let cwd_path = PathBuf::from("dist");
    if cwd_path.join("index.html").exists() {
        tracing::info!("Using frontend path from CWD: dist");
        return Ok(cwd_path);
    }

    Err(AppError::Configuration(
        "Frontend build directory not found. Set TORCH_FRONTEND_PATH, TORCH_INSTALL_PATH, or ensure dist/ exists with index.html.".to_string(),
    ))
}

// =============================================================================
// Server Startup
// =============================================================================

/// Start the HTTP server and return the port it's listening on.
pub async fn start_http_server(
    app_state: Arc<AppState>,
    shutdown_rx: broadcast::Receiver<()>,
) -> AppResult<u16> {
    let frontend_dir = find_frontend_dir()?;
    tracing::info!("Serving frontend from: {}", frontend_dir.display());

    // Find an available port by binding to port 0
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Internal(format!("Failed to bind HTTP server: {}", e)))?;
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Release so tokio can rebind

    let http_state = HttpState { app_state };

    // Build router: API routes first, static files as fallback
    let app = Router::new()
        .route("/api/auth", get(get_auth))
        .with_state(http_state)
        .fallback_service(
            ServeDir::new(&frontend_dir).append_index_html_on_directories(true),
        );

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to bind HTTP server: {}", e)))?;

    tracing::info!("HTTP server listening on http://{}", addr);

    // Spawn server with graceful shutdown
    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
                tracing::info!("HTTP server shutting down");
            })
            .await
        {
            tracing::error!("HTTP server error: {}", e);
        }
    });

    Ok(port)
}
