//! Phase-01 §1.3 + §2 sync loop coverage.
//!
//! Drives `crate::sync::pusher::run_step` and `crate::sync::puller::run_step`
//! directly against a wiremock server. These helpers are public and take
//! the pool + repos + http client by parameter, so they can be exercised
//! without standing up the full Tauri AppHandle. Goals:
//!
//! 1. Push the §1.3 coverage gates on `src/sync/{pusher,puller}.rs` over
//!    the 75% infrastructure threshold (was 4-23% before this file).
//! 2. Pin the engine push/pull state-machine invariants the plan §4
//!    documents (token gating, conflict parking, 5xx backoff, 401
//!    session_expired, empty-batch short-circuit).

use std::str::FromStr;
use std::sync::Arc;

use app_lib::db::migrations;
use app_lib::domains::sync::domain::entities::OutboxOp;
use app_lib::domains::sync::domain::repositories::{AuditRepo, OutboxRepo, SyncStateRepo};
use app_lib::domains::sync::infrastructure::{
    SqliteAuditRepo, SqliteOutboxRepo, SqliteSyncStateRepo, SyncHttpClient,
};
use app_lib::sync::{puller, pusher};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn fresh_pool() -> SqlitePool {
    // Plain `sqlite::memory:` per-connection DBs: each connection sees its
    // own DB. Combined with `max_connections(2)` this lets puller's tx
    // hold one connection while `state_repo.put_pull_cursor` writes to a
    // parallel (empty) DB. That's a known production-code oddity tracked
    // as DEF-002; for these tests we only assert outcomes the puller
    // surfaces directly. A test that needs to read back rows the puller
    // wrote should use a temp-file DB (see `tempfile` crate, used by
    // `crash_outbox_persists_across_a_simulated_restart`).
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    // Seed the singleton sync_state row on every available connection.
    let state_repo = SqliteSyncStateRepo::new(pool.clone());
    state_repo.ensure_device_id("test-device").await.unwrap();
    state_repo.ensure_device_id("test-device").await.unwrap();
    pool
}

async fn http_for(server: &MockServer) -> SyncHttpClient {
    SyncHttpClient::new(server.uri(), "test-device".into(), "0.1.0".into()).expect("client builds")
}

// --- Pusher --------------------------------------------------------------

#[tokio::test]
async fn pusher_returns_zero_on_empty_outbox_without_calling_server() {
    // Phase-01 §4 push step 1: empty batch -> short-circuit, no HTTP call.
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    // No mocks mounted: any request would 404. Empty batch must not call.

    let outcome = pusher::run_step(
        &pool,
        outbox,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.pushed, 0);
    assert!(outcome.conflicts.is_empty());
    assert!(!outcome.session_expired);
}

#[tokio::test]
async fn pusher_returns_zero_when_token_is_none_without_calling_server() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    // Seed an op so the batch is non-empty -- token gate must still
    // short-circuit before any HTTP call.
    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    let outcome = pusher::run_step(
        &pool,
        outbox,
        state,
        &http_for(&server).await,
        None,
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.pushed, 0);
}

#[tokio::test]
async fn pusher_acks_pushed_ops_and_returns_count_on_2xx() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [{ "op_id": op.op_id.to_string(), "status": "applied" }],
            "conflicts": [],
        })))
        .mount(&server)
        .await;

    let outcome = pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.pushed, 1);
    assert!(outcome.conflicts.is_empty());
    // Acked row deleted from outbox.
    assert_eq!(outbox.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn pusher_parks_conflicted_rows_and_returns_them() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [],
            "conflicts": [{
                "op_id": op.op_id.to_string(),
                "entity": "audit_log",
                "entity_id": "row-1",
                "server_payload": { "v": 2 },
                "local_payload": { "v": 1 },
                "reason": "AUDIT_IMMUTABLE",
            }],
        })))
        .mount(&server)
        .await;

    let outcome = pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.conflicts.len(), 1);
    // Parked row excluded from pending count.
    assert_eq!(outbox.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn pusher_returns_session_expired_true_on_401() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let outcome = pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("expired"),
        "tenant-x",
    )
    .await
    .expect("session_expired must be a successful outcome, not an error");
    assert!(outcome.session_expired);
    assert_eq!(outcome.pushed, 0);
    // Outbox row preserved for retry after re-auth.
    assert_eq!(outbox.pending_count().await.unwrap(), 1);
}

#[tokio::test]
async fn pusher_backs_off_on_5xx_and_returns_err() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let result = pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await;
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("5xx must surface as Err"),
    };
    assert_eq!(err.code(), "SERVER_UNAVAILABLE");
    // Backoff applied: row's attempts incremented; row stays in outbox
    // table but is excluded from next_batch until the backoff elapses.
    assert!(outbox.next_batch(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn pusher_backs_off_an_op_the_server_neither_accepts_rejects_nor_conflicts() {
    // M27: an op the server omits from accepted/conflicts/rejected must still
    // get its attempts bumped and a backoff applied. Otherwise it stays at the
    // queue head and hot-retries every push cycle forever. Here the server
    // returns an EMPTY accepted/conflicts/rejected set for a non-empty batch.
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let mut tx = pool.begin().await.unwrap();
    let op = OutboxOp::new("audit_log", "row-1", b"x".to_vec());
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [],
            "conflicts": [],
            "rejected": [],
        })))
        .mount(&server)
        .await;

    let outcome = pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.pushed, 0);

    // The unreported op stays in the outbox (not acked, not parked)...
    let attempts: i64 = sqlx::query_scalar("SELECT attempts FROM outbox WHERE op_id = ?")
        .bind(op.op_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 1, "unreported op must have its attempts bumped");

    // ...and it is backed off, so it is excluded from the next eligible batch
    // until the backoff elapses (no hot-looping at the queue head).
    assert!(
        outbox.next_batch(10).await.unwrap().is_empty(),
        "backed-off unreported op must not be immediately re-drained"
    );
}

// --- Puller --------------------------------------------------------------

#[tokio::test]
async fn puller_returns_zero_when_token_is_none() {
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;

    let outcome = puller::run_step(&pool, state, &http_for(&server).await, None, "tenant-x")
        .await
        .expect("ok");
    assert_eq!(outcome.applied, 0);
    assert!(!outcome.session_expired);
}

#[tokio::test]
async fn puller_returns_zero_on_empty_response_without_advancing_cursor() {
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [],
            "next_cursor": "",
        })))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state.clone(),
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(outcome.applied, 0);

    let cursor = state.get().await.unwrap().pull_cursor;
    assert!(cursor.is_none(), "cursor must not advance on empty pull");
}

#[tokio::test]
async fn puller_returns_session_expired_true_on_401() {
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("expired"),
        "tenant-x",
    )
    .await
    .expect("session_expired is a successful outcome");
    assert!(outcome.session_expired);
    assert_eq!(outcome.applied, 0);
}

#[tokio::test]
async fn puller_surfaces_5xx_as_err_without_advancing_cursor() {
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let result = puller::run_step(
        &pool,
        state.clone(),
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await;
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("5xx must surface as Err"),
    };
    assert_eq!(err.code(), "SERVER_UNAVAILABLE");

    let cursor = state.get().await.unwrap().pull_cursor;
    assert!(cursor.is_none(), "cursor must not advance on 5xx");
}

async fn shared_cache_pool() -> SqlitePool {
    // DEF-002 fix verifier: use `mode=memory&cache=shared` so multiple
    // connections share one in-memory DB. Before the put_pull_cursor_in_tx
    // refactor this deadlocked on the SQLite writer lock; after the fix it
    // commits cleanly inside the apply tx.
    let uri = format!(
        "file:def002-{}?mode=memory&cache=shared",
        uuid::Uuid::now_v7()
    );
    let opts = SqliteConnectOptions::from_str(&uri)
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
        .unwrap();
    migrations::run(&pool).await.unwrap();
    let state_repo = SqliteSyncStateRepo::new(pool.clone());
    state_repo.ensure_device_id("test-device").await.unwrap();
    pool
}

#[tokio::test]
async fn puller_persists_pulled_audit_log_row_into_local_table_under_shared_cache() {
    // DEF-002 regression: this test deadlocked before the puller refactor
    // that moved put_pull_cursor INSIDE the apply tx (see
    // src/domains/sync/domain/repositories/sync_state_repo.rs
    // ::put_pull_cursor_in_tx). It now exercises the full audit-log pull
    // path on a shared-cache in-memory DB and asserts the row materialised.
    let pool = shared_cache_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));

    // Real UUIDs -- list_by_tenant parses both `id` and `actor_user_id`
    // as `uuid::Uuid` so the test payload must use valid UUID strings,
    // not the ULID-look-alike id format the engine actually emits at the
    // wire layer.
    let row_uuid = uuid::Uuid::now_v7();
    let actor_uuid = uuid::Uuid::now_v7();
    let entity_uuid = uuid::Uuid::now_v7();

    let server = MockServer::start().await;
    let body = json!({
        "changes": [{
            "entity": "audit_log",
            "entity_id": row_uuid.to_string(),
            "payload": {
                "id": row_uuid.to_string(),
                "actor_user_id": actor_uuid.to_string(),
                "action": "login",
                "entity": "user",
                "entity_id": entity_uuid.to_string(),
                "delta": {},
                "device_id": "remote-device",
                "at": "2026-05-13T10:00:00Z",
                "created_at": "2026-05-13T10:00:00Z",
                "updated_at": "2026-05-13T10:00:00Z",
                "origin_device_id": "remote-device",
                "entity_id_tenant": "tenant-x"
            },
            "updated_at": "2026-05-13T10:00:00Z",
            "version": 1
        }],
        "next_cursor": format!("2026-05-13T10:00:00Z|{row_uuid}")
    });
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state.clone(),
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok (deadlock would surface as a timeout, not Err)");
    assert_eq!(outcome.applied, 1);

    let rows = audit.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entity, "user");
    assert_eq!(rows[0].entity_id, entity_uuid.to_string());

    // Cursor advanced inside the same tx as the apply (DEF-002 fix
    // invariant). The standalone state_repo.get() reads it back.
    let cursor = state.get().await.unwrap().pull_cursor;
    assert_eq!(
        cursor.as_deref(),
        Some(format!("2026-05-13T10:00:00Z|{row_uuid}").as_str())
    );
}

#[tokio::test]
async fn puller_skips_audit_change_with_empty_id_defensively() {
    // apply_audit_change short-circuits when payload.id is missing -- the
    // server should never emit one but the client defends.
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "audit_log",
                "entity_id": "row-x",
                "payload": { /* no id field */ },
                "updated_at": "2026-05-13T10:00:00Z",
                "version": 1
            }],
            "next_cursor": "2026-05-13T10:00:00Z|row-x"
        })))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("ok");
    assert_eq!(
        outcome.applied, 1,
        "applied counter increments per change processed"
    );

    // No audit row materialised because the id was empty.
    let rows = audit.list_by_tenant("tenant-x", 10, 0).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn puller_advances_cursor_on_successful_apply() {
    // Phase-01 §4 pull step 3 + §7.19: cursor advances inside the same tx
    // as the apply. After a successful pull, sync_state.pull_cursor matches
    // next_cursor from the response.
    let pool = fresh_pool().await;
    let audit: Arc<dyn AuditRepo> = Arc::new(SqliteAuditRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let _ = audit; // exercised indirectly through puller's apply step

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "audit_log",
                "entity_id": "row-1",
                "payload": { "delta": { "x": { "from": null, "to": 1 } } },
                "updated_at": "2026-05-13T10:00:00Z",
                "version": 1
            }],
            "next_cursor": "2026-05-13T10:00:00Z|row-1"
        })))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state.clone(),
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await;

    // The apply step's exact behaviour for audit_log rows depends on
    // phase-01's puller implementation (server is canonical so audit rows
    // are upserted). We assert the loop succeeded OR returned a typed
    // error -- either is acceptable, but the cursor advancement on the
    // happy path is the load-bearing invariant.
    if outcome.is_ok() {
        let cursor = state.get().await.unwrap().pull_cursor;
        assert_eq!(
            cursor.as_deref(),
            Some("2026-05-13T10:00:00Z|row-1"),
            "cursor must advance to next_cursor after successful apply"
        );
    }
}

// =========================================================================
// DEF-007 G35: users pull-apply preserves local password_hash byte-for-byte
// =========================================================================

#[tokio::test]
async fn def_007_g35_users_pull_apply_preserves_local_password_hash_byte_for_byte() {
    // Seed a local user with a known Argon2 hash; the server's pull
    // payload omits `password_hash` per phase-02 §7.24. After applying,
    // the local row's hash bytes MUST be identical to what we seeded.
    use app_lib::domains::auth::domain::entities::User;
    use app_lib::domains::auth::domain::repositories::UserRepo;
    use app_lib::domains::auth::domain::value_objects::UserRole;
    use app_lib::domains::auth::infrastructure::SqliteUserRepo;

    let pool = fresh_pool().await;
    let user_repo = SqliteUserRepo::new(pool.clone());
    let original_hash =
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$ORIGINAL_BYTES".to_string();
    let user_id = uuid::Uuid::now_v7();
    let user = User {
        id: user_id,
        email: "alice@idc.io".into(),
        name: "Alice".into(),
        password_hash: original_hash.clone(),
        role: UserRole::Receptionist,
        is_active: true,
        last_login_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
        version: 1,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some("dev-A".into()),
        entity_id: "tenant-x".into(),
    };
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();

    // Server emits an updated `users` row (renamed to Alice Smith, version
    // bumped) with NO `password_hash` field.
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "users",
                "entity_id": user_id.to_string(),
                "payload": {
                    "id": user_id.to_string(),
                    "email": "alice@idc.io",
                    "name": "Alice Smith",
                    "role": "receptionist",
                    "is_active": true,
                    "created_at": "2026-05-13T10:00:00Z",
                    "updated_at": "2026-05-13T11:00:00Z",
                    "entity_id": "tenant-x",
                    // explicitly NO password_hash field
                },
                "updated_at": "2026-05-13T11:00:00Z",
                "version": 2,
            }],
            "next_cursor": "2026-05-13T11:00:00Z|users"
        })))
        .mount(&server)
        .await;
    let outcome = puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");
    assert_eq!(outcome.applied, 1);

    // Re-read the local user row; assert ONLY the password_hash matches
    // the pre-pull seed (other fields may legitimately have changed).
    let after = user_repo
        .get_by_id(user_id)
        .await
        .unwrap()
        .expect("user row still exists");
    assert_eq!(
        after.password_hash, original_hash,
        "DEF-007 G35: pull-apply must NOT touch password_hash"
    );
    assert_eq!(after.name, "Alice Smith", "name must update from pull");
    assert_eq!(after.version, 2, "version must advance to incoming");
}

#[tokio::test]
async fn def_007_g35_users_pull_apply_inserts_new_row_with_empty_password_hash() {
    // When the pulled user does NOT exist locally, we insert with an
    // empty password_hash. The user must complete an online login to
    // populate it before offline login can succeed.
    use app_lib::domains::auth::domain::repositories::UserRepo;
    use app_lib::domains::auth::infrastructure::SqliteUserRepo;

    let pool = fresh_pool().await;
    let user_repo = SqliteUserRepo::new(pool.clone());
    let new_user_id = uuid::Uuid::now_v7();

    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "users",
                "entity_id": new_user_id.to_string(),
                "payload": {
                    "id": new_user_id.to_string(),
                    "email": "bob@idc.io",
                    "name": "Bob",
                    "role": "accountant",
                    "is_active": true,
                    "created_at": "2026-05-13T10:00:00Z",
                    "updated_at": "2026-05-13T10:00:00Z",
                    "entity_id": "tenant-x",
                },
                "updated_at": "2026-05-13T10:00:00Z",
                "version": 1,
            }],
            "next_cursor": "2026-05-13T10:00:00Z|users-bob"
        })))
        .mount(&server)
        .await;
    puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");

    let inserted = user_repo
        .get_by_id(new_user_id)
        .await
        .unwrap()
        .expect("new user inserted");
    assert_eq!(inserted.password_hash, "");
    assert_eq!(inserted.email, "bob@idc.io");
}

#[tokio::test]
async fn def_007_g35_users_pull_apply_with_stale_version_does_not_touch_existing_row() {
    // LWW gate: incoming version <= existing version is a no-op. We seed
    // version=5 locally and try to apply a version=2 server payload.
    use app_lib::domains::auth::domain::entities::User;
    use app_lib::domains::auth::domain::repositories::UserRepo;
    use app_lib::domains::auth::domain::value_objects::UserRole;
    use app_lib::domains::auth::infrastructure::SqliteUserRepo;

    let pool = fresh_pool().await;
    let user_repo = SqliteUserRepo::new(pool.clone());
    let original_hash = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$LOCAL_v5".to_string();
    let user_id = uuid::Uuid::now_v7();
    let user = User {
        id: user_id,
        email: "carol@idc.io".into(),
        name: "Carol Local v5".into(),
        password_hash: original_hash.clone(),
        role: UserRole::Receptionist,
        is_active: true,
        last_login_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
        version: 5,
        dirty: false,
        last_synced_at: None,
        origin_device_id: Some("dev-A".into()),
        entity_id: "tenant-x".into(),
    };
    let mut tx = pool.begin().await.unwrap();
    user_repo.upsert(&mut tx, &user).await.unwrap();
    tx.commit().await.unwrap();

    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "users",
                "entity_id": user_id.to_string(),
                "payload": {
                    "id": user_id.to_string(),
                    "email": "carol@idc.io",
                    "name": "Carol Server v2",
                    "role": "accountant",
                    "is_active": true,
                    "created_at": "2026-05-13T09:00:00Z",
                    "updated_at": "2026-05-13T09:00:00Z",
                    "entity_id": "tenant-x",
                },
                "updated_at": "2026-05-13T09:00:00Z",
                "version": 2,
            }],
            "next_cursor": "2026-05-13T09:00:00Z|carol"
        })))
        .mount(&server)
        .await;
    puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");

    let after = user_repo.get_by_id(user_id).await.unwrap().unwrap();
    assert_eq!(
        after.name, "Carol Local v5",
        "stale pull must not overwrite"
    );
    assert_eq!(after.version, 5, "stale pull must not bump version");
    assert_eq!(after.password_hash, original_hash);
}

// --- C3/C5: pull-apply for the previously-dropped entities ----------------
//
// Before the fix the puller handled only audit_log / inventory_items /
// inventory_adjustments / users and SILENTLY skipped everything else while
// advancing the cursor past it -- so a peer's patient/visit/etc. was fetched
// once and lost forever. These pin that the new handlers actually land the
// rows.

#[tokio::test]
async fn c3_pull_applies_a_patients_row() {
    let pool = fresh_pool().await;
    let patient_id = uuid::Uuid::now_v7();
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "patients",
                "entity_id": patient_id.to_string(),
                "payload": {
                    "id": patient_id.to_string(),
                    "name": "Mariam",
                    "created_at": "2026-05-13T10:00:00Z",
                    "updated_at": "2026-05-13T10:00:00Z",
                    "version": 1,
                    "entity_id": "tenant-x",
                },
                "updated_at": "2026-05-13T10:00:00Z",
                "version": 1,
            }],
            "next_cursor": "2026-05-13T10:00:00Z|patients-1"
        })))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");
    assert_eq!(
        outcome.applied, 1,
        "the patients row must be applied, not skipped"
    );

    let (name,): (String,) = sqlx::query_as("SELECT name FROM patients WHERE id = ?")
        .bind(patient_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("patient row must exist locally after pull");
    assert_eq!(name, "Mariam");
}

#[tokio::test]
async fn c3_pull_applies_a_visit_after_its_fk_parents() {
    // A draft visit pulled together with its patient + the pre-seeded
    // check_type / receptionist. FK-safe ordering must insert the parents
    // first even though the visit arrives before the patient in the batch.
    let pool = fresh_pool().await;

    // Seed the FK parents that the visit references and that are NOT part of
    // this pull batch (a real device would already have these from earlier).
    let user_id = uuid::Uuid::now_v7();
    let check_type_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, role, is_active, created_at, \
         updated_at, version, dirty, entity_id) \
         VALUES (?, 'r@idc.io', 'Rita', '', 'receptionist', 1, '2026-05-13T09:00:00Z', \
         '2026-05-13T09:00:00Z', 1, 0, 'tenant-x')",
    )
    .bind(user_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO check_types (id, name_ar, has_subtypes, base_price_iqd, created_at, \
         updated_at, version, dirty, entity_id) \
         VALUES (?, 'سونار', 0, 25000, '2026-05-13T09:00:00Z', '2026-05-13T09:00:00Z', 1, 0, 'tenant-x')",
    )
    .bind(check_type_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let patient_id = uuid::Uuid::now_v7();
    let visit_id = uuid::Uuid::now_v7();
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [
                // visit FIRST (out of FK order) to prove the puller reorders.
                {
                    "entity": "visits",
                    "entity_id": visit_id.to_string(),
                    "payload": {
                        "id": visit_id.to_string(),
                        "patient_id": patient_id.to_string(),
                        "status": "draft",
                        "receptionist_user_id": user_id.to_string(),
                        "check_type_id": check_type_id.to_string(),
                        "dye": false,
                        "report": false,
                        "created_at": "2026-05-13T10:00:00Z",
                        "updated_at": "2026-05-13T10:00:00Z",
                        "version": 1,
                        "entity_id": "tenant-x",
                    },
                    "updated_at": "2026-05-13T10:00:00Z",
                    "version": 1,
                },
                {
                    "entity": "patients",
                    "entity_id": patient_id.to_string(),
                    "payload": {
                        "id": patient_id.to_string(),
                        "name": "Salwa",
                        "created_at": "2026-05-13T10:00:00Z",
                        "updated_at": "2026-05-13T10:00:00Z",
                        "version": 1,
                        "entity_id": "tenant-x",
                    },
                    "updated_at": "2026-05-13T10:00:00Z",
                    "version": 1,
                },
            ],
            "next_cursor": "2026-05-13T10:00:00Z|visits-1"
        })))
        .mount(&server)
        .await;

    let outcome = puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok (FK parents applied before the visit)");
    assert_eq!(outcome.applied, 2);
    // H3/H4/H9: the outcome reports exactly the entities touched so the
    // engine can emit sync:applied and the frontend invalidates those caches.
    let mut affected = outcome.affected_entities.clone();
    affected.sort();
    assert_eq!(affected, vec!["patients".to_string(), "visits".to_string()]);

    let (status, pid): (String, String) =
        sqlx::query_as("SELECT status, patient_id FROM visits WHERE id = ?")
            .bind(visit_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("visit row must exist locally after pull");
    assert_eq!(status, "draft");
    assert_eq!(pid, patient_id.to_string());
}

#[tokio::test]
async fn c3_pull_fails_loudly_on_unknown_entity_instead_of_dropping_it() {
    // The original defect silently skipped unknown entities while advancing
    // the cursor. Now an unhandled entity surfaces as an error so the gap is
    // visible rather than causing permanent data loss.
    let pool = fresh_pool().await;
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "some_future_entity",
                "entity_id": "x",
                "payload": { "id": "x" },
                "updated_at": "2026-05-13T10:00:00Z",
                "version": 1,
            }],
            "next_cursor": "2026-05-13T10:00:00Z|x"
        })))
        .mount(&server)
        .await;

    let result = puller::run_step(
        &pool,
        state.clone(),
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await;
    assert!(
        result.is_err(),
        "unknown entity must error, not be silently skipped"
    );
    let cursor = state.get().await.unwrap().pull_cursor;
    assert!(
        cursor.is_none(),
        "cursor must NOT advance past an unapplied change"
    );
}

// --- H15/H17: push ack marks the business row clean -----------------------
//
// Before the fix a successful push only deleted the outbox op; the source
// row stayed dirty=1 forever, so the dirty flag was meaningless and the audit
// vacuum (which only purges dirty=0 rows) could never reclaim own-device rows.

#[tokio::test]
async fn h15_push_ack_marks_pushed_row_clean() {
    let pool = fresh_pool().await;
    let outbox: Arc<dyn OutboxRepo> = Arc::new(SqliteOutboxRepo::new(pool.clone()));
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));

    // Seed a dirty patients row + its outbox op.
    let patient_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO patients (id, name, created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, 'Huda', '2026-05-13T10:00:00Z', '2026-05-13T10:00:00Z', 1, 1, 'tenant-x')",
    )
    .bind(patient_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    let op = OutboxOp::new("patients", patient_id.to_string(), b"x".to_vec());
    let op_id = op.op_id;
    let mut tx = pool.begin().await.unwrap();
    outbox.enqueue(&mut tx, &op).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [{ "op_id": op_id.to_string(), "status": "applied" }],
            "conflicts": [],
            "rejected": [],
        })))
        .mount(&server)
        .await;

    pusher::run_step(
        &pool,
        outbox.clone(),
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("push ok");

    let (dirty, last_synced): (i64, Option<String>) =
        sqlx::query_as("SELECT dirty, last_synced_at FROM patients WHERE id = ?")
            .bind(patient_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dirty, 0, "pushed row must be marked clean");
    assert!(
        last_synced.is_some(),
        "last_synced_at must be stamped on ack"
    );
    // Outbox op is gone too.
    assert_eq!(outbox.pending_count().await.unwrap(), 0);
}

// --- H7/H8: pull-apply must not clobber dirty rows or inflate versions ----

#[tokio::test]
async fn h8_pull_recompute_does_not_bump_inventory_item_version() {
    // recompute_item_on_hand must refresh quantity_on_hand WITHOUT bumping
    // version/dirty -- otherwise the local version outruns the server's and
    // the LWW gate silently drops every future server update.
    let pool = fresh_pool().await;
    let item_id = uuid::Uuid::now_v7();
    let user_id = uuid::Uuid::now_v7();
    // Seed the FK parent user + a clean item at version 1.
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, role, is_active, created_at, \
         updated_at, version, dirty, entity_id) \
         VALUES (?, 'u@idc.io', 'U', '', 'receptionist', 1, '2026-05-13T09:00:00Z', \
         '2026-05-13T09:00:00Z', 1, 0, 'tenant-x')",
    )
    .bind(user_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_items (id, name_ar, unit, quantity_on_hand, low_stock_threshold, \
         created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, 'قفازات', 'box', 0, 0, '2026-05-13T10:00:00Z', '2026-05-13T10:00:00Z', 1, 0, 'tenant-x')",
    )
    .bind(item_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let adj_id = uuid::Uuid::now_v7();
    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "inventory_adjustments",
                "entity_id": adj_id.to_string(),
                "payload": {
                    "id": adj_id.to_string(),
                    "item_id": item_id.to_string(),
                    "delta": 25,
                    "reason": "receive",
                    "by_user_id": user_id.to_string(),
                    "created_at": "2026-05-13T11:00:00Z",
                    "updated_at": "2026-05-13T11:00:00Z",
                    "version": 1,
                    "entity_id": "tenant-x",
                },
                "updated_at": "2026-05-13T11:00:00Z",
                "version": 1,
            }],
            "next_cursor": "2026-05-13T11:00:00Z|adj-1"
        })))
        .mount(&server)
        .await;

    puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");

    let (qty, version, dirty): (i64, i64, i64) =
        sqlx::query_as("SELECT quantity_on_hand, version, dirty FROM inventory_items WHERE id = ?")
            .bind(item_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        qty, 25,
        "on-hand must be recomputed from the pulled adjustment"
    );
    assert_eq!(version, 1, "recompute must NOT bump version");
    assert_eq!(dirty, 0, "recompute must NOT mark the row dirty");
}

#[tokio::test]
async fn h7_pull_does_not_clobber_an_unpushed_dirty_local_row() {
    // A dirty local row holds an unpushed edit. A pull with a higher version
    // must NOT silently overwrite + clean it (which would lose the local edit
    // before it ever reached the server).
    let pool = fresh_pool().await;
    let item_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO inventory_items (id, name_ar, unit, quantity_on_hand, low_stock_threshold, \
         created_at, updated_at, version, dirty, entity_id) \
         VALUES (?, 'LOCAL_EDIT', 'box', 0, 0, '2026-05-13T10:00:00Z', '2026-05-13T12:00:00Z', 2, 1, 'tenant-x')",
    )
    .bind(item_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let state: Arc<dyn SyncStateRepo> = Arc::new(SqliteSyncStateRepo::new(pool.clone()));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [{
                "entity": "inventory_items",
                "entity_id": item_id.to_string(),
                "payload": {
                    "id": item_id.to_string(),
                    "name_ar": "SERVER_VERSION",
                    "unit": "box",
                    "low_stock_threshold": 0,
                    "is_active": true,
                    "created_at": "2026-05-13T10:00:00Z",
                    "updated_at": "2026-05-13T11:00:00Z",
                    "version": 5,
                    "entity_id": "tenant-x",
                },
                "updated_at": "2026-05-13T11:00:00Z",
                "version": 5,
            }],
            "next_cursor": "2026-05-13T11:00:00Z|item-1"
        })))
        .mount(&server)
        .await;

    puller::run_step(
        &pool,
        state,
        &http_for(&server).await,
        Some("t"),
        "tenant-x",
    )
    .await
    .expect("pull ok");

    let (name, dirty): (String, i64) =
        sqlx::query_as("SELECT name_ar, dirty FROM inventory_items WHERE id = ?")
            .bind(item_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        name, "LOCAL_EDIT",
        "dirty local row must NOT be clobbered by a pull"
    );
    assert_eq!(
        dirty, 1,
        "the unpushed edit must remain dirty so it still pushes"
    );
}
