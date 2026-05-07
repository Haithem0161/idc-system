# Axum Guide

Enterprise-grade patterns for building HTTP servers with Axum, a modular web framework built on Tokio, Tower, and Hyper.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "fs"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Basic Usage

```rust
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build the router
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }));

    // Bind and serve
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

## Router

The `Router` is the central type for defining your application's route table. It supports path-based routing, nesting, merging, and fallback handlers.

### Creating Routes

```rust
use axum::{
    routing::{get, post, put, patch, delete},
    Router,
};

let app = Router::new()
    .route("/", get(root_handler))
    .route("/users", get(list_users).post(create_user))
    .route("/users/{id}", get(get_user).put(update_user).delete(delete_user))
    .route("/health", get(|| async { "OK" }));
```

### Path Parameters

```rust
use axum::{routing::get, Router};

let app = Router::new()
    // Single parameter
    .route("/users/{id}", get(get_user))
    // Multiple parameters
    .route("/orgs/{org_id}/teams/{team_id}", get(get_team))
    // Wildcard (catch-all)
    .route("/files/{*path}", get(serve_file));
```

### Nesting Routers

Nest a sub-router under a common prefix. All routes within the nested router inherit the prefix path.

```rust
use axum::{routing::get, Router};

fn users_router() -> Router {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
}

fn posts_router() -> Router {
    Router::new()
        .route("/", get(list_posts).post(create_post))
        .route("/{id}", get(get_post))
}

let app = Router::new()
    .nest("/api/users", users_router())
    .nest("/api/posts", posts_router());

// Results in:
// GET  /api/users
// POST /api/users
// GET  /api/users/:id
// PUT  /api/users/:id
// DELETE /api/users/:id
// GET  /api/posts
// POST /api/posts
// GET  /api/posts/:id
```

### Merging Routers

Combine two routers at the same path level. Useful for composing independent modules.

```rust
use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;
use tower_http::compression::CompressionLayer;

let public_routes = Router::new()
    .route("/", get(home))
    .route("/health", get(health_check));

let api_routes = Router::new()
    .route("/api/users", get(list_users))
    .route("/api/posts", get(list_posts))
    .layer(TraceLayer::new_for_http());

let admin_routes = Router::new()
    .route("/admin/dashboard", get(admin_dashboard))
    .route("/admin/settings", get(admin_settings))
    .layer(CompressionLayer::new());

let app = Router::new()
    .merge(public_routes)
    .merge(api_routes)
    .merge(admin_routes);
```

### Fallback Handler

Define a handler for requests that match no routes. Without a fallback, unmatched requests return `404 Not Found` by default.

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;

async fn fallback_handler() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "not_found",
            "message": "The requested resource does not exist"
        })),
    )
}

let app = Router::new()
    .route("/api/users", get(list_users))
    .fallback(fallback_handler);
```

## Handlers

Handlers are async functions that accept zero or more extractors as arguments and return something that implements `IntoResponse`.

### Basic Handlers

```rust
use axum::{
    extract::{Path, Query, Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

// No extractors - simplest handler
async fn root() -> &'static str {
    "Hello, World!"
}

// Return a status code
async fn health_check() -> StatusCode {
    StatusCode::OK
}

// Multiple extractors + typed response
async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), AppError> {
    let user = state.db.insert_user(payload).await?;
    Ok((StatusCode::CREATED, Json(user)))
}
```

### Handler Function Signatures

Axum handlers are regular async functions. Extractors are applied in the order they appear as parameters. The last extractor can consume the request body (e.g., `Json`, `String`, `Bytes`).

```rust
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Deserialize)]
struct CreatePost {
    title: String,
    body: String,
}

// Combining multiple extractors
async fn create_post_for_user(
    State(state): State<AppState>,
    Path(user_id): Path<u64>,
    Query(pagination): Query<Pagination>,
    headers: HeaderMap,
    Json(payload): Json<CreatePost>,
) -> impl IntoResponse {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let page = pagination.page.unwrap_or(1);

    // ... handler logic
    StatusCode::CREATED
}
```

### Returning impl IntoResponse

Any type implementing `IntoResponse` can be returned. You can also implement it for your own types.

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response, Json},
};
use serde_json::json;

enum ApiResponse {
    Ok,
    Created(String),
    Error(StatusCode, String),
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> Response {
        match self {
            ApiResponse::Ok => StatusCode::OK.into_response(),
            ApiResponse::Created(id) => {
                (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
            }
            ApiResponse::Error(status, message) => {
                (status, Json(json!({ "error": message }))).into_response()
            }
        }
    }
}

async fn handler() -> ApiResponse {
    ApiResponse::Created("user_123".to_string())
}
```

## Extractors

Extractors parse incoming requests into typed data. They are used as handler function parameters.

### Json Extractor

Parse the request body as JSON into a typed struct.

```rust
use axum::{extract::Json, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CreateUserRequest {
    name: String,
    email: String,
    role: Option<String>,
}

#[derive(Serialize)]
struct UserResponse {
    id: u64,
    name: String,
    email: String,
}

async fn create_user(
    Json(payload): Json<CreateUserRequest>,
) -> (StatusCode, Json<UserResponse>) {
    let user = UserResponse {
        id: 1,
        name: payload.name,
        email: payload.email,
    };

    (StatusCode::CREATED, Json(user))
}
```

### Path Extractor

Extract path parameters from the URL.

```rust
use axum::extract::Path;

// Single path parameter
async fn get_user(Path(user_id): Path<u64>) -> String {
    format!("User ID: {user_id}")
}

// Multiple path parameters via tuple
async fn get_team_member(
    Path((org_id, team_id, member_id)): Path<(String, u64, u64)>,
) -> String {
    format!("Org: {org_id}, Team: {team_id}, Member: {member_id}")
}

// Multiple path parameters via struct
#[derive(Deserialize)]
struct TeamPath {
    org_id: String,
    team_id: u64,
}

async fn get_team(Path(path): Path<TeamPath>) -> String {
    format!("Org: {}, Team: {}", path.org_id, path.team_id)
}
```

### Query Extractor

Parse query string parameters into a typed struct.

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct ListParams {
    page: Option<u32>,
    per_page: Option<u32>,
    sort_by: Option<String>,
    order: Option<String>,
    search: Option<String>,
}

// GET /users?page=2&per_page=20&sort_by=name&order=asc&search=john
async fn list_users(Query(params): Query<ListParams>) -> String {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(10);
    let sort_by = params.sort_by.unwrap_or_else(|| "id".to_string());

    format!(
        "Listing users: page={page}, per_page={per_page}, sort_by={sort_by}"
    )
}
```

### State Extractor

Access shared application state injected via `with_state()`.

```rust
use axum::extract::State;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    db_pool: Arc<DatabasePool>,
    config: Arc<AppConfig>,
}

async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    let users = state.db_pool.query("SELECT * FROM users").await;
    // ...
}

// Access specific fields via sub-state
async fn get_config(State(state): State<AppState>) -> String {
    format!("App: {}", state.config.app_name)
}
```

### Headers Extractor

Access request headers.

```rust
use axum::http::header::HeaderMap;

// Get all headers
async fn with_headers(headers: HeaderMap) -> String {
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    format!("User-Agent: {user_agent}")
}

// Extract specific typed headers
use axum::extract::TypedHeader;
use axum::headers::{Authorization, authorization::Bearer};

async fn protected(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> String {
    let token = auth.token();
    format!("Token: {token}")
}
```

### Combining Multiple Extractors

```rust
use axum::{
    extract::{Path, Query, State, Json},
    http::HeaderMap,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct UpdateUserRequest {
    name: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
struct QueryOptions {
    dry_run: Option<bool>,
}

async fn update_user(
    State(state): State<AppState>,
    Path(user_id): Path<u64>,
    Query(opts): Query<QueryOptions>,
    headers: HeaderMap,
    Json(body): Json<UpdateUserRequest>,
) -> impl IntoResponse {
    let is_dry_run = opts.dry_run.unwrap_or(false);
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");

    tracing::info!(
        user_id,
        request_id,
        dry_run = is_dry_run,
        "Updating user"
    );

    // ... update logic

    StatusCode::OK
}
```

## State Management

### Using with_state()

Provide shared state to all handlers via the `State` extractor.

```rust
use axum::{extract::State, routing::get, Router};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    db_pool: Arc<DatabasePool>,
    cache: Arc<RedisPool>,
    config: Arc<AppConfig>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        db_pool: Arc::new(DatabasePool::new().await),
        cache: Arc::new(RedisPool::new().await),
        config: Arc::new(AppConfig::from_env()),
    };

    let app = Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    let users = state.db_pool.fetch_all_users().await;
    Json(users)
}
```

### Arc Pattern for Shared Mutable State

Use `Arc<Mutex<T>>` when state must be mutated across handlers. Prefer `tokio::sync::Mutex` if the lock is held across `.await` points.

```rust
use axum::{extract::State, routing::{get, post}, Router, Json};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    // Use Arc<Mutex<T>> for mutable shared state
    documents: Arc<Mutex<Vec<Document>>>,
    // Use Arc for immutable shared data
    config: Arc<AppConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Document {
    id: u64,
    title: String,
    content: String,
}

async fn list_documents(State(state): State<AppState>) -> Json<Vec<Document>> {
    let docs = state.documents.lock().expect("mutex was poisoned");
    Json(docs.clone())
}

async fn create_document(
    State(state): State<AppState>,
    Json(doc): Json<Document>,
) -> (StatusCode, Json<Document>) {
    {
        let mut docs = state.documents.lock().expect("mutex was poisoned");
        docs.push(doc.clone());
    } // Lock is dropped here before any .await

    (StatusCode::CREATED, Json(doc))
}

#[tokio::main]
async fn main() {
    let state = AppState {
        documents: Arc::new(Mutex::new(Vec::new())),
        config: Arc::new(AppConfig::default()),
    };

    let app = Router::new()
        .route("/documents", get(list_documents).post(create_document))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### State with Async Mutex

When the lock must be held across `.await` points, use `tokio::sync::Mutex`.

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<DatabaseConnection>>,
}

async fn handler(State(state): State<AppState>) -> impl IntoResponse {
    // tokio::sync::Mutex is safe to hold across .await
    let mut db = state.db.lock().await;
    let result = db.query("SELECT 1").await;
    Json(result)
}
```

### Nested State with Sub-Routers

```rust
use axum::{extract::State, routing::get, Router};

#[derive(Clone)]
struct UserServiceState {
    user_db: Arc<UserDatabase>,
}

#[derive(Clone)]
struct PostServiceState {
    post_db: Arc<PostDatabase>,
}

fn user_routes() -> Router<UserServiceState> {
    Router::new()
        .route("/", get(list_users))
        .route("/{id}", get(get_user))
}

fn post_routes() -> Router<PostServiceState> {
    Router::new()
        .route("/", get(list_posts))
        .route("/{id}", get(get_post))
}

// Each sub-router gets its own state type
let app = Router::new()
    .nest("/users", user_routes().with_state(user_state))
    .nest("/posts", post_routes().with_state(post_state));
```

## Response Types

Axum provides flexible response types through the `IntoResponse` trait.

### Built-in Response Types

```rust
use axum::{
    http::StatusCode,
    response::{Json, Html, Redirect, IntoResponse},
};
use serde_json::{json, Value};

// Plain text (200 OK, content-type: text/plain)
async fn plain_text() -> &'static str {
    "Hello, World!"
}

// String (200 OK, content-type: text/plain)
async fn dynamic_text() -> String {
    format!("Current time: {}", chrono::Utc::now())
}

// HTML response
async fn html_page() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

// JSON response (200 OK, content-type: application/json)
async fn json_response() -> Json<Value> {
    Json(json!({ "message": "Hello", "status": "ok" }))
}

// Status code only (no body)
async fn no_content() -> StatusCode {
    StatusCode::NO_CONTENT
}

// Redirect
async fn redirect() -> Redirect {
    Redirect::to("/new-location")
}
```

### Tuple Response (StatusCode + Body)

```rust
use axum::{http::StatusCode, response::Json};
use serde::Serialize;

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

// (StatusCode, Json<T>) tuple
async fn create_user() -> (StatusCode, Json<User>) {
    let user = User {
        id: 1,
        name: "Alice".to_string(),
    };
    (StatusCode::CREATED, Json(user))
}

// (StatusCode, headers, body) tuple
async fn with_custom_headers() -> (StatusCode, [(String, String); 1], Json<User>) {
    let user = User { id: 1, name: "Alice".to_string() };
    (
        StatusCode::OK,
        [("x-request-id".to_string(), "abc-123".to_string())],
        Json(user),
    )
}
```

### Custom IntoResponse Implementation

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response, Json},
};
use serde::Serialize;
use serde_json::json;

// Custom error type
enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
    Unauthorized,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required".to_string(),
            ),
        };

        (
            status,
            Json(json!({
                "error": error_code,
                "message": message,
            })),
        )
            .into_response()
    }
}

// Use Result<T, AppError> in handlers
async fn get_user(Path(id): Path<u64>) -> Result<Json<User>, AppError> {
    let user = find_user(id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("User {id} not found")))?;

    Ok(Json(user))
}
```

### Typed JSON Response Wrapper

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response, Json}};
use serde::Serialize;

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> (StatusCode, Json<Self>) {
        (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(data),
                error: None,
            }),
        )
    }

    fn created(data: T) -> (StatusCode, Json<Self>) {
        (
            StatusCode::CREATED,
            Json(ApiResponse {
                success: true,
                data: Some(data),
                error: None,
            }),
        )
    }

    fn error(status: StatusCode, message: &str) -> (StatusCode, Json<Self>) {
        (
            status,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(message.to_string()),
            }),
        )
    }
}
```

## Static File Serving

Use `tower-http`'s `ServeDir` to serve static files from a directory.

### Basic Static File Serving

```rust
use axum::Router;
use tower_http::services::ServeDir;

let app = Router::new()
    .nest_service("/static", ServeDir::new("assets"));

// Serves files from the "assets" directory:
// GET /static/style.css  ->  assets/style.css
// GET /static/js/app.js  ->  assets/js/app.js
```

### Serving with Index HTML Fallback

Serve an `index.html` as the default when a directory is requested. This is essential for single-page applications (SPAs).

```rust
use axum::{routing::get, Router};
use tower_http::services::{ServeDir, ServeFile};

let app = Router::new()
    // API routes
    .route("/api/health", get(health_check))
    // Serve SPA - fallback to index.html for client-side routing
    .fallback_service(
        ServeDir::new("dist")
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new("dist/index.html")),
    );
```

### Combining API Routes with Static Files

```rust
use axum::{routing::get, Router};
use tower_http::services::{ServeDir, ServeFile};

fn api_router() -> Router {
    Router::new()
        .route("/users", get(list_users))
        .route("/posts", get(list_posts))
}

let app = Router::new()
    // API routes under /api prefix
    .nest("/api", api_router())
    // Serve static assets (CSS, JS, images)
    .nest_service("/assets", ServeDir::new("public/assets"))
    // Favicon
    .route_service("/favicon.ico", ServeFile::new("public/favicon.ico"))
    // SPA fallback - serves index.html for all unmatched routes
    .fallback_service(
        ServeDir::new("public")
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new("public/index.html")),
    );
```

### Serving Files with Cache Headers

```rust
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use axum::http::header;

let app = Router::new()
    .nest_service(
        "/static",
        ServeDir::new("assets").layer(
            SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ),
    );
```

## Middleware

Axum leverages the Tower middleware ecosystem. Middleware is applied using `.layer()` on a `Router`.

### Tower Layer Basics

```rust
use axum::{routing::get, Router};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use std::time::Duration;

let app = Router::new()
    .route("/", get(root))
    .route("/users", get(list_users))
    .layer(
        ServiceBuilder::new()
            // Layers execute top-to-bottom for requests, bottom-to-top for responses
            .layer(TraceLayer::new_for_http())
            .timeout(Duration::from_secs(30))
    );
```

### CORS Middleware

```rust
use axum::{routing::get, Router};
use tower_http::cors::{CorsLayer, Any};
use axum::http::{header, Method};

// Permissive CORS for development
let cors_dev = CorsLayer::very_permissive();

// Restrictive CORS for production
let cors_prod = CorsLayer::new()
    .allow_origin([
        "https://example.com".parse().unwrap(),
        "https://app.example.com".parse().unwrap(),
    ])
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([
        header::CONTENT_TYPE,
        header::AUTHORIZATION,
        header::ACCEPT,
    ])
    .allow_credentials(true)
    .max_age(Duration::from_secs(3600));

let app = Router::new()
    .route("/api/users", get(list_users))
    .layer(cors_prod);
```

### Logging / Tracing Middleware

```rust
use axum::{routing::get, Router};
use tower_http::trace::{TraceLayer, DefaultMakeSpan, DefaultOnResponse};
use tracing::Level;

let app = Router::new()
    .route("/", get(root))
    .layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );
```

### Custom Middleware with axum::middleware::from_fn

```rust
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};

// Custom auth middleware
async fn auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(token) if token.starts_with("Bearer ") => {
            // Token validation logic here
            let response = next.run(request).await;
            Ok(response)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// Middleware that adds a request ID
async fn request_id_middleware(
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    request
        .headers_mut()
        .insert("x-request-id", request_id.parse().unwrap());

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-request-id", request_id.parse().unwrap());

    response
}

let app = Router::new()
    .route("/protected", get(protected_handler))
    .layer(middleware::from_fn(auth_middleware))
    .route("/public", get(public_handler))
    .layer(middleware::from_fn(request_id_middleware));

// Note: .layer() applies to all routes defined ABOVE it.
// In this example, auth_middleware applies only to /protected,
// while request_id_middleware applies to both routes.
```

### Middleware with State

```rust
use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    Router,
};

#[derive(Clone)]
struct AppState {
    api_keys: Vec<String>,
}

async fn api_key_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if state.api_keys.contains(&api_key.to_string()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

let state = AppState {
    api_keys: vec!["key-1".to_string(), "key-2".to_string()],
};

let app = Router::new()
    .route("/api/data", get(get_data))
    .layer(middleware::from_fn_with_state(state.clone(), api_key_middleware))
    .with_state(state);
```

### Applying Different Middleware to Route Groups

```rust
use axum::{routing::get, Router, middleware};
use tower_http::trace::TraceLayer;

let public_routes = Router::new()
    .route("/", get(home))
    .route("/health", get(health_check));

let authenticated_routes = Router::new()
    .route("/dashboard", get(dashboard))
    .route("/settings", get(settings))
    .layer(middleware::from_fn(auth_middleware));

let admin_routes = Router::new()
    .route("/admin/users", get(admin_users))
    .route("/admin/logs", get(admin_logs))
    .layer(middleware::from_fn(admin_middleware))
    .layer(middleware::from_fn(auth_middleware));

let app = Router::new()
    .merge(public_routes)
    .merge(authenticated_routes)
    .merge(admin_routes)
    .layer(TraceLayer::new_for_http()); // Applied to all routes
```

## Graceful Shutdown

Configure the server to finish processing in-flight requests before shutting down.

### Basic Graceful Shutdown with Ctrl+C

```rust
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    tracing::info!("Server shut down gracefully");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    tracing::info!("Received shutdown signal");
}
```

### Graceful Shutdown with Broadcast Channel

Use a broadcast channel for programmatic shutdown control, useful when integrating with other systems like Tauri.

```rust
use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/shutdown", get({
            let shutdown_tx = shutdown_tx.clone();
            move || async move {
                let _ = shutdown_tx.send(());
                "Shutting down..."
            }
        }));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
            tracing::info!("Graceful shutdown initiated");
        })
        .await
        .unwrap();
}
```

### Combined Signal Handling (Unix + Ctrl+C)

```rust
use tokio::signal;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received SIGINT"),
        _ = terminate => tracing::info!("Received SIGTERM"),
    }

    tracing::info!("Starting graceful shutdown...");
}
```

## Dynamic Port Binding

Bind to port `0` to let the OS assign an available port, then retrieve it. This is useful for tests, local development, and Tauri desktop app integrations.

### Bind to Port 0 and Retrieve Assigned Port

```rust
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }));

    // Bind to port 0 - OS assigns an available port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    // Retrieve the assigned port
    let addr = listener.local_addr().unwrap();
    let port = addr.port();
    tracing::info!("Server listening on port {port}");

    axum::serve(listener, app).await.unwrap();
}
```

### Dynamic Port with State (Tauri Integration Pattern)

```rust
use axum::{extract::State, routing::get, Router, Json};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, oneshot};
use std::sync::Arc;
use serde::Serialize;

#[derive(Clone)]
struct ServerState {
    shutdown_tx: Arc<broadcast::Sender<()>>,
}

#[derive(Serialize)]
struct ServerInfo {
    port: u16,
    host: String,
}

/// Start the server on a dynamic port and return the assigned port
/// via a oneshot channel.
async fn start_server(
    port_tx: oneshot::Sender<u16>,
    shutdown_tx: broadcast::Sender<()>,
) {
    let state = ServerState {
        shutdown_tx: Arc::new(shutdown_tx.clone()),
    };

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/info", get(server_info))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Send the assigned port back to the caller
    let _ = port_tx.send(port);

    let mut shutdown_rx = shutdown_tx.subscribe();

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await
        .unwrap();
}

async fn server_info(State(state): State<ServerState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        port: 0, // Would be populated from actual state
        host: "127.0.0.1".to_string(),
    })
}

#[tokio::main]
async fn main() {
    let (port_tx, port_rx) = oneshot::channel();
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Spawn the server
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        start_server(port_tx, shutdown_tx_clone).await;
    });

    // Wait for the port to be assigned
    let port = port_rx.await.unwrap();
    println!("Server started on http://127.0.0.1:{port}");

    // Server runs in background, main task can do other work...
    // To stop: shutdown_tx.send(()).unwrap();
}
```

## File Organization

Recommended module structure for an enterprise Axum application:

```
src/
├── main.rs                 # Entry point: server bootstrap + graceful shutdown
├── lib.rs                  # App builder, re-exports
├── config.rs               # Configuration loading (env vars, files)
├── state.rs                # AppState definition + constructors
├── routes/
│   ├── mod.rs              # Route composition: merge all sub-routers
│   ├── health.rs           # Health check endpoints
│   ├── users.rs            # User CRUD routes + handlers
│   ├── posts.rs            # Post CRUD routes + handlers
│   └── auth.rs             # Auth routes (login, register, refresh)
├── handlers/               # (Alternative) Handlers separated from routes
│   ├── mod.rs
│   ├── users.rs
│   └── posts.rs
├── middleware/
│   ├── mod.rs              # Middleware re-exports
│   ├── auth.rs             # Authentication middleware
│   ├── request_id.rs       # Request ID injection
│   └── logging.rs          # Custom logging middleware
├── models/
│   ├── mod.rs              # Model re-exports
│   ├── user.rs             # User struct, DB model
│   └── post.rs             # Post struct, DB model
├── errors/
│   ├── mod.rs              # Error type re-exports
│   └── app_error.rs        # AppError enum + IntoResponse impl
├── extractors/
│   ├── mod.rs              # Custom extractor re-exports
│   └── auth.rs             # Custom auth extractor
└── db/
    ├── mod.rs              # Database pool setup
    ├── users.rs            # User queries
    └── posts.rs            # Post queries
```

### Example: Route Composition in routes/mod.rs

```rust
// src/routes/mod.rs
mod auth;
mod health;
mod posts;
mod users;

use axum::Router;
use crate::state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/api/auth", auth::router())
        .nest("/api/users", users::router())
        .nest("/api/posts", posts::router())
}
```

### Example: Route Module

```rust
// src/routes/users.rs
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use crate::{
    errors::AppError,
    models::user::{CreateUserRequest, User, ListParams},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(show).put(update).delete(destroy))
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<User>>, AppError> {
    let users = state.db.list_users(params).await?;
    Ok(Json(users))
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), AppError> {
    let user = state.db.create_user(payload).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn show(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, AppError> {
    let user = state.db.get_user(id).await?;
    Ok(Json(user))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<User>, AppError> {
    let user = state.db.update_user(id, payload).await?;
    Ok(Json(user))
}

async fn destroy(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<StatusCode, AppError> {
    state.db.delete_user(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

## Best Practices

1. **Use `with_state()` over `Extension`** - The `State` extractor is type-safe at compile time and provides better error messages than `Extension`, which fails at runtime if the type is missing.

2. **Keep locks short-lived** - When using `Arc<Mutex<T>>`, drop the lock before any `.await` point. Use scoping braces to make lock lifetimes explicit. Prefer `std::sync::Mutex` unless you must hold the lock across `.await`, in which case use `tokio::sync::Mutex`.

3. **Order extractors correctly** - Body-consuming extractors (`Json`, `String`, `Bytes`) must be the last parameter. Non-consuming extractors (`Path`, `Query`, `State`, `HeaderMap`) can appear in any order before them.

4. **Separate routes from handlers** - Define route tables in dedicated router functions and keep handler logic in separate modules. This makes the application easier to navigate and test.

5. **Use custom error types** - Implement `IntoResponse` for a unified `AppError` enum rather than returning raw `StatusCode` values. This ensures consistent error response shapes across all endpoints.

6. **Apply middleware selectively** - Use `.layer()` placement and router merging to apply middleware only where needed. Authentication middleware should not apply to public health check endpoints.

7. **Prefer `nest()` for API versioning** - Use `Router::nest("/api/v1", v1_routes())` to cleanly group versioned endpoints under a common prefix without repeating the prefix in every route.

8. **Use graceful shutdown in production** - Always configure `with_graceful_shutdown()` to handle SIGTERM and SIGINT. This ensures in-flight requests complete before the process exits.

9. **Bind to port 0 for tests and embedded servers** - Let the OS assign a free port and retrieve it via `listener.local_addr()`. This avoids port conflicts in parallel test execution and Tauri integrations.

10. **Compose middleware with `ServiceBuilder`** - When applying multiple layers, use `tower::ServiceBuilder` to compose them. Layers in `ServiceBuilder` execute top-to-bottom for requests, making the order easier to reason about.
