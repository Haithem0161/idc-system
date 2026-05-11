//! Embedded mode support for running within Business OS.
//!
//! When TORCH_EMBEDDED_MODE=true, the app runs headless:
//! - No Tauri window is created
//! - Frontend is served via HTTP on a dynamic port
//! - Communicates with Business OS via MessagePack IPC

pub mod http_server;
pub mod ipc_client;
pub mod messages;
pub mod runner;

pub use runner::run_embedded;

/// Check if the app should run in embedded mode.
pub fn is_embedded_mode() -> bool {
    std::env::var("TORCH_EMBEDDED_MODE")
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Configuration for embedded mode, parsed from environment variables.
#[derive(Debug, Clone)]
pub struct EmbeddedConfig {
    pub ipc_port: u16,
    pub run_id: String,
    /// The installation path provided by Business OS (optional, for locating assets).
    pub install_path: Option<String>,
}

impl EmbeddedConfig {
    /// Parse embedded mode configuration from environment.
    pub fn from_env() -> Result<Self, String> {
        let ipc_port = std::env::var("TORCH_IPC_PORT")
            .map_err(|_| "TORCH_IPC_PORT not set")?
            .parse()
            .map_err(|_| "TORCH_IPC_PORT is not a valid port number")?;

        let run_id = std::env::var("TORCH_RUN_ID").map_err(|_| "TORCH_RUN_ID not set")?;

        let install_path = std::env::var("TORCH_INSTALL_PATH").ok();

        Ok(Self {
            ipc_port,
            run_id,
            install_path,
        })
    }
}
