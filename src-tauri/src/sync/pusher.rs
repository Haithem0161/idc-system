//! Push loop: drains outbox -> POST /sync/push -> marks rows clean on success,
//! invokes conflict resolver on 409, exponential backoff on 5xx.
