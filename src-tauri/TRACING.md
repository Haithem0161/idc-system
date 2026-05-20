# Tracing Guide

Enterprise-grade patterns for structured logging and diagnostics in Rust with tracing.

## Installation

```toml
# Cargo.toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
log = "0.4"                    # Compatibility with log-based crates
tauri-plugin-log = "2"         # Tauri integration
```

## Basic Setup

```rust
fn main() {
    // Initialize the default subscriber with env filter
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Application started");
}
```

## Log Levels

```rust
// From most to least severe
tracing::error!("Connection failed: {}", err);
tracing::warn!("Retry attempt {} of {}", attempt, max);
tracing::info!("Server listening on port {}", port);
tracing::debug!("Processing request: {:?}", request);
tracing::trace!("Raw bytes received: {:?}", bytes);
```

### Conditional Logging

```rust
if tracing::enabled!(tracing::Level::DEBUG) {
    let expensive = compute_debug_info();
    tracing::debug!("Debug info: {}", expensive);
}
```

## Structured Logging

### Key-Value Fields

```rust
// Named fields with values
tracing::info!(port = 8080, host = "127.0.0.1", "Server started");
// Output: INFO Server started port=8080 host=127.0.0.1

// Variable capture (field name = variable name)
let port = 8080;
let host = "127.0.0.1";
tracing::info!(port, host, "Server started");

// Display vs Debug formatting
tracing::info!(user = %user_id, data = ?request, "Processing");
// %user_id uses Display, ?request uses Debug
```

### Typed Fields

```rust
tracing::info!(
    user_id = %id,
    entity_id = %entity,
    token_len = token.len(),
    expires_at = expires,
    "Auth token stored"
);

tracing::error!(
    error = %e,
    code = "SESSION_EXPIRED",
    "Session expired"
);
```

## Environment Filter

Control log levels via the `RUST_LOG` environment variable:

```bash
# Show all info and above
RUST_LOG=info cargo run

# Show debug for your crate, info for everything else
RUST_LOG=my_app=debug,info cargo run

# Show trace for a specific module
RUST_LOG=my_app::services::sync=trace cargo run

# Multiple targets
RUST_LOG=my_app=debug,axum=info,tower_http=debug cargo run

# Suppress noisy crates
RUST_LOG=info,hyper=warn,mio=warn cargo run
```

### Programmatic Filter

```rust
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter(
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| {
                EnvFilter::new("info")
                    .add_directive("my_app=debug".parse().unwrap())
                    .add_directive("axum=info".parse().unwrap())
            }),
    )
    .init();
```

## Spans

Spans represent a period of time with context attached.

### Function-Level Spans

```rust
use tracing::instrument;

#[instrument]
fn process_request(user_id: &str, action: &str) -> Result<(), Error> {
    tracing::info!("Processing");
    // All logs inside this function include user_id and action
    Ok(())
}

// Skip sensitive fields
#[instrument(skip(password))]
fn login(username: &str, password: &str) -> Result<Token, Error> {
    tracing::info!("Login attempt");
    Ok(Token::new())
}

// Custom span name and fields
#[instrument(name = "ipc_handle", fields(msg_type))]
async fn handle_message(msg: Message) -> Result<()> {
    tracing::Span::current().record("msg_type", &format!("{:?}", msg.kind));
    // ...
    Ok(())
}
```

### Manual Spans

```rust
let span = tracing::info_span!("http_request", method = "GET", path = "/api/auth");
let _guard = span.enter();

tracing::info!("Handling request");
// Output: INFO http_request{method="GET" path="/api/auth"}: Handling request

// Async-safe span (for async code)
async fn handle() {
    let span = tracing::info_span!("async_task", task_id = 42);
    async {
        tracing::info!("Inside task");
    }
    .instrument(span)
    .await;
}
```

## Subscriber Configuration

### Custom Formatting

```rust
tracing_subscriber::fmt()
    // Compact single-line format
    .compact()
    // Or full multi-line format
    // .pretty()
    // Include thread names
    .with_thread_names(true)
    // Include file and line numbers
    .with_file(true)
    .with_line_number(true)
    // Include target (module path)
    .with_target(true)
    // Custom time format
    .with_timer(tracing_subscriber::fmt::time::uptime())
    .init();
```

### JSON Output

```rust
tracing_subscriber::fmt()
    .json()
    .with_env_filter(EnvFilter::new("info"))
    .init();

// Output:
// {"timestamp":"2024-01-01T00:00:00Z","level":"INFO","message":"Server started","port":8080}
```

## Tauri Integration

### tauri-plugin-log

```rust
use tauri::Manager;

pub fn run() {
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
```

## Logging Patterns

### Startup Logging

```rust
tracing::info!("Starting app");
tracing::info!("  Version: {}", env!("CARGO_PKG_VERSION"));
tracing::info!("  Install Path: {:?}", config.install_path);
```

### Connection Logging

```rust
tracing::info!("HTTP server listening on http://{}", addr);
tracing::info!("Frontend available at http://127.0.0.1:{}", port);
```

### Message Logging

```rust
// Log message type without sensitive data
tracing::info!("IPC message received: {}", match &msg.payload {
    IpcPayload::Ping { .. } => "Ping",
    IpcPayload::AuthToken { .. } => "AuthToken",
    IpcPayload::Shutdown { .. } => "Shutdown",
    _ => "Other",
});

// Log token metadata, not the token itself
tracing::info!(
    token_len = token.len(),
    expires_at = expires_at,
    "Auth token stored"
);
```

### Error Logging

```rust
tracing::error!("IPC read error: {}", e);
tracing::error!("HTTP server error: {}", e);
tracing::error!(
    code = %code,
    message = %message,
    "Error from upstream"
);

tracing::warn!(
    "Session expired: {}. Clearing auth state.",
    message
);
```

### Shutdown Logging

```rust
tracing::info!("Received SIGINT (Ctrl+C), initiating shutdown");
tracing::info!("Received SIGTERM, initiating shutdown");
tracing::info!("HTTP server shutting down");
tracing::info!("Shutdown complete");
```

## Compatibility with `log` Crate

The `tracing` crate is compatible with the `log` crate. Libraries using `log::info!()` etc. will have their output captured by the tracing subscriber automatically.

```rust
// These both work and appear in the same output:
log::info!("From log crate");
tracing::info!("From tracing crate");
```

## File Organization

```
src/
├── lib.rs              # Initialize tracing_subscriber in run()
├── commands/           # Command logging with #[instrument]
└── services/           # Service logging with #[instrument]
```

## Best Practices

- **Initialize tracing early** — call `tracing_subscriber::fmt().init()` at the start of `run()` before any logging calls.
- **Use `RUST_LOG` env var** for runtime log level control — default to `"info"` when not set.
- **Use structured fields** (`tracing::info!(port = 8080, "Started")`) instead of string interpolation when possible.
- **Never log sensitive data** — log token length and expiry, not the token value. Log user IDs, not passwords.
- **Use `#[instrument]`** on async functions for automatic span creation with function arguments as fields.
- **Use `eprintln!`** for critical startup messages that must appear even if tracing isn't initialized yet.
- **Suppress noisy crates** — add `hyper=warn,mio=warn` to your `RUST_LOG` filter to reduce noise.
- **Use `tracing::debug!`** for detailed operational logging that's off by default but available for troubleshooting.
- **Log lifecycle events** — connection established, handshake complete, message received, shutdown initiated.
- **Use consistent message patterns** — prefix with component name or action for easy grep filtering.
