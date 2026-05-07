# Business OS Integration Guide

Complete reference for integrating Tauri apps with the Torch Business OS ecosystem.

## Overview

Torch Business OS apps support two execution modes:

- **Standalone**: Normal Tauri desktop app with its own window
- **Embedded**: Headless mode running inside Business OS as a child process

In embedded mode, Business OS manages authentication, entity context, and lifecycle. The app communicates via a MessagePack-over-TCP IPC protocol.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Business OS (Parent)                   │
│                                                          │
│  ┌─────────┐    ┌──────────────┐    ┌────────────────┐  │
│  │ Auth    │    │ App Launcher │    │ IPC Server     │  │
│  │ Service │    │              │    │ (TCP listener) │  │
│  └─────────┘    └──────────────┘    └────────────────┘  │
│                        │                    ▲            │
│                        │ spawns             │ TCP        │
│                        ▼                    │            │
│  ┌──────────────────────────────────────────┤           │
│  │           Child App (Your App)            │           │
│  │                                           │           │
│  │  ┌──────────┐  ┌────────────┐  ┌────────┤           │
│  │  │ Axum HTTP│  │  AppState  │  │ IPC    │           │
│  │  │ Server   │  │ (RwLock)   │  │ Client │───────────┘
│  │  │          │  │            │  │        │
│  │  │ /api/auth│◀─│ token      │◀─│ recv() │
│  │  │ static/  │  │ user       │  │ send() │
│  │  └────▲─────┘  └────────────┘  └────────┘
│  │       │
│  │  ┌────┴─────────────────────────────────┐
│  │  │     Frontend (iframe in Business OS)  │
│  │  │     polls /api/auth for token         │
│  │  │     makes API calls with Bearer token │
│  │  └──────────────────────────────────────┘
│  └───────────────────────────────────────────┘
└──────────────────────────────────────────────────────────┘
```

## Embedded Mode Detection

### Environment Variables

Business OS sets these environment variables when spawning a child app:

| Variable | Required | Description |
|----------|----------|-------------|
| `TORCH_EMBEDDED_MODE` | Yes | Set to `"true"` to enable embedded mode |
| `TORCH_IPC_PORT` | Yes | TCP port for IPC connection to Business OS |
| `TORCH_RUN_ID` | Yes | UUID identifying this app instance |
| `TORCH_INSTALL_PATH` | No | App installation directory (for locating assets) |
| `TORCH_FRONTEND_PATH` | No | Explicit path to frontend build directory |

### Rust Detection

```rust
pub fn is_embedded_mode() -> bool {
    std::env::var("TORCH_EMBEDDED_MODE")
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub struct EmbeddedConfig {
    pub ipc_port: u16,
    pub run_id: String,
    pub install_path: Option<String>,
}

impl EmbeddedConfig {
    pub fn from_env() -> Result<Self, String> {
        let ipc_port = std::env::var("TORCH_IPC_PORT")
            .map_err(|_| "TORCH_IPC_PORT not set")?
            .parse()
            .map_err(|_| "TORCH_IPC_PORT is not a valid port number")?;

        let run_id = std::env::var("TORCH_RUN_ID")
            .map_err(|_| "TORCH_RUN_ID not set")?;

        let install_path = std::env::var("TORCH_INSTALL_PATH").ok();

        Ok(Self { ipc_port, run_id, install_path })
    }
}
```

### Frontend Detection

Embedded mode detection uses a two-step approach:

1. **Heuristic check**: `isEmbeddedMode()` checks if `__TAURI_INTERNALS__` is absent (no Tauri IPC bridge)
2. **Probe confirmation**: `AuthProvider` probes `/api/auth` to confirm the endpoint exists. If the probe fails (e.g., running in a regular browser), it falls back to standalone mode.

```typescript
// Heuristic: no Tauri IPC bridge means we might be in embedded mode
export function isEmbeddedMode(): boolean {
  return !("__TAURI_INTERNALS__" in window);
}

// Probe: returns null if /api/auth is unreachable (not actually embedded)
export async function fetchEmbeddedAuth(): Promise<EmbeddedAuthResponse | null> {
  try {
    const res = await fetch("/api/auth");
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}
```

This two-step approach ensures the app works correctly in all scenarios:
- **Tauri standalone**: `isEmbeddedMode()` returns `false` → standalone mode
- **Business OS embedded**: `isEmbeddedMode()` returns `true` + probe succeeds → embedded mode
- **Regular browser (dev)**: `isEmbeddedMode()` returns `true` but probe fails → falls back to standalone

## Dual-Mode Application Entry

```rust
pub fn run() {
    // Initialize tracing first (works in both modes)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if embedded::is_embedded_mode() {
        // Embedded: headless HTTP server + IPC client
        match embedded::EmbeddedConfig::from_env() {
            Ok(config) => {
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime");
                if let Err(e) = rt.block_on(embedded::run_embedded(config)) {
                    eprintln!("[ERROR] Embedded mode failed: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("[ERROR] Invalid config: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Standalone: normal Tauri window
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
}
```

## IPC Protocol

### Wire Format

All messages use a 4-byte big-endian length prefix followed by a MessagePack-encoded body:

```
┌──────────────────┬──────────────────────────────────┐
│  Length (4 bytes) │     MessagePack Payload          │
│  big-endian u32   │     IpcEnvelope { version, ... } │
└──────────────────┴──────────────────────────────────┘
```

Maximum message size: **1 MB** (enforced to prevent memory exhaustion).

### Message Envelope

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope {
    pub version: u8,          // Protocol version (currently 1)
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcPayload {
    // See message types below
}
```

### Outbound Messages (App → Business OS)

| Message | Fields | When Sent |
|---------|--------|-----------|
| `Connect` | `run_id: String` | First message after TCP connect |
| `EmbeddedReady` | `port: u16, base_path: String` | After HTTP server starts |
| `Pong` | `timestamp: i64` | Response to Ping |
| `NavigationChanged` | `path: String, title: String` | Frontend route change |
| `TokenRefreshRequest` | (empty) | When token nears expiry |

### Inbound Messages (Business OS → App)

| Message | Fields | Action |
|---------|--------|--------|
| `ConnectAck` | `success: bool, error: Option<String>` | Confirms connection |
| `EmbeddedAck` | `success: bool, webview_label: Option<String>, error: Option<String>` | Confirms embed setup |
| `Ping` | `timestamp: i64` | Reply with Pong |
| `Shutdown` | `reason: String` | Initiate graceful shutdown |
| `AuthToken` | `token: String, expires_at: i64` | Store auth token |
| `EntityContext` | `entity_id: String, entity_name: String, role: Option<String>` | Set user/entity info |
| `TokenRefreshResponse` | `token: Option<String>, expires_at: Option<i64>, error: Option<String>` | Updated token |
| `NavigateTo` | `path: String` | Business OS requests navigation |
| `Error` | `code: String, message: String` | Error notification (e.g., SESSION_EXPIRED) |

## Connection Lifecycle

### Startup Sequence

```
1. App spawned by Business OS with env vars set
2. App detects embedded mode, creates tokio runtime
3. App starts Axum HTTP server on dynamic port (127.0.0.1:0)
4. App connects to Business OS IPC (127.0.0.1:{TORCH_IPC_PORT})
5. App sends Connect { run_id }
6. Business OS replies ConnectAck { success: true }
7. App sends EmbeddedReady { port: <http_port>, base_path: "/" }
8. Business OS creates iframe pointing to http://127.0.0.1:<http_port>
9. Business OS sends AuthToken { token, expires_at }
10. Business OS sends EntityContext { entity_id, entity_name, role }
11. Frontend loads in iframe, polls /api/auth
12. Frontend receives token, starts making API calls
```

### Message Loop

```rust
pub async fn run_message_loop(mut self) -> AppResult<()> {
    let mut shutdown_rx = self.shutdown_tx.subscribe();

    loop {
        tokio::select! {
            result = self.receive() => {
                match result {
                    Ok(msg) => self.handle_message(msg).await?,
                    Err(e) => {
                        tracing::error!("IPC read error: {}", e);
                        let _ = self.shutdown_tx.send(());
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                tracing::info!("Shutdown signal received");
                break;
            }
        }
    }
    Ok(())
}
```

### Shutdown Triggers

1. Business OS sends `Shutdown { reason }` via IPC
2. User presses Ctrl+C (SIGINT)
3. Process receives SIGTERM
4. IPC connection lost (read error)

All triggers broadcast via `tokio::sync::broadcast` channel → HTTP server and IPC client shut down gracefully.

## Auth Flow

### Token Storage (Rust Side)

```rust
pub struct AppState {
    token: RwLock<Option<String>>,
    expires_at: RwLock<Option<i64>>,
    user: RwLock<Option<UserContext>>,
}
```

- Token received via IPC `AuthToken` message → stored in `AppState`
- Token refreshed proactively by Business OS every ~60 seconds
- On `SESSION_EXPIRED` error → `clear_auth()` → frontend loses authentication

### Auth HTTP Endpoint

```rust
// GET /api/auth → JSON response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    authenticated: bool,
    token: Option<String>,
    expires_at: Option<i64>,
    user: Option<AuthUserInfo>,
}
```

### Frontend Auth Polling

The `AuthProvider` handles embedded auth through three phases:

```typescript
// Phase 1: Probe /api/auth to confirm embedded mode
const probe = await fetchEmbeddedAuth();
if (probe === null) {
  // Not reachable — fall back to standalone mode
}

// Phase 2: Wait for initial auth (Business OS sends token via IPC)
export async function waitForEmbeddedAuth(
  intervalMs = 1000,
  timeoutMs = 60000,
): Promise<EmbeddedAuthResponse | null> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const status = await fetchEmbeddedAuth();
    if (status?.authenticated && status.user) return status;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return null;
}

// Phase 3: Ongoing token refresh polling (30s interval)
// Handles token updates and session expiry recovery
```

**Auth phases in AuthProvider:**

| Phase | Description |
|-------|-------------|
| `detecting` | Initial state, probing `/api/auth` |
| `waiting_embedded` | Probe succeeded, polling for auth token from Business OS |
| `authenticated` | Auth token received, app fully functional |
| `standalone` | Not in embedded mode (or probe failed), using local auth |
| `error` | Embedded auth timed out after 60s |

### Token Flow

```
Business OS ──AuthToken──→ Rust AppState ──/api/auth──→ Frontend
                                                          │
                                                    localStorage
                                                          │
                                                    Axios interceptor
                                                          │
                                                    Bearer token in
                                                    API requests
```

### Axios Interceptor Adaptation

The Axios 401 handler behaves differently per mode:

```typescript
api.interceptors.response.use(
  (response) => response,
  async (error) => {
    const originalRequest = error.config;

    if (error.response?.status === 401 && !originalRequest._retry) {
      originalRequest._retry = true;

      if (isEmbeddedMode()) {
        // Embedded: refresh token from Rust HTTP server, retry request
        const refreshed = await refreshEmbeddedToken();
        if (refreshed) {
          const newToken = localStorage.getItem("token");
          originalRequest.headers.Authorization = `Bearer ${newToken}`;
          return api(originalRequest);
        }
        // Refresh failed — reject without redirect (Business OS re-authenticates)
        return Promise.reject(error);
      }

      // Standalone: clear token and redirect to login
      localStorage.removeItem("token");
      window.location.href = "/login";
    }

    return Promise.reject(error);
  },
);
```

Key differences:
- **Embedded mode**: Attempts token refresh via `/api/auth`, retries the failed request
- **Standalone mode**: Clears token and redirects to `/login`
- Both modes use `_retry` flag to prevent infinite loops

## HTTP Server

### Frontend Discovery

The Axum server locates the frontend build directory using a priority-based search:

1. `TORCH_FRONTEND_PATH` env var (explicit override)
2. `TORCH_INSTALL_PATH` + standard subdirectories
3. Relative to executable (`.deb` resource paths)
4. Development path (`../dist` relative to `src-tauri/`)
5. Current working directory (`dist/`)

### Dynamic Port

```rust
// Bind to port 0 — OS assigns an available port
let listener = TcpListener::bind("127.0.0.1:0")?;
let port = listener.local_addr().unwrap().port();
drop(listener); // Release for tokio to rebind

let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
```

The assigned port is sent to Business OS via the `EmbeddedReady` message.

## Embedded Mode Runner

### Orchestration

```rust
pub async fn run_embedded(config: EmbeddedConfig) -> AppResult<()> {
    // 1. Create shutdown broadcast channel
    let (shutdown_tx, _) = broadcast::channel::<()>(16);

    // 2. Create shared app state
    let app_state = Arc::new(AppState::new());

    // 3. Start HTTP server (frontend + /api/auth)
    let http_port = start_http_server(
        app_state.clone(),
        shutdown_tx.subscribe(),
    ).await?;

    // 4. Connect to Business OS IPC
    let mut ipc_client = IpcClient::connect(
        config.ipc_port,
        app_state.clone(),
        shutdown_tx.clone(),
    ).await?;

    // 5. Perform handshake
    ipc_client.handshake(&config.run_id, http_port).await?;

    // 6. Register signal handlers
    let tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    if let Ok(mut s) = signal(SignalKind::terminate()) {
                        s.recv().await;
                    }
                }
                #[cfg(not(unix))]
                std::future::pending::<()>().await
            } => {}
        }
        let _ = tx.send(());
    });

    // 7. Run message loop (blocks until shutdown)
    ipc_client.run_message_loop().await?;

    Ok(())
}
```

## Multi-Entity Support

Business OS supports users belonging to multiple entities (companies/holdings). When the user switches entity:

1. Business OS sends new `EntityContext` with updated `entity_id`, `entity_name`, `role`
2. App updates `AppState` with new user context
3. Business OS may send a new `AuthToken` scoped to the new entity
4. Frontend picks up changes on next `/api/auth` poll

```rust
IpcPayload::EntityContext { entity_id, entity_name, role } => {
    let user_context = UserContext {
        user_id: String::new(),
        entity_id,
        email: entity_name.clone(),
        name: Some(entity_name),
        role: role.unwrap_or_else(|| "member".to_string()),
    };
    self.app_state.set_current_user(user_context).await;
}
```

## Error Handling

### Session Expiry

```rust
IpcPayload::Error { code, message } => {
    if code == "SESSION_EXPIRED" {
        self.app_state.clear_auth().await;
        // Frontend will get authenticated: false on next poll
        // Business OS will re-authenticate and send new AuthToken
    }
}
```

### Token Refresh Failure

```rust
IpcPayload::TokenRefreshResponse { token, expires_at, error } => {
    if let Some(err) = error {
        if err == "NOT_AUTHENTICATED" {
            self.app_state.clear_auth().await;
        }
    } else if let Some(new_token) = token {
        let exp = expires_at.unwrap_or(0);
        self.app_state.set_current_token(new_token, exp).await;
    }
}
```

### Connection Loss

If the TCP connection to Business OS is lost, the app triggers a graceful shutdown:

```rust
Err(e) => {
    tracing::error!("IPC read error: {}", e);
    let _ = self.shutdown_tx.send(());
    break;
}
```

## File Organization

```
src-tauri/src/
├── main.rs                     # Entry point → lib::run()
├── lib.rs                      # Dual-mode detection + startup
├── state.rs                    # AppState (token, user context)
├── error.rs                    # AppError enum
└── embedded/
    ├── mod.rs                  # is_embedded_mode(), EmbeddedConfig
    ├── messages.rs             # IpcEnvelope, IpcPayload
    ├── http_server.rs          # Axum: /api/auth + static files
    ├── ipc_client.rs           # TCP client: send/receive/handshake
    └── runner.rs               # Orchestration: run_embedded()

src/
├── lib/embedded.ts             # isEmbeddedMode(), fetchEmbeddedAuth(), waitForEmbeddedAuth(), refreshEmbeddedToken()
├── hooks/use-embedded-auth.ts  # React Query polling hook (convenience, AuthProvider has its own polling)
├── hooks/use-auth.ts           # AuthContext, AuthUser, useAuth hook
├── providers/auth-provider.tsx # AuthProvider: probe → wait → poll lifecycle with phase tracking
└── api/axios.ts                # Axios with embedded token refresh + request retry on 401
```

## Best Practices

### Rust Backend
- **Check embedded mode first** — detect `TORCH_EMBEDDED_MODE` before initializing Tauri. Embedded mode doesn't create a Tauri window.
- **Use dynamic ports** — bind HTTP server to `127.0.0.1:0` and report the port to Business OS. Never hardcode ports.
- **Store tokens in memory** — use `RwLock<Option<String>>` in Rust. Never persist tokens to disk in embedded mode.
- **Enforce message size limits** — reject IPC messages over 1 MB to prevent memory exhaustion attacks.
- **Handle connection loss** — if the IPC connection drops, shut down gracefully. Business OS will restart the app if needed.
- **Log lifecycle events** — startup, handshake, auth received, entity switch, shutdown. This is critical for debugging.
- **Support entity switching** — handle `EntityContext` messages that update the user's entity without requiring a restart.
- **Use broadcast channels** for shutdown coordination — the HTTP server, IPC client, and signal handlers all need to trigger/receive shutdown.

### Frontend
- **Use two-step embedded detection** — heuristic `isEmbeddedMode()` + probe `/api/auth` confirmation. Never rely on heuristic alone.
- **Return null on fetch failure** — `fetchEmbeddedAuth()` must return `null` (not throw) when `/api/auth` is unreachable. This enables the probe fallback.
- **Wait for initial auth** — use `waitForEmbeddedAuth()` with a 60s timeout on startup. Business OS sends auth asynchronously after IPC handshake.
- **Capture embedded mode in useRef** — avoid re-evaluating `isEmbeddedMode()` on every render. Capture once on mount.
- **Show loading state** — display a loading spinner while waiting for embedded auth. The user needs feedback during the 1-60s wait.
- **Handle session expiry** — if embedded auth becomes `authenticated: false`, clear local state and re-poll for re-authentication.
- **Never redirect to /login in embedded mode** — Business OS manages authentication. On 401, attempt token refresh via `/api/auth` instead.
- **Retry failed requests** — after embedded token refresh succeeds, retry the original 401 request with the new token.
- **Sync token to localStorage** — embedded auth tokens must be written to `localStorage` so the Axios interceptor can attach them as Bearer tokens.
