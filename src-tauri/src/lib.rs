//! Torch App Template - Tauri Application Library
//!
//! Supports two modes:
//! - **Standalone**: Normal Tauri window app
//! - **Embedded**: Headless mode for Business OS integration

pub mod config;
pub mod db;
pub mod domains;
pub mod embedded;
pub mod error;
pub mod state;
pub mod sync;

/// Initialize the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Check for embedded mode BEFORE building Tauri
    // In embedded mode, we run headless with HTTP server + IPC to Business OS
    if embedded::is_embedded_mode() {
        eprintln!("[STARTUP] Embedded mode detected (TORCH_EMBEDDED_MODE=true)");

        match embedded::EmbeddedConfig::from_env() {
            Ok(embedded_config) => {
                // Create a new tokio runtime for embedded mode
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime for embedded mode");

                if let Err(e) = rt.block_on(embedded::run_embedded(embedded_config)) {
                    eprintln!("[ERROR] Embedded mode failed: {}", e);
                    std::process::exit(1);
                }
                return; // Exit after embedded mode completes
            }
            Err(e) => {
                eprintln!("[ERROR] Invalid embedded mode configuration: {}", e);
                eprintln!("[ERROR] Required env vars: TORCH_IPC_PORT, TORCH_RUN_ID");
                std::process::exit(1);
            }
        }
    }

    // Standalone mode: normal Tauri application with window
    eprintln!("[STARTUP] Running in standalone mode");

    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
