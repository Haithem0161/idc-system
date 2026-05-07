# Tauri v2 Guide

Enterprise-grade patterns for building secure, performant desktop applications with Tauri v2 and Rust.

## Installation

```bash
# Install Tauri CLI
cargo install tauri-cli

# Add Tauri to an existing project
cargo tauri init

# Create a new project from scratch
cargo create-tauri-app

# Run in development mode
cargo tauri dev

# Build for production
cargo tauri build
```

### Cargo.toml Dependencies

```toml
[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-log = "2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
thiserror = "2"
log = "0.4"
```

## Basic Setup

### Project Structure

```
src-tauri/
├── capabilities/
│   └── default.json         # Default capability permissions
├── gen/
│   └── schemas/             # Auto-generated JSON schemas
├── icons/
│   ├── 32x32.png
│   ├── 128x128.png
│   ├── 128x128@2x.png
│   ├── icon.icns            # macOS icon
│   └── icon.ico             # Windows icon
├── src/
│   ├── commands/            # Tauri command handlers (organized by domain)
│   │   ├── mod.rs
│   │   ├── documents.rs
│   │   └── users.rs
│   ├── state.rs             # Managed application state
│   ├── error.rs             # Error types with Serialize impl
│   ├── lib.rs               # App library entry point (run function)
│   └── main.rs              # Binary entry point
├── build.rs                 # Tauri build script
├── Cargo.toml               # Rust dependencies
└── tauri.conf.json          # Tauri application configuration
```

### Minimal tauri.conf.json

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "My Application",
  "version": "0.1.0",
  "identifier": "com.company.myapp",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "main",
        "title": "My Application",
        "width": 1400,
        "height": 900,
        "minWidth": 900,
        "minHeight": 600,
        "center": true,
        "decorations": true,
        "theme": "Dark"
      }
    ]
  }
}
```

### Entry Points

```rust
// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
```

```rust
// src-tauri/src/lib.rs
pub mod commands;
pub mod error;
pub mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
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
        .invoke_handler(tauri::generate_handler![
            commands::documents::list_documents,
            commands::documents::get_document,
            commands::documents::create_document,
            commands::users::get_current_user,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Commands

Commands are the primary IPC mechanism between the frontend and Rust backend. Decorate functions with `#[tauri::command]` and register them with `generate_handler!`.

### Basic Commands

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
}

/// Synchronous command - blocks the main thread. Use only for fast operations.
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Command with parameters - parameter names must match frontend invoke keys.
#[tauri::command]
fn format_title(title: String, uppercase: bool) -> String {
    if uppercase {
        title.to_uppercase()
    } else {
        title
    }
}
```

### Async Commands

```rust
/// Async commands run on a separate thread and do not block the main thread.
/// Always prefer async for I/O operations, database queries, and network calls.
#[tauri::command]
async fn list_documents(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Document>, AppError> {
    let db = state.db_pool().await;
    let documents = db.query("SELECT * FROM documents ORDER BY created_at DESC")
        .fetch_all()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(documents)
}

#[tauri::command]
async fn create_document(
    title: String,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<Document, AppError> {
    let db = state.db_pool().await;
    let doc = db.query("INSERT INTO documents (title, content) VALUES ($1, $2) RETURNING *")
        .bind(&title)
        .bind(&content)
        .fetch_one()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(doc)
}
```

### Error Handling

```rust
use thiserror::Error;

/// Errors must implement Serialize to cross the IPC boundary.
/// The serialized string becomes the rejected promise message in the frontend.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Tauri requires errors to be serializable. Implement Serialize manually
/// to control the shape of the error sent to the frontend.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        let (code, message) = match self {
            AppError::NotFound(msg) => ("NOT_FOUND", msg.as_str()),
            AppError::Validation(msg) => ("VALIDATION_ERROR", msg.as_str()),
            AppError::NotAuthenticated => ("NOT_AUTHENTICATED", "Not authenticated"),
            AppError::PermissionDenied(msg) => ("PERMISSION_DENIED", msg.as_str()),
            AppError::Database(msg) => ("DATABASE_ERROR", msg.as_str()),
            AppError::Internal(msg) => ("INTERNAL_ERROR", msg.as_str()),
        };
        state.serialize_field("code", code)?;
        state.serialize_field("message", message)?;
        state.end()
    }
}

/// Type alias for cleaner command signatures.
pub type AppResult<T> = Result<T, AppError>;

/// Convert common error types automatically.
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
```

### Accessing Window and AppHandle

```rust
/// Commands can inject tauri::Window and tauri::AppHandle as parameters.
/// These are not passed from the frontend -- Tauri injects them automatically.
#[tauri::command]
async fn close_splash_screen(window: tauri::Window) -> Result<(), AppError> {
    window.close().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
async fn get_config_dir(app: tauri::AppHandle) -> Result<String, AppError> {
    let path = app.path()
        .app_config_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(path.display().to_string())
}
```

### Frontend Invocation

```typescript
import { invoke } from '@tauri-apps/api/core';

// Simple command
const version = await invoke<string>('get_app_version');

// Command with parameters -- keys must match Rust parameter names
const doc = await invoke<Document>('create_document', {
  title: 'Quarterly Report',
  content: 'Revenue grew 15% year-over-year...',
});

// Error handling -- rejected promise catches the serialized AppError
try {
  const user = await invoke<User>('get_current_user');
} catch (error) {
  // error is the serialized AppError: { code: "NOT_AUTHENTICATED", message: "..." }
  const appError = error as { code: string; message: string };
  if (appError.code === 'NOT_AUTHENTICATED') {
    redirectToLogin();
  }
}

// Async wrapper for React Query integration
export const documentKeys = {
  all: ['documents'] as const,
  detail: (id: string) => ['documents', id] as const,
};

export function useDocuments() {
  return useQuery({
    queryKey: documentKeys.all,
    queryFn: () => invoke<Document[]>('list_documents'),
  });
}
```

## Events

Events provide a pub/sub communication channel between the Rust backend and the frontend. Unlike commands (request/response), events are fire-and-forget.

### Emitting Events from Rust

```rust
use tauri::{AppHandle, Emitter};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub task_id: String,
    pub progress: f64,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct DocumentChangedPayload {
    pub document_id: String,
    pub action: String, // "created" | "updated" | "deleted"
}

/// Emit a global event to all windows.
#[tauri::command]
async fn start_export(
    task_id: String,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    // Spawn a background task that emits progress events
    let app_handle = app.clone();
    tokio::spawn(async move {
        for i in 0..=100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = app_handle.emit("export-progress", ProgressPayload {
                task_id: task_id.clone(),
                progress: i as f64 / 100.0,
                message: format!("Exporting... {}%", i),
            });
        }
        let _ = app_handle.emit("export-complete", &task_id);
    });

    Ok(())
}

/// Emit an event to a specific window by its label.
fn notify_window(app: &AppHandle, window_label: &str, payload: DocumentChangedPayload) {
    if let Some(window) = app.get_webview_window(window_label) {
        let _ = window.emit("document-changed", payload);
    }
}
```

### Listening to Events in Rust

```rust
use tauri::Listener;

/// Listen for events from the frontend during app setup.
fn setup_event_listeners(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

    // Listen for a global event
    app.listen("frontend-ready", move |event| {
        log::info!("Frontend signaled ready: {:?}", event.payload());
        // Perform post-initialization tasks
        let _ = handle.emit("backend-ready", "initialized");
    });

    Ok(())
}
```

### Listening to Events in the Frontend

```typescript
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

// Global event listener -- receives events from any source
const unlisten = await listen<ProgressPayload>('export-progress', (event) => {
  console.log(`Progress: ${event.payload.progress * 100}%`);
  console.log(`Message: ${event.payload.message}`);
});

// Clean up when no longer needed
unlisten();

// Window-specific event listener
const currentWindow = getCurrentWindow();
const unlistenWindow = await currentWindow.listen<DocumentChangedPayload>(
  'document-changed',
  (event) => {
    console.log(`Document ${event.payload.document_id} was ${event.payload.action}`);
    queryClient.invalidateQueries({ queryKey: ['documents'] });
  }
);

// React hook pattern for event listeners
import { useEffect } from 'react';

function useExportProgress(taskId: string, onProgress: (p: ProgressPayload) => void) {
  useEffect(() => {
    const unlistenPromise = listen<ProgressPayload>('export-progress', (event) => {
      if (event.payload.task_id === taskId) {
        onProgress(event.payload);
      }
    });

    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, [taskId, onProgress]);
}
```

### Emitting Events from the Frontend

```typescript
import { emit } from '@tauri-apps/api/event';

// Emit a global event that Rust can listen to
await emit('frontend-ready', { timestamp: Date.now() });

// Emit to the current window
const currentWindow = getCurrentWindow();
await currentWindow.emit('user-action', { action: 'tab-changed', tab: 'settings' });
```

## Window Management

### Creating Windows Programmatically

```rust
use tauri::WebviewWindowBuilder;

/// Create a new window from a Tauri command.
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), AppError> {
    let _settings_window = WebviewWindowBuilder::new(
        &app,
        "settings",          // Unique window label
        tauri::WebviewUrl::App("settings".into()),
    )
    .title("Settings")
    .inner_size(800.0, 600.0)
    .min_inner_size(600.0, 400.0)
    .center()
    .resizable(true)
    .decorations(true)
    .build()
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}

/// Open a document in a dedicated window.
#[tauri::command]
async fn open_document_window(
    document_id: String,
    title: String,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    let label = format!("doc-{}", document_id);

    // Check if the window already exists and focus it
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus().map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok(());
    }

    let url = format!("documents/{}", document_id);
    let _window = WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title(&title)
    .inner_size(1200.0, 800.0)
    .build()
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}
```

### Window Configuration in tauri.conf.json

```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "My Application",
        "width": 1400,
        "height": 900,
        "minWidth": 900,
        "minHeight": 600,
        "maxWidth": null,
        "maxHeight": null,
        "resizable": true,
        "fullscreen": false,
        "center": true,
        "decorations": true,
        "transparent": false,
        "alwaysOnTop": false,
        "visible": true,
        "theme": "Dark",
        "url": "/"
      }
    ]
  }
}
```

### Frontend Window API

```typescript
import { Window } from '@tauri-apps/api/window';
import { getCurrentWindow, getAllWindows } from '@tauri-apps/api/window';

// Get the current window instance
const appWindow = getCurrentWindow();

// Window operations
await appWindow.setTitle('New Title');
await appWindow.setSize(new LogicalSize(1200, 800));
await appWindow.center();
await appWindow.minimize();
await appWindow.maximize();
await appWindow.unmaximize();
await appWindow.setFullscreen(true);
await appWindow.show();
await appWindow.hide();
await appWindow.close();

// Query window state
const isMaximized = await appWindow.isMaximized();
const isFullscreen = await appWindow.isFullscreen();
const scaleFactor = await appWindow.scaleFactor();

// Create a new window from the frontend
const settingsWindow = new Window('settings', {
  url: '/settings',
  title: 'Settings',
  width: 800,
  height: 600,
  center: true,
});

settingsWindow.once('tauri://created', () => {
  console.log('Settings window created');
});

settingsWindow.once('tauri://error', (e) => {
  console.error('Failed to create settings window:', e);
});

// Listen for events on the new window
const unlisten = await settingsWindow.listen('settings-updated', (event) => {
  console.log('Settings updated:', event.payload);
});

// Get all open windows
const windows = await getAllWindows();
for (const win of windows) {
  console.log(`Window: ${win.label}`);
}
```

## Plugins

### Using Official Plugins

```toml
# Cargo.toml
[dependencies]
tauri-plugin-log = "2"
tauri-plugin-store = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
tauri-plugin-shell = "2"
tauri-plugin-process = "2"
tauri-plugin-notification = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-http = "2"
tauri-plugin-opener = "2"
```

```bash
# Frontend package for plugin APIs
pnpm add @tauri-apps/plugin-log
pnpm add @tauri-apps/plugin-store
pnpm add @tauri-apps/plugin-dialog
pnpm add @tauri-apps/plugin-fs
```

### Plugin Registration

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![/* commands */])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### tauri-plugin-log

```rust
// Registration with custom configuration
tauri::Builder::default()
    .plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .with_colors(tauri_plugin_log::fern::colors::ColoredLevelConfig::default())
            .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
            .max_file_size(5_000_000) // 5MB per log file
            .build(),
    )
```

```rust
// Use the standard `log` crate macros throughout Rust code
use log::{info, warn, error, debug, trace};

#[tauri::command]
async fn process_batch(items: Vec<String>) -> Result<usize, AppError> {
    info!("Processing batch of {} items", items.len());

    for (i, item) in items.iter().enumerate() {
        debug!("Processing item {}: {}", i, item);
        if item.is_empty() {
            warn!("Skipping empty item at index {}", i);
            continue;
        }
    }

    info!("Batch processing complete");
    Ok(items.len())
}
```

### Building a Custom Plugin

```rust
// src-tauri/src/plugins/analytics.rs
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Default)]
struct AnalyticsState {
    events: Mutex<Vec<AnalyticsEvent>>,
}

#[derive(Clone, Serialize)]
struct AnalyticsEvent {
    name: String,
    timestamp: i64,
    properties: serde_json::Value,
}

#[tauri::command]
async fn track_event(
    name: String,
    properties: serde_json::Value,
    state: tauri::State<'_, AnalyticsState>,
) -> Result<(), String> {
    let event = AnalyticsEvent {
        name,
        timestamp: chrono::Utc::now().timestamp(),
        properties,
    };

    state.events
        .lock()
        .map_err(|e| e.to_string())?
        .push(event);

    Ok(())
}

#[tauri::command]
async fn flush_events(
    state: tauri::State<'_, AnalyticsState>,
) -> Result<usize, String> {
    let mut events = state.events
        .lock()
        .map_err(|e| e.to_string())?;

    let count = events.len();
    // Send events to analytics backend...
    events.clear();
    Ok(count)
}

/// Build the plugin with its commands and managed state.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("analytics")
        .invoke_handler(tauri::generate_handler![track_event, flush_events])
        .setup(|app, _api| {
            app.manage(AnalyticsState::default());
            log::info!("Analytics plugin initialized");
            Ok(())
        })
        .on_drop(|_app| {
            log::info!("Analytics plugin shutting down -- flushing remaining events");
        })
        .build()
}
```

```rust
// Register in lib.rs
pub fn run() {
    tauri::Builder::default()
        .plugin(plugins::analytics::init())
        // ...
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Capabilities & Permissions

Capabilities define what permissions are granted to each window. This is the core of Tauri v2's security model.

### Default Capability File

```json
// src-tauri/capabilities/default.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "log:default",
    "store:default",
    "dialog:default",
    "notification:default",
    "clipboard-manager:default"
  ]
}
```

### Fine-Grained Permissions

```json
// src-tauri/capabilities/main-window.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main-window",
  "description": "Full permissions for the main application window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-set-title",
    "core:window:allow-maximize",
    "core:window:allow-minimize",
    "core:window:allow-start-dragging",
    "fs:allow-read-text-file",
    "fs:allow-write-text-file",
    "fs:allow-resource-read-recursive",
    "dialog:allow-open",
    "dialog:allow-save",
    "dialog:allow-message",
    "shell:allow-open",
    "http:default",
    "notification:allow-notify",
    "notification:allow-request-permission",
    "clipboard-manager:allow-read",
    "clipboard-manager:allow-write"
  ]
}
```

### Scoped Permissions for Secondary Windows

```json
// src-tauri/capabilities/settings-window.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "settings-window",
  "description": "Limited permissions for the settings window",
  "windows": ["settings"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "store:allow-get",
    "store:allow-set",
    "store:allow-save"
  ]
}
```

### Inline Capabilities in tauri.conf.json

```json
{
  "app": {
    "security": {
      "capabilities": [
        "main-window",
        {
          "identifier": "drag-window",
          "permissions": ["core:window:allow-start-dragging"]
        }
      ]
    }
  }
}
```

### Content Security Policy (CSP)

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' data: https://fonts.gstatic.com; img-src 'self' data: https: blob:; connect-src 'self' http://localhost:* ws://localhost:* https://api.company.com"
    }
  }
}
```

Key CSP directives:
- `default-src 'self'` -- Only load resources from the app origin by default
- `script-src 'self' 'wasm-unsafe-eval'` -- Allow scripts from origin and WebAssembly evaluation
- `connect-src` -- Whitelist specific API domains for fetch/XHR/WebSocket
- `style-src 'self' 'unsafe-inline'` -- Allow inline styles (needed for most CSS-in-JS and Tailwind)
- `img-src 'self' data: https: blob:` -- Allow images from origin, data URIs, HTTPS, and blob URIs

Tauri automatically injects nonce and hash sources at compile time for additional script/style protection. Disable this only when absolutely necessary:

```json
{
  "app": {
    "security": {
      "dangerousDisableAssetCspModification": false
    }
  }
}
```

## State Management

### Thread-Safe Managed State

```rust
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Application state managed by Tauri. Injected into commands via tauri::State.
/// Must be Send + Sync since commands can run on any thread.
pub struct AppState {
    /// Use Mutex for synchronous access with low contention.
    config: Mutex<AppConfig>,
    /// Use RwLock for async access with many readers and few writers.
    session: RwLock<Option<Session>>,
    /// Use RwLock for connection pools and shared resources.
    db_pool: RwLock<Option<DatabasePool>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_base_url: String,
    pub theme: String,
    pub language: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub expires_at: i64,
}

pub struct DatabasePool {
    // Your database pool type (e.g., sqlx::PgPool)
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
            session: RwLock::new(None),
            db_pool: RwLock::new(None),
        }
    }

    /// Synchronous access pattern -- use Mutex for quick reads/writes.
    pub fn get_config(&self) -> AppConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn update_config<F>(&self, f: F)
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut config = self.config.lock().unwrap();
        f(&mut config);
    }

    /// Async access pattern -- use RwLock for I/O-bound operations.
    pub async fn set_session(&self, session: Session) {
        *self.session.write().await = Some(session);
    }

    pub async fn get_session(&self) -> Option<Session> {
        self.session.read().await.clone()
    }

    pub async fn clear_session(&self) {
        *self.session.write().await = None;
    }

    pub async fn is_authenticated(&self) -> bool {
        let session = self.session.read().await;
        match &*session {
            Some(s) => s.expires_at > chrono::Utc::now().timestamp(),
            None => false,
        }
    }
}
```

### Using State in Commands

```rust
/// Tauri injects State automatically -- no need to pass it from the frontend.
#[tauri::command]
async fn login(
    username: String,
    password: String,
    state: tauri::State<'_, AppState>,
) -> Result<Session, AppError> {
    let config = state.get_config();
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/auth/login", config.api_base_url))
        .json(&serde_json::json!({
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let session: Session = response
        .json()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    state.set_session(session.clone()).await;
    Ok(session)
}

#[tauri::command]
async fn logout(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    state.clear_session().await;
    Ok(())
}

#[tauri::command]
async fn get_current_session(
    state: tauri::State<'_, AppState>,
) -> Result<Option<Session>, AppError> {
    Ok(state.get_session().await)
}
```

### Registering State

```rust
pub fn run() {
    let config = AppConfig {
        api_base_url: "https://api.company.com".to_string(),
        theme: "dark".to_string(),
        language: "en".to_string(),
    };

    tauri::Builder::default()
        .manage(AppState::new(config))
        .invoke_handler(tauri::generate_handler![
            login,
            logout,
            get_current_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Configuration

### Full tauri.conf.json Reference

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Torch Document Center",
  "version": "1.0.0",
  "identifier": "com.torch.document-center",

  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },

  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "main",
        "title": "Torch Document Center",
        "width": 1400,
        "height": 900,
        "minWidth": 900,
        "minHeight": 600,
        "resizable": true,
        "fullscreen": false,
        "center": true,
        "decorations": true,
        "transparent": false,
        "alwaysOnTop": false,
        "visible": true,
        "theme": "Dark"
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; font-src 'self' data: https://cdn.jsdelivr.net; img-src 'self' data: https: blob:; connect-src 'self' http://localhost:* ws://localhost:* https://api.torch.com",
      "dangerousDisableAssetCspModification": false,
      "freezePrototype": false
    }
  },

  "bundle": {
    "active": true,
    "targets": "all",
    "resources": {
      "../dist/": "frontend/dist/"
    },
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "category": "Productivity",
    "shortDescription": "Torch Document Center",
    "longDescription": "Enterprise document management for Torch platform",
    "copyright": "Copyright (c) Torch Corp",
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"]
      },
      "appimage": {
        "bundleMediaFramework": true
      }
    },
    "macOS": {
      "minimumSystemVersion": "10.15",
      "frameworks": [],
      "signingIdentity": null
    },
    "windows": {
      "nsis": {
        "installMode": "perMachine",
        "languages": ["English"]
      },
      "wix": null
    }
  }
}
```

### Configuration Sections Explained

**build** -- Controls how the frontend is served and built:
- `frontendDist`: Path to the built frontend assets (relative to src-tauri)
- `devUrl`: URL where the dev server runs
- `beforeDevCommand`: Shell command executed before `tauri dev`
- `beforeBuildCommand`: Shell command executed before `tauri build`

**app.windows** -- Default window configuration:
- `label`: Unique identifier for the window (used in capabilities and window management)
- `theme`: `"Light"`, `"Dark"`, or `null` (follows system)
- `decorations`: Whether to show the OS title bar and window frame
- `transparent`: Enable window transparency (requires `decorations: false` for custom title bars)

**app.security** -- Security policies for the WebView:
- `csp`: Content Security Policy injected into all HTML
- `capabilities`: Fine-grained control over which capability files are loaded
- `freezePrototype`: Freeze `Object.prototype` for enhanced security

**bundle** -- Production build configuration:
- `targets`: `"all"`, `"deb"`, `"appimage"`, `"nsis"`, `"msi"`, `"dmg"`, `"app"`
- `resources`: Additional files to include in the bundle
- `icon`: Icon files for each platform

## Building & Bundling

### Development Build

```bash
# Start development with hot-reload
cargo tauri dev

# Development with specific features enabled
cargo tauri dev --features "debug-logging"

# Development targeting a specific frontend
cargo tauri dev --config '{"build":{"devUrl":"http://localhost:3000"}}'
```

### Production Build

```bash
# Build for the current platform (all targets)
cargo tauri build

# Build specific target
cargo tauri build --target x86_64-unknown-linux-gnu

# Build only .deb package (Linux)
cargo tauri build --bundles deb

# Build only .dmg (macOS)
cargo tauri build --bundles dmg

# Build only NSIS installer (Windows)
cargo tauri build --bundles nsis

# Build with verbose output for troubleshooting
cargo tauri build --verbose

# Build in debug mode (faster, but larger binary)
cargo tauri build --debug
```

### Generating Icons

```bash
# Generate all icon sizes from a single 1024x1024+ PNG source
cargo tauri icon path/to/app-icon.png
```

This generates all required icon formats in `src-tauri/icons/`:
- `32x32.png`, `128x128.png`, `128x128@2x.png` (Linux/generic)
- `icon.icns` (macOS)
- `icon.ico` (Windows)

### Bundling Resources

```json
{
  "bundle": {
    "resources": {
      "../migrations/": "migrations/",
      "../config/defaults.toml": "config/defaults.toml",
      "../dist/": "frontend/dist/"
    }
  }
}
```

```rust
/// Access bundled resources at runtime using the resource resolver.
#[tauri::command]
async fn read_default_config(app: tauri::AppHandle) -> Result<String, AppError> {
    let resource_path = app.path()
        .resolve("config/defaults.toml", tauri::path::BaseDirectory::Resource)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    std::fs::read_to_string(resource_path)
        .map_err(|e| AppError::Internal(e.to_string()))
}
```

### Platform-Specific Build Configuration

```rust
// build.rs
fn main() {
    // Tauri build step -- required
    tauri_build::build();

    // Platform-specific build logic
    #[cfg(target_os = "windows")]
    {
        // Embed Windows application manifest
        println!("cargo:rerun-if-changed=windows-manifest.xml");
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Security");
    }
}
```

```rust
// Conditional compilation for platform-specific commands
#[tauri::command]
async fn get_system_info() -> Result<SystemInfo, AppError> {
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),

        #[cfg(target_os = "linux")]
        desktop_environment: std::env::var("XDG_CURRENT_DESKTOP").ok(),

        #[cfg(not(target_os = "linux"))]
        desktop_environment: None,
    })
}
```

## File Organization

### Recommended Directory Structure

```
src-tauri/
├── capabilities/
│   ├── default.json              # Base permissions for all windows
│   ├── main-window.json          # Extended permissions for main window
│   └── settings-window.json      # Scoped permissions for settings
├── icons/
│   ├── 32x32.png
│   ├── 128x128.png
│   ├── 128x128@2x.png
│   ├── icon.icns
│   └── icon.ico
├── src/
│   ├── commands/
│   │   ├── mod.rs                # Re-exports all command modules
│   │   ├── auth.rs               # Authentication commands (login, logout, refresh)
│   │   ├── documents.rs          # Document CRUD commands
│   │   ├── settings.rs           # Application settings commands
│   │   └── system.rs             # System info, health check commands
│   ├── plugins/
│   │   ├── mod.rs                # Re-exports custom plugins
│   │   └── analytics.rs          # Custom analytics plugin
│   ├── models/
│   │   ├── mod.rs                # Re-exports model types
│   │   ├── document.rs           # Document structs and enums
│   │   └── user.rs               # User structs and enums
│   ├── services/
│   │   ├── mod.rs                # Re-exports service modules
│   │   ├── api_client.rs         # HTTP client for external APIs
│   │   └── storage.rs            # Local file storage operations
│   ├── state.rs                  # Managed application state (AppState)
│   ├── error.rs                  # AppError enum with Serialize impl
│   ├── lib.rs                    # App entry: Builder setup, plugin/command registration
│   └── main.rs                   # Binary entry point (calls lib::run)
├── build.rs                      # Tauri build script
├── Cargo.toml                    # Rust dependencies
├── Cargo.lock                    # Locked dependency versions
└── tauri.conf.json               # Tauri application configuration
```

### Module Re-export Pattern

```rust
// src-tauri/src/commands/mod.rs
pub mod auth;
pub mod documents;
pub mod settings;
pub mod system;
```

```rust
// src-tauri/src/lib.rs
pub mod commands;
pub mod error;
pub mod models;
pub mod plugins;
pub mod services;
pub mod state;

pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::new(/* ... */))
        .plugin(plugins::analytics::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            // Auth
            commands::auth::login,
            commands::auth::logout,
            commands::auth::refresh_token,
            // Documents
            commands::documents::list_documents,
            commands::documents::get_document,
            commands::documents::create_document,
            commands::documents::update_document,
            commands::documents::delete_document,
            // Settings
            commands::settings::get_settings,
            commands::settings::update_settings,
            // System
            commands::system::health_check,
            commands::system::get_system_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Best Practices

- **Always use async commands** for anything involving I/O, network, or database access. Synchronous commands block the main thread and freeze the UI. Reserve sync commands only for trivial computations that return instantly.

- **Implement structured error types** with `thiserror` and a manual `Serialize` implementation. Return error codes and messages as structured JSON rather than plain strings so the frontend can branch on error types programmatically.

- **Apply the principle of least privilege** to capabilities. Each window should only have the permissions it actually needs. Never grant `fs:default` or `shell:default` to windows that do not require file system or shell access.

- **Keep commands thin** -- commands should validate input, delegate to service modules, and return results. Business logic belongs in `services/` modules that are testable without the Tauri runtime.

- **Use `tauri::State` for shared application state** rather than global statics. Wrap mutable data in `Mutex` (for synchronous access) or `tokio::sync::RwLock` (for async access). Prefer `RwLock` when reads significantly outnumber writes.

- **Clean up event listeners** in the frontend. Every `listen()` call returns an unlisten function. Call it when the React component unmounts or the listener is no longer needed. Failing to unlisten causes memory leaks and duplicate handler invocations.

- **Set a restrictive CSP** from day one. Whitelist only the specific domains your application needs. Never use `'unsafe-eval'` in production unless WebAssembly requires `'wasm-unsafe-eval'`. Tauri's automatic nonce/hash injection provides defense-in-depth.

- **Organize commands by domain** in separate modules (e.g., `commands/auth.rs`, `commands/documents.rs`). Register them all in `lib.rs` via `generate_handler!`. This keeps the codebase navigable as the command surface grows.

- **Use `#[cfg(debug_assertions)]`** to gate development-only features like verbose logging, dev tools plugins, and debug commands. This ensures they are stripped from production builds automatically.

- **Generate icons from a single high-resolution source** using `cargo tauri icon`. Maintain one 1024x1024 or larger PNG as the source of truth and regenerate platform-specific formats during the build pipeline.
