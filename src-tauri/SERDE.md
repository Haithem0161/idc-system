# Serde Guide

Enterprise-grade patterns for serialization and deserialization in Rust with serde.

## Installation

```toml
# Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rmp-serde = "1"        # MessagePack (binary) serialization
```

## Basic Usage

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    email: String,
    age: u32,
}

// Serialize to JSON
let user = User {
    name: "Alice".to_string(),
    email: "alice@example.com".to_string(),
    age: 30,
};

let json = serde_json::to_string(&user).unwrap();
// {"name":"Alice","email":"alice@example.com","age":30}

let pretty = serde_json::to_string_pretty(&user).unwrap();

// Deserialize from JSON
let parsed: User = serde_json::from_str(&json).unwrap();
```

## Derive Macros

### Serialize & Deserialize

```rust
use serde::{Deserialize, Serialize};

// Both traits derived automatically
#[derive(Serialize, Deserialize)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

// Serialize-only (one-way)
#[derive(Serialize)]
struct ApiResponse {
    status: String,
    data: Vec<String>,
}

// Deserialize-only (incoming data)
#[derive(Deserialize)]
struct ApiRequest {
    query: String,
    limit: Option<u32>,
}
```

## Field Attributes

### Renaming

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    user_id: String,        // → "userId"
    first_name: String,     // → "firstName"
    last_name: String,      // → "lastName"
    is_active: bool,        // → "isActive"
}

// Other rename_all options:
// "snake_case", "camelCase", "PascalCase",
// "SCREAMING_SNAKE_CASE", "kebab-case"
```

### Field-Level Attributes

```rust
#[derive(Serialize, Deserialize)]
struct Config {
    // Rename a specific field
    #[serde(rename = "type")]
    kind: String,

    // Use default value if missing
    #[serde(default)]
    retries: u32,

    // Custom default
    #[serde(default = "default_timeout")]
    timeout: u64,

    // Skip serialization
    #[serde(skip)]
    internal_cache: Vec<u8>,

    // Skip if None
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,

    // Flatten nested struct
    #[serde(flatten)]
    metadata: Metadata,
}

fn default_timeout() -> u64 {
    30
}
```

### Optional Fields

```rust
#[derive(Serialize, Deserialize)]
struct Settings {
    // None serializes as null, missing key deserializes as None
    theme: Option<String>,

    // Skip null in output, default to None if missing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    custom_css: Option<String>,
}
```

## Enum Serialization

### Tagged Enums (Default)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Message {
    Text { content: String },
    Image { url: String, width: u32, height: u32 },
    Audio { url: String, duration: f64 },
}

// Serializes as: {"type":"Text","content":"hello"}
```

### Adjacently Tagged

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress { key: String },
    Scroll { delta: f64 },
}

// Serializes as: {"type":"Click","data":{"x":10,"y":20}}
```

### Untagged

```rust
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    Str(String),
    Num(f64),
}

// Tries each variant in order during deserialization
```

### Rename Variants

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Status {
    Active,          // → "ACTIVE"
    Inactive,        // → "INACTIVE"
    PendingReview,   // → "PENDING_REVIEW"
}
```

## JSON (serde_json)

### Working with Values

```rust
use serde_json::{json, Value};

// Build JSON dynamically
let data = json!({
    "name": "Alice",
    "age": 30,
    "tags": ["admin", "user"],
    "address": {
        "city": "NYC",
        "zip": "10001"
    }
});

// Access fields
let name = data["name"].as_str().unwrap();
let age = data["age"].as_u64().unwrap();
let city = data["address"]["city"].as_str().unwrap();

// Check types
if data["tags"].is_array() {
    let tags = data["tags"].as_array().unwrap();
}
```

### Streaming I/O

```rust
use std::io::{BufReader, BufWriter};
use std::fs::File;

// Read from file
let file = File::open("config.json").unwrap();
let reader = BufReader::new(file);
let config: Config = serde_json::from_reader(reader).unwrap();

// Write to file
let file = File::create("output.json").unwrap();
let writer = BufWriter::new(file);
serde_json::to_writer_pretty(writer, &config).unwrap();
```

### Raw JSON

```rust
use serde_json::value::RawValue;

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u8,
    // Keep payload as raw JSON without parsing
    #[serde(borrow)]
    payload: &'a RawValue,
}
```

## MessagePack (rmp-serde)

MessagePack is a binary serialization format — compact and fast, ideal for IPC.

### Basic Usage

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct IpcMessage {
    id: u32,
    data: Vec<u8>,
}

// Serialize to MessagePack bytes
let msg = IpcMessage {
    id: 1,
    data: vec![1, 2, 3],
};
let bytes: Vec<u8> = rmp_serde::to_vec(&msg).unwrap();

// Deserialize from MessagePack bytes
let decoded: IpcMessage = rmp_serde::from_slice(&bytes).unwrap();
```

### Length-Prefixed Protocol

Used for TCP IPC with Business OS:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn send_message(stream: &mut TcpStream, msg: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = rmp_serde::to_vec(msg)?;
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn receive_message<T: for<'a> Deserialize<'a>>(stream: &mut TcpStream) -> Result<T, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    Ok(rmp_serde::from_slice(&buf)?)
}
```

### JSON vs MessagePack Comparison

```rust
#[derive(Serialize, Deserialize)]
struct Data {
    id: u32,
    name: String,
    active: bool,
}

let data = Data { id: 1, name: "test".into(), active: true };

// JSON: 38 bytes (human-readable)
let json = serde_json::to_vec(&data).unwrap();

// MessagePack: ~20 bytes (binary, compact)
let msgpack = rmp_serde::to_vec(&data).unwrap();
```

## Custom Serialization

### Custom Serialize/Deserialize

```rust
use serde::{Serializer, Deserializer};

#[derive(Debug)]
struct Timestamp(i64);

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;
        Ok(Timestamp(value))
    }
}
```

### Serialize With Helper

```rust
mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
struct Event {
    name: String,
    #[serde(with = "date_format")]
    date: chrono::NaiveDate,
}
```

## Integration with Tauri

### Command Return Types

```rust
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    app_name: String,
    version: String,
    is_debug: bool,
}

// Tauri commands must return Serialize types
#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        app_name: "My App".to_string(),
        version: "1.0.0".to_string(),
        is_debug: cfg!(debug_assertions),
    }
}
```

### Command Error Types

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: String,
    message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[tauri::command]
fn risky_operation() -> Result<String, CommandError> {
    Err(CommandError {
        code: "INVALID_INPUT".to_string(),
        message: "Name is required".to_string(),
    })
}
```

## Integration with Axum

### Request/Response Bodies

```rust
use axum::{Json, extract::State as AxumState};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserRequest {
    first_name: String,
    last_name: String,
    email: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserResponse {
    user_id: String,
    created_at: String,
}

async fn create_user(
    Json(req): Json<CreateUserRequest>,
) -> Json<CreateUserResponse> {
    Json(CreateUserResponse {
        user_id: "123".to_string(),
        created_at: "2024-01-01".to_string(),
    })
}
```

## File Organization

```
src/
├── models/
│   ├── mod.rs          # pub mod declarations
│   ├── user.rs         # #[derive(Serialize, Deserialize)]
│   └── config.rs       # Configuration structs
├── dto/
│   ├── mod.rs          # Data Transfer Objects
│   ├── requests.rs     # API request types (Deserialize only)
│   └── responses.rs    # API response types (Serialize only)
└── protocol/
    └── messages.rs     # IPC message types (both)
```

## Best Practices

- **Use `rename_all = "camelCase"`** on structs that serialize to/from JavaScript — Rust uses `snake_case`, JS uses `camelCase`.
- **Derive both traits** unless you have a reason not to. Serialize-only for responses, Deserialize-only for requests.
- **Use `#[serde(default)]`** for optional fields with sensible defaults to make deserialization more forgiving.
- **Use `#[serde(skip_serializing_if = "Option::is_none")]`** to keep JSON output clean by omitting null fields.
- **Use tagged enums** (`#[serde(tag = "type")]`) for discriminated unions — they map cleanly to TypeScript.
- **Prefer MessagePack** for internal IPC communication — it's smaller and faster than JSON.
- **Use JSON** for HTTP APIs and human-readable configuration — it's universal and debuggable.
- **Keep DTOs separate** from domain types — derive serde on API boundary types, not core business logic.
- **Validate after deserialization** — serde handles structure, but business rules need separate validation.
- **Use `#[serde(deny_unknown_fields)]`** on request types when you want strict parsing and early error detection.
