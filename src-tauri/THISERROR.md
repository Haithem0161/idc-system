# Thiserror Guide

Enterprise-grade patterns for error handling in Rust with thiserror.

## Installation

```toml
# Cargo.toml
[dependencies]
thiserror = "2"
```

## Basic Usage

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized")]
    Unauthorized,
}

// Usage
fn find_user(id: &str) -> Result<User, AppError> {
    Err(AppError::NotFound(format!("User {} not found", id)))
}
```

## Result Type Alias

```rust
/// Application result type alias — use throughout the crate.
pub type AppResult<T> = Result<T, AppError>;

fn process() -> AppResult<String> {
    Ok("done".to_string())
}
```

## Error Formatting

### String Interpolation

```rust
#[derive(Debug, Error)]
pub enum ConfigError {
    // Positional: {0} refers to the first field
    #[error("Missing key: {0}")]
    MissingKey(String),

    // Named fields
    #[error("Invalid value for {key}: expected {expected}, got {actual}")]
    InvalidValue {
        key: String,
        expected: String,
        actual: String,
    },

    // Display trait of inner type
    #[error("Parse error: {0}")]
    Parse(#[from] std::num::ParseIntError),
}
```

### Transparent Errors

```rust
#[derive(Debug, Error)]
pub enum Error {
    // Delegates Display and source() to the inner error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

## From Implementations

### Automatic Conversion

```rust
#[derive(Debug, Error)]
pub enum AppError {
    // #[from] generates From<std::io::Error> for AppError
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal: {0}")]
    Internal(String),
}

// Now the ? operator converts automatically:
fn read_config() -> Result<Config, AppError> {
    let data = std::fs::read_to_string("config.json")?;  // io::Error → AppError::Io
    let config: Config = serde_json::from_str(&data)?;    // serde_json::Error → AppError::Json
    Ok(config)
}
```

### Manual From Implementation

```rust
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Internal error: {0}")]
    Internal(String),
}

// Custom conversion with context
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
```

## Error Source Chaining

```rust
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Failed to read file")]
    ReadFailed(#[source] std::io::Error),

    #[error("Failed to parse config")]
    ParseFailed {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

// Access the chain:
fn handle_error(e: &StorageError) {
    eprintln!("Error: {}", e);
    if let Some(source) = std::error::Error::source(e) {
        eprintln!("Caused by: {}", source);
    }
}
```

## Error Hierarchy

### Domain-Specific Errors

```rust
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Insufficient permissions: requires {required}")]
    InsufficientPermissions { required: String },
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Record not found")]
    NotFound,
}

// Top-level error composes domain errors
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Authentication failed: {0}")]
    Auth(#[from] AuthError),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
```

## Integration with Axum

### IntoResponse for Errors

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Unauthorized".into()),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", msg.clone()),
        };

        (
            status,
            Json(json!({
                "error": {
                    "code": code,
                    "message": message,
                }
            })),
        )
            .into_response()
    }
}

// Use in handlers:
async fn get_user(id: String) -> Result<Json<User>, ApiError> {
    let user = find_user(&id)
        .ok_or_else(|| ApiError::NotFound(format!("User {}", id)))?;
    Ok(Json(user))
}
```

## Integration with Tauri

### Serializable Errors for Commands

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Internal: {0}")]
    Internal(String),
}

// Tauri commands need Serialize for error types
impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
fn get_data(id: String) -> Result<Data, CommandError> {
    Err(CommandError::NotFound(id))
}
```

## Patterns

### Error Context

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Failed to {action}: {source}")]
    WithContext {
        action: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Internal error: {0}")]
    Internal(String),
}

// Helper for adding context
impl AppError {
    pub fn context(action: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        AppError::WithContext {
            action: action.into(),
            source: Box::new(source),
        }
    }
}

// Usage
fn load_config() -> AppResult<Config> {
    std::fs::read_to_string("config.json")
        .map_err(|e| AppError::context("load configuration", e))?;
    todo!()
}
```

### Error Codes for API Responses

```rust
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Session expired")]
    SessionExpired,

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Machine-readable error code for API consumers.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotAuthenticated => "NOT_AUTHENTICATED",
            AppError::SessionExpired => "SESSION_EXPIRED",
            AppError::Configuration(_) => "CONFIGURATION_ERROR",
            AppError::Internal(_) => "INTERNAL_ERROR",
        }
    }
}
```

## File Organization

```
src/
├── error.rs            # Top-level AppError + AppResult
├── error/
│   ├── mod.rs          # Re-exports
│   ├── auth.rs         # AuthError variants
│   └── database.rs     # DatabaseError variants
```

## Best Practices

- **Define a top-level `AppError`** enum and `AppResult<T>` alias for consistent error handling across the crate.
- **Use `#[from]`** for automatic error conversion from library errors — it generates `From` implementations.
- **Use `#[source]`** when you want error chaining without automatic `From` conversion.
- **Use `#[error(transparent)]`** when your error variant just wraps another error without adding context.
- **Keep error variants specific** — `NotFound`, `Unauthorized`, `Validation` are better than a generic `Error(String)`.
- **Implement `IntoResponse`** for Axum handler errors to map each variant to the correct HTTP status code.
- **Implement `Serialize`** for Tauri command errors so they can be sent to the frontend.
- **Use error codes** (uppercase snake_case strings) alongside messages for machine-readable API error handling.
- **Compose domain errors** into a top-level error using `#[from]` — this keeps domain logic clean while allowing `?` propagation.
- **Don't expose internal details** in production error messages — log the full error, but return a sanitized message to the client.
