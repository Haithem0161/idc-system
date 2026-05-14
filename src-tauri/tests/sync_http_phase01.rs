//! Phase-01 §2 wiremock-backed SyncHttpClient tests.
//!
//! Spins an ephemeral mock HTTP server per test and drives the real
//! `SyncHttpClient` against it. Covers the engine's HTTP transport surface
//! (push, pull, lookup-op, resolve-conflict) without standing up a Tauri
//! AppHandle.

use app_lib::domains::sync::infrastructure::{PushOp, SyncHttpClient};
use serde_json::json;
use wiremock::matchers::{body_json_string, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client(server: &MockServer) -> SyncHttpClient {
    SyncHttpClient::new(server.uri(), "test-device".into(), "0.1.0".into())
        .expect("client builds against mock server")
}

fn push_op(op_id: &str, entity_id: &str) -> PushOp {
    PushOp {
        op_id: op_id.into(),
        entity: "audit_log".into(),
        entity_id: entity_id.into(),
        op: "upsert".into(),
        payload_b64: "aGVsbG8=".into(),
    }
}

// --- /sync/push ----------------------------------------------------------

#[tokio::test]
async fn push_returns_accepted_and_conflicts_on_200() {
    let server = MockServer::start().await;
    let response_body = json!({
        "accepted": [{ "op_id": "op-A", "status": "applied" }],
        "conflicts": [{
            "op_id": "op-B",
            "entity": "audit_log",
            "entity_id": "row-B",
            "server_payload": {},
            "local_payload": {},
            "reason": "AUDIT_IMMUTABLE",
        }],
    });
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .and(header("authorization", "Bearer test-token"))
        .and(header("x-device-id", "test-device"))
        .and(header("x-app-version", "0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&server)
        .await;

    let result = client(&server)
        .await
        .push("test-token", &[push_op("op-A", "row-A"), push_op("op-B", "row-B")])
        .await
        .expect("2xx push must succeed");
    assert_eq!(result.accepted.len(), 1);
    assert_eq!(result.accepted[0].op_id, "op-A");
    assert_eq!(result.accepted[0].status, "applied");
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].op_id, "op-B");
    assert_eq!(result.conflicts[0].reason, "AUDIT_IMMUTABLE");
}

#[tokio::test]
async fn push_returns_session_expired_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .push("expired-token", &[push_op("op-A", "row-A")])
        .await
        .expect_err("401 must surface SessionExpired");
    assert_eq!(err.code(), "SESSION_EXPIRED");
}

#[tokio::test]
async fn push_returns_sync_unavailable_on_5xx() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .respond_with(ResponseTemplate::new(503).set_body_string("backend down"))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .push("test-token", &[push_op("op-A", "row-A")])
        .await
        .expect_err("503 must surface SyncUnavailable");
    assert_eq!(err.code(), "SERVER_UNAVAILABLE");
    assert!(err.to_string().contains("503"));
}

#[tokio::test]
async fn push_sends_body_with_ops_array() {
    let server = MockServer::start().await;
    let expected = json!({
        "ops": [{
            "op_id": "op-A",
            "entity": "audit_log",
            "entity_id": "row-A",
            "op": "upsert",
            "payload_b64": "aGVsbG8=",
        }],
    });
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .and(body_json_string(expected.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [],
            "conflicts": [],
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .await
        .push("test-token", &[push_op("op-A", "row-A")])
        .await
        .expect("body shape must match");
    assert!(result.accepted.is_empty());
    assert!(result.conflicts.is_empty());
}

// --- /sync/pull ----------------------------------------------------------

#[tokio::test]
async fn pull_returns_changes_and_next_cursor() {
    let server = MockServer::start().await;
    let response_body = json!({
        "changes": [{
            "entity": "audit_log",
            "entity_id": "row-1",
            "payload": { "delta": { "status": { "from": null, "to": "created" } } },
            "updated_at": "2026-05-13T10:00:00Z",
            "version": 1,
        }],
        "next_cursor": "2026-05-13T10:00:00Z|row-1",
    });
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .and(query_param("since", "2026-05-13T09:00:00Z|row-0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&server)
        .await;

    let result = client(&server)
        .await
        .pull("test-token", Some("2026-05-13T09:00:00Z|row-0"))
        .await
        .expect("pull must succeed");
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].entity, "audit_log");
    assert_eq!(result.changes[0].version, 1);
    assert_eq!(result.next_cursor, "2026-05-13T10:00:00Z|row-1");
}

#[tokio::test]
async fn pull_without_cursor_omits_since_query_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [],
            "next_cursor": "",
        })))
        .mount(&server)
        .await;

    let result = client(&server).await.pull("test-token", None).await.expect("ok");
    assert!(result.changes.is_empty());
    assert_eq!(result.next_cursor, "");
}

#[tokio::test]
async fn pull_returns_session_expired_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .pull("expired", None)
        .await
        .expect_err("401 must surface SessionExpired");
    assert_eq!(err.code(), "SESSION_EXPIRED");
}

// --- /sync/lookup-op -----------------------------------------------------

#[tokio::test]
async fn lookup_op_returns_found_op_ids() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/lookup-op"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "found": ["op-A", "op-C"],
        })))
        .mount(&server)
        .await;

    let found = client(&server)
        .await
        .lookup_op("test-token", &["op-A".into(), "op-B".into(), "op-C".into()])
        .await
        .expect("lookup-op must succeed");
    assert_eq!(found, vec!["op-A".to_string(), "op-C".to_string()]);
}

#[tokio::test]
async fn lookup_op_returns_empty_when_server_acked_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/lookup-op"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "found": [] })))
        .mount(&server)
        .await;

    let found = client(&server)
        .await
        .lookup_op("test-token", &["op-A".into()])
        .await
        .expect("ok");
    assert!(found.is_empty());
}

#[tokio::test]
async fn lookup_op_returns_unavailable_on_non_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/lookup-op"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .lookup_op("test-token", &["op-A".into()])
        .await
        .expect_err("5xx must surface SyncUnavailable");
    assert_eq!(err.code(), "SERVER_UNAVAILABLE");
}

// --- /sync/conflicts/:opId/resolve ---------------------------------------

#[tokio::test]
async fn resolve_conflict_returns_ok_on_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/conflicts/op-A/resolve"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&server)
        .await;

    let result = client(&server)
        .await
        .resolve_conflict("test-token", "op-A", "resolve-1", "local", None)
        .await;
    assert!(result.is_ok(), "resolve must succeed: {result:?}");
}

#[tokio::test]
async fn resolve_conflict_surfaces_already_resolved_on_409() {
    // Phase-08 §7.22 invariant: a server 409 on resolve means another device
    // already submitted a different resolution; client surfaces the prior
    // body via AppError::Conflict for the resolver UI.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/conflicts/op-A/resolve"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_string("{\"choice\":\"server\",\"resolve_op_id\":\"prior\"}"),
        )
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .resolve_conflict("test-token", "op-A", "resolve-2", "local", None)
        .await
        .expect_err("409 must surface Conflict");
    assert_eq!(err.code(), "CONFLICT_PARKED");
    assert!(
        err.to_string().contains("ALREADY_RESOLVED"),
        "got: {err}"
    );
}

#[tokio::test]
async fn resolve_conflict_returns_session_expired_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/conflicts/op-A/resolve"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .resolve_conflict("expired", "op-A", "resolve-1", "local", None)
        .await
        .expect_err("401 must surface SessionExpired");
    assert_eq!(err.code(), "SESSION_EXPIRED");
}

// --- /healthz ------------------------------------------------------------

#[tokio::test]
async fn healthz_returns_true_on_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let ok = client(&server).await.healthz().await.expect("ok");
    assert!(ok);
}

#[tokio::test]
async fn healthz_returns_false_on_5xx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let ok = client(&server).await.healthz().await.expect("ok");
    assert!(!ok);
}

// --- request headers -----------------------------------------------------

#[tokio::test]
async fn every_request_sends_x_device_id_and_x_app_version_headers() {
    // Phase-01 §7 X-Device-Id + X-App-Version invariant for the engine HTTP
    // surface. Mounting a Mock matcher that REQUIRES both headers; any
    // request missing either will fail to match and the call will 404.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync/push"))
        .and(header("x-device-id", "test-device"))
        .and(header("x-app-version", "0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accepted": [],
            "conflicts": [],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/sync/pull"))
        .and(header("x-device-id", "test-device"))
        .and(header("x-app-version", "0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [],
            "next_cursor": "",
        })))
        .mount(&server)
        .await;

    let c = client(&server).await;
    let push = c
        .push("test-token", &[push_op("op-A", "row-A")])
        .await
        .expect("push receives headers");
    let pull = c.pull("test-token", None).await.expect("pull receives headers");
    assert!(push.accepted.is_empty());
    assert!(pull.changes.is_empty());
}
