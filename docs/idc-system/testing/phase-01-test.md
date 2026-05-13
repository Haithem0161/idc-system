# Phase 01: Foundation & Sync Plumbing -- Test Plan

**Proves:** The offline-first plumbing the entire rest of the system relies on actually works end-to-end: outbox + sync engine (push / pull / conflict envelope) drain a backlog into a Postgres-backed server with at-least-once + idempotent-by-`op_id` guarantees; `AuditWriter::with_audit` writes audit-first inside a single SQLite transaction such that any business-write failure leaves zero audit rows AND zero outbox rows; pull-time delete-vs-edit reconciliation never resurrects soft-deleted rows; `audit_log` push enforces immutability server-side; `ProcessedOp` idempotency cache makes replays no-ops with byte-identical responses; the app shell (sidebar, header, status bar, `<SyncPill>`, `<RtlBoundary>`) renders in both `dir=ltr` and `dir=rtl` and the sync pill cycles its five states from real engine events; the JWT public key is fetched-and-pinned at boot; the `outbox.parked` flag stops retry storms when the server returns a conflict envelope; the v1-only `outbox.op = 'upsert'` invariant (no hard deletes) holds on both surfaces.

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server). This is the foundational phase -- every later phase test plan depends on the invariants verified here.

**Dependencies (other test plans):** None. This is the root of the dependency graph; every other phase test plan transitively depends on this one.

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_outbox_op_audit_log, make_audit_entry, make_sync_state, make_synthetic_audit_action}` (NEW -- the support module itself is bootstrapped in this phase).
- Factories (TS): `src/test-utils/factories.ts::{makeSyncStatusSnapshot, makeConflictRecord, makeDeviceContext}` (NEW -- module bootstrapped here).
- Factories (Sync server): `sync-server/test/support/factories.ts::{makeAuditLogPushRow, makeProcessedOp, makeConflictParked, makeJwtToken, makeRs256KeyPair}` (NEW -- module bootstrapped here; consumed by every later server test plan).
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` is NOT loaded by phase-01 tests directly (no domain entities exist yet); the fixture is constructed against the foundation but consumed by later phases. Phase-01 owns the SQL stub that creates the fixture's `audit_log` rows and ensures the migration runner accepts a populated table.

**Tool prerequisites:**
- Rust: `cargo` (in use), `cargo-llvm-cov` (NEW: `cargo install cargo-llvm-cov` -- first phase to introduce Rust coverage gating; ALL later plans inherit this).
- Frontend: `vitest` + `@testing-library/react` + `@testing-library/jest-dom` + `jsdom` + `@vitest/coverage-v8` (NEW: `pnpm add -D ...` -- first phase to introduce frontend tests; all later plans inherit).
- E2E: `webdriverio` + `@wdio/cli` + `@wdio/local-runner` + `@wdio/spec-reporter` + `@wdio/mocha-framework` + `tauri-driver` binary (NEW: `pnpm add -D ...` + per-OS driver install).
- Contract: `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser` (NEW: contract harness for §3.1; reused by every later plan).
- Sync server: `node --test` + `c8` + `ts-node` already present; `testcontainers` (NEW: `pnpm add -D testcontainers` -- spins ephemeral Postgres for `/sync/*` integration tests against a real Prisma DB).
- Network failure injection: `wiremock` (NEW Rust dev-dep, `cargo add --dev wiremock`) -- used by §2.1 to assert 5xx backoff, 401 refresh, partial-batch results.
- All tooling installed during phase-01-test execution becomes the inherited toolchain for phases 02-09.

**Out of scope (cross-cutting tests):**
- Refresh-token replay attacks -- owned by `security.md` (the JWT-tamper / replay row matrix). Phase-01 verifies the engine's 401-refresh-retry mechanic itself; the security primitives around refresh are cross-cutting.
- 3xN conflict matrix exhaustively -- the `additive-only` cell for `audit_log` is exercised here; the cross-product against every later entity lives in `sync-conflicts.md`.
- Page-by-page i18n / RTL snapshots for the app shell -- phase-01 asserts the `<RtlBoundary>` / `<SyncPill>` / `<LanguageToggle>` / `<StatusBar>` invariants in both directions; the full visual page-by-page sweep is in `i18n-rtl.md` once phases 02-08 surfaces exist.
- 8-hour soak + 12-month scale runs aggregated -- owned by `performance-soak.md` (which is bootstrapped in phase-08 §5). Phase-01 verifies the per-op SLOs the soak harness aggregates.

**Cross-phase commands:** none -- phase-01 owns every command it registers (`sync::status`, `sync::trigger_push`, `sync::trigger_pull`, `sync::list_conflicts`, `sync::resolve_conflict`, `sync::outbox_count` from §7.4, `device::info`, `config::set_sync_server_url` from §7.22, `config::get_sync_server_url` from §7.22). Phase-08 test wires the resolver UI on top of `sync::list_conflicts` / `sync::resolve_conflict` but ownership remains here.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`OutboxOp` entity (`src-tauri/src/domains/sync/domain/entities/outbox_op.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `OutboxOp::try_new` | `produces_op_with_uuid_v7_id_and_attempts_zero` | Defaults: UUID v7 `op_id`, `attempts=0`, `next_attempt_at == created_at`, `last_error == None`, `parked == false` (per §7.17). |
| `OutboxOp::try_new` | `rejects_empty_entity_name` | `entity = ""` -> `Err(SyncError::EntityNameEmpty)`. |
| `OutboxOp::try_new` | `rejects_empty_entity_id` | `entity_id = ""` -> `Err`. |
| `OutboxOp::try_new` | `accepts_only_upsert_op_in_v1` | `op = OutboxAction::Upsert` succeeds; `op = OutboxAction::Delete` is gated behind a `#[cfg(feature = "horizon2-pii-purge")]` flag and unreachable in v1 (per §7.15). A direct construction with `Delete` in the default feature set fails to compile (verified by `trybuild`). |
| `OutboxOp::try_new` | `payload_max_size_8mb` | A 9MB payload returns `Err(SyncError::PayloadTooLarge { actual, limit })`. Limit is the sync envelope max per phase-01 §3 Server `SyncPushBodySchema`. |
| `OutboxOp::compute_backoff` | `exponential_capped_at_60_minutes` | `attempts=0 -> 1s`, `1 -> 2s`, `2 -> 4s`, ..., `10 -> 60min` (saturated). Per §4 SyncEngine push-step 5. |
| `OutboxOp::compute_backoff` | `attempts_above_threshold_stops_retrying` | `attempts >= 10` -> the partial index filter excludes the row; the test asserts the SQL predicate by constructing the WHERE clause string. |

**`AuditEntry` entity (`src-tauri/src/domains/sync/domain/entities/audit_entry.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `AuditEntry::try_new` | `produces_audit_with_uuid_v7_actor_action_at` | Defaults; `delta` is `serde_json::Value::Object` (never `Null`); `device_id` echoed from `AppState`. |
| `AuditEntry::try_new` | `delta_must_be_non_null_object` | `delta = Value::Null` -> `Err(AuditError::DeltaNotObject)`. |
| `AuditEntry::try_new` | `delta_object_must_have_at_least_one_field_when_action_is_update` | `action = Update` + empty delta -> `Err(AuditError::UpdateDeltaEmpty)`. `Create` and `SoftDelete` accept empty objects. |
| `AuditEntry::try_new` | `rejects_unknown_action_at_compile_time` | `AuditAction` is an enum; the Rust compiler enforces; the test uses `trybuild` to assert a stringly-typed action fails to compile. |
| `AuditEntry::try_new` | `accepts_all_14_phase01_to_phase07_actions_in_enum` | Per §7.36 final enum: `create | update | soft_delete | lock | void | discard | clock_in | clock_out | password_change | login | logout | conflict_resolve | vacuum | daily_close_run`. Every variant round-trips through `serde::Serialize` -> `Deserialize` to a canonical string. |
| `AuditEntry::try_new` | `compute_delta_omits_identical_fields` | `before = {a:1,b:2}`, `after = {a:1,b:3}` -> `delta = {b: {from: 2, to: 3}}`. The `a` key is dropped. Per §4 `AuditWriter::with_audit` step 4. |
| `AuditEntry::try_new` | `compute_delta_handles_added_and_removed_fields` | `before = {a:1}`, `after = {a:1, b:2}` -> `delta = {b: {from: null, to: 2}}`. `before = {a:1, b:2}`, `after = {a:1}` -> `delta = {b: {from: 2, to: null}}`. |
| `AuditEntry::try_new` | `redacts_password_and_token_fields_in_delta` | `before/after` containing `password_hash` or `password` or `token` or `hash` field -> the rendered `delta` has the redacted value `"[REDACTED]"`, not the raw bytes. Mirror of the `RedactionLayer` from §7.14; this is a domain-layer invariant, not just a `tracing` filter. |

**`SyncStatus` value object (`src-tauri/src/domains/sync/domain/value_objects/sync_status.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `SyncStatus::transition` | `legal_transitions_match_state_machine` | The 5-state machine: `Idle <-> Pushing`, `Idle <-> Pulling`, `Idle <-> Offline`, `Idle <-> Error`, and the path back from `Error` via resolver. Exactly the legal pairs return `Ok`; the rest return `Err(IllegalTransition)`. |
| `SyncStatus::transition` | `error_to_offline_allowed_when_network_drops` | `Error -> Offline` is legal (a parked-conflict error transitions to offline if the network drops mid-resolution). |
| `SyncStatus::display` | `i18n_key_per_state_matches_phase_30_inventory` | Each state maps to the correct `errors:sync.*` or `sync.pill.*` key per phase-01 §7.30 inventory. |

**`AuditWriter::with_audit` (`src-tauri/src/domains/sync/service/audit_writer.rs`)** -- the most-important pure-logic surface in the phase (the audit-first invariant rides on this).

| Module | Test | Asserts |
|-|-|-|
| `AuditWriter::compute_step_order` | `audit_first_then_business_then_outbox` | Per §7.7 restructured ordering: returns the sequence `[InsertAudit, InvokeBusinessWrites, EnqueueOutbox]`. The test asserts the literal step order (an enum sequence), not just that all three happen. |
| `AuditWriter::compute_delta_with_redaction` | `composes_redactor_with_field_diff` | Given a before/after with a `password_hash` field that changed, the rendered delta has the `password_hash` entry as `{from: "[REDACTED]", to: "[REDACTED]"}`. The change is recorded but the values are scrubbed. |
| `AuditWriter::skip_if_no_change` | `returns_unit_when_before_equals_after` | When `before == after` field-by-field, the writer returns `Ok(())` without writing audit or outbox rows. Phase-04 `bumps_version_and_updated_at_only_when_fields_changed` relies on this. |

**`SyncEngine` pure helpers (`src-tauri/src/domains/sync/service/sync_engine.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `SyncEngine::should_park_outbox_row` | `parks_when_server_returns_conflict_envelope` | Given a server response `{ accepted: [], conflicts: [{ op_id }] }`, returns `Park { op_id }` for the matching outbox row. Per §7.17. |
| `SyncEngine::should_park_outbox_row` | `does_not_park_on_5xx` | 5xx is a retry-with-backoff, not a park. The function returns `Backoff`, never `Park`. |
| `SyncEngine::should_park_outbox_row` | `does_not_park_on_401_once` | 401 triggers a single refresh + retry; the function returns `RefreshAndRetry`, never `Park`. Per §4 SyncEngine push-step 4. |
| `SyncEngine::reconcile_outbox_lookup_response` | `marks_acked_rows_for_deletion` | Given the `/sync/lookup-op` response `{ found: ["op-A", "op-B"] }` and local outbox `["op-A", "op-B", "op-C"]`, returns `DeleteLocal { op_ids: ["op-A", "op-B"] }`. Per §7.20. |
| `SyncEngine::compute_pull_cursor_advance` | `monotonic_on_canonical_cursor_format` | The cursor format is `<rfc3339_updated_at>|<id_uuid>` per §7.19. The function compares two cursors lex and returns `Advance` only when the new is strictly greater. Empty/null cursor counts as `-infinity`. |
| `SyncEngine::reconcile_delete_vs_edit_lww` | `deleted_row_with_later_updated_at_wins` | Two rows: local `{ updated_at: T2, deleted_at: T2 }`, incoming `{ updated_at: T1, deleted_at: null }`. Returns `KeepLocal`. Per §7.16. |
| `SyncEngine::reconcile_delete_vs_edit_lww` | `tie_goes_to_deletion` | Same `updated_at`, one has `deleted_at != null` -> deletion wins. Per §7.16. |
| `SyncEngine::reconcile_delete_vs_edit_lww` | `manual_policy_entities_always_park` | `policy = Manual` + delete-vs-edit pair -> returns `Park`, regardless of timestamps. Per §7.16. Phase-02 `settings` and phase-05 `visits` both consume this. |
| `SyncEngine::reconcile_audit_log` | `never_accepts_deleted_at_not_null` | Per §7.21: an `audit_log` push row with `deleted_at != null` -> returns `Reject(AuditImmutable)`. The local pruner (`vacuum_unsynced_safe` from phase-08 §7.1) MUST never set `dirty=1`; the engine asserts this contract at the function boundary. |
| `SyncEngine::handle_unsupported_op` | `rejects_delete_op_at_engine_layer` | Per §7.15: any outbox op with `op = 'delete'` returned from the local DB triggers `Err(EngineError::UnsupportedOp)`. Never reaches the network. Defence-in-depth next to the SQL CHECK. |

### §1.2 TS pure functions / value objects (Vitest, no IPC, no React)

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/sync.ts::SyncStatusSchema` | `parses_each_of_5_states` | `'idle' | 'pushing' | 'pulling' | 'offline' | 'error'` round-trip; anything else -> `ZodError`. |
| `src/lib/schemas/sync.ts::ConflictSchema` | `requires_local_and_server_payload_objects` | A conflict missing either side rejects with `path: ['localPayload']` or `['serverPayload']`. |
| `src/lib/schemas/sync.ts::ConflictResolutionSchema` | `requires_choice_local_or_server_or_merged` | Discriminated union: `{ choice: 'local' }` and `{ choice: 'server' }` parse without `merged`; `{ choice: 'merged', merged: {...} }` requires `merged` to be an object. |
| `src/lib/schemas/device.ts::DeviceContextSchema` | `requires_uuid_device_id_and_semver_app_version` | `deviceId` must match UUID regex; `appVersion` must match semver. |
| `src/stores/sync-status-store.ts` | `subscribe_dispatches_state_transition_on_sync_status_event` | Given a stub Tauri event listener, dispatching `sync:status -> 'pushing'` flips the store's `status` and increments a render counter. |
| `src/stores/sync-status-store.ts` | `queued_ops_polls_every_2_seconds_via_outbox_count_ipc` | Per §7.4: with a 2s timer fake, after 2 ticks the IPC mock is called 2 times. The badge re-renders only when the count changes. |
| `src/stores/sync-status-store.ts` | `conflicts_array_updates_on_sync_conflict_event` | Receives a `sync:conflict` event with payload `{ opId, entity, ... }` -> store's `conflicts` array length grows by 1; duplicates by `opId` are deduplicated. |
| `src/stores/device-store.ts` | `populates_once_at_boot_and_does_not_repoll` | The device-info IPC is called exactly once across multiple `useDeviceStore()` mounts. |
| `src/lib/i18n/first-launch.ts` | `forces_ar_on_first_launch_ignoring_os_locale` | Per phase-02 §7.11 (declared in phase-02 but exercised here because the i18n bootstrap is owned by phase-01 §7.10). The fn writes `'ar'` to the store when no prior locale exists. |
| `src/lib/i18n/first-launch.ts` | `respects_persisted_locale_on_subsequent_launch` | Pre-seeded store with `locale: 'en'` -> returns `'en'`. |
| `src/lib/toast.ts::isPhantomNetworkError` | `suppresses_NetworkError_and_OfflineError_causes` | Per §7.12: an `Error` with `cause: NetworkError` returns `true` (suppress toast); a generic `Error` returns `false`. |
| `src/components/shell/breadcrumbs.tsx::resolveCrumb` (extracted helper) | `falls_back_to_route_id_when_handle_crumb_missing` | A route without `handle.crumb` produces a crumb labelled by its `id` slug. Per §7.13. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/sync/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::sync::domain` |
| `src-tauri/src/domains/sync/service/**` (audit_writer, sync_engine pure helpers, outbox repo invariants) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::sync::service` |
| `src-tauri/src/domains/sync/infrastructure/**` (SQLite outbox repo, audit_log repo, sync_state repo) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::sync::infrastructure` |
| `src-tauri/src/sync/**` (SyncEngine push/pull loops, retry / backoff, conflict handling) | >= 95% lines | `cargo llvm-cov --lib --fail-under-lines 95 -- sync` |
| `src-tauri/src/observability.rs` (RedactionLayer from §7.14) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- observability` |
| `src/stores/sync-status-store.ts`, `src/stores/device-store.ts`, `src/lib/schemas/sync.ts`, `src/lib/schemas/device.ts`, `src/lib/toast.ts`, `src/lib/i18n/first-launch.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/stores/{sync-status,device}-store.ts,src/lib/schemas/{sync,device}.ts,src/lib/toast.ts,src/lib/i18n/first-launch.ts"` |
| `src/components/shell/**` (app-shell, sidebar, sync-pill, language-toggle, rtl-boundary, status-bar, breadcrumbs, skip-to-content) | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/components/shell/**"` |
| `sync-server/src/app/sync/domain/**` + `service/**` (SyncPushService, SyncPullService, ConflictResolveService -- including the audit-first ordering and ProcessedOp idempotency) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/sync/presentation/**` (`/sync/push`, `/sync/pull`, `/sync/conflicts/:opId/resolve`, `/sync/lookup-op` from §7.20) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

This phase establishes the coverage gating apparatus. Later plans inherit the tool invocations.

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/sync_phase01.rs` (already exists per `testing-status.md` baseline -- 13 tests at HEAD). Extend; do not duplicate.

Existing scenarios at HEAD (do NOT duplicate):
- `outbox_enqueue_persists_op` (per build cycle)
- (Whatever else commit `cc3b949: all done, not tested yet` authored.)

**New scenarios in `sync_phase01.rs`:**

| Scenario | Asserts |
|-|-|
| `with_audit_orders_audit_first_then_business_then_outbox` | Per §7.7: instrument the writer to record write order in a `Vec<WriteEvent>`; assert the literal sequence is `[InsertAudit, BusinessWrite, EnqueueOutbox]` inside one `BEGIN/COMMIT` tx. |
| `with_audit_rolls_back_business_when_audit_fails` | Drop the `audit_log` table inside the tx; assert: no business row, no outbox row, no audit row -- the entire BEGIN/COMMIT rolls back. |
| `with_audit_rolls_back_audit_when_business_fails` | Force the business write to fail (FK violation against a non-existent referenced row); assert: no audit row, no outbox row. |
| `with_audit_omits_unchanged_fields_in_delta` | Update one field while another stays constant; the persisted `audit_log.delta` JSON contains only the changed field. |
| `with_audit_redacts_password_and_token_fields` | Update a row whose payload has a `password_hash` change; the persisted `delta` shows `{from: "[REDACTED]", to: "[REDACTED]"}`, never the raw bytes. Per §7.14 RedactionLayer + the §1.1 invariant. |
| `outbox_enqueue_uses_partial_index_for_next_batch_query` | `EXPLAIN QUERY PLAN` for `SELECT * FROM outbox WHERE next_attempt_at <= ? AND attempts < 10 AND parked = 0 ORDER BY next_attempt_at LIMIT 50` mentions `outbox_next_attempt`. Per §1 partial index + §7.17 parked extension. |
| `outbox_op_id_unique_constraint_blocks_duplicate_inserts` | Insert two outbox rows with the same `op_id` -> the second hits `SQLITE_CONSTRAINT_PRIMARYKEY`. Defence in depth next to the engine's idempotency. |
| `outbox_partial_index_excludes_parked_rows` | Two rows: one with `parked=0` and one with `parked=1`, both eligible by `next_attempt_at` and `attempts`. `SELECT` via the partial index returns only the unparked row. Per §7.17. |
| `outbox_partial_index_excludes_attempts_above_10` | A row with `attempts=10` is not returned by the next-batch query. The dead-letter signal is the row's presence in `outbox` but exclusion from the queue. |
| `audit_log_immutability_local_attempts_to_update_delta_fail` | The local repo's `vacuum_unsynced_safe` (declared but not yet implemented here; phase-08 §7.1 owns the SQL) is the ONLY allowed mutation path. A direct `UPDATE audit_log SET delta = ? WHERE id = ?` outside this path is forbidden by convention; the test asserts the repository trait has no `update` method that could expose it. (Compile-time check via `trybuild`.) |
| `audit_log_tenant_at_index_used_by_audit_query_plan` | Per §7.9: `EXPLAIN QUERY PLAN` for `SELECT * FROM audit_log WHERE entity_id_tenant = ? ORDER BY at DESC LIMIT 50` mentions `audit_log_tenant_at`. |
| `sync_state_singleton_constraint_enforced` | Per §1 schema `CHECK (id = 1)`: inserting a second row with `id = 2` returns `SQLITE_CONSTRAINT_CHECK`. The repo's `ensure_device_id()` uses `INSERT OR IGNORE`. |
| `sync_state_ensure_device_id_idempotent` | Run `ensure_device_id()` twice in parallel threads; the second sees the first's persisted value. The function never produces two distinct device IDs. |
| `migration_001_idempotent_on_fresh_db` | Run `001_foundation.sql` twice on an empty DB; the second run succeeds because of `CREATE * IF NOT EXISTS`. Tables, indexes, and CHECK constraints match between the two runs (verified by `PRAGMA table_info` + `sqlite_master` snapshots). |
| `migration_001_idempotent_on_populated_db` | Run `001_foundation.sql` against a DB pre-seeded with 100 `audit_log` rows from a synthetic snapshot. Rows preserved, indexes present, no constraint violations. |
| `migration_001_creates_metrics_events_table_and_index` | Per §7.28: `metrics_events` table exists with the CHECK on `kind`, the `metrics_events_kind_at` composite index is present. |
| `engine_push_drains_audit_log_backlog_on_drain_start` | Seed 25 dirty audit rows; start the engine (test harness with a `wiremock` Postgres-mock server returning 200); assert all 25 ops disappear from outbox within 5 simulated seconds. |
| `engine_push_marks_failure_with_exponential_backoff_on_5xx` | `wiremock` returns 503 once. After the failed push, the row's `attempts` is `1` and `next_attempt_at` is `created_at + 1s` (or `2^1 = 2s` -- pin the actual formula). Per §4 step 5. |
| `engine_push_caps_backoff_at_60_minutes` | After 10 failures the next attempt is `now + 60min`, not 1024s or any larger value. |
| `engine_push_refresh_and_retry_on_401_then_pause_on_second_401` | Mock returns 401 twice. Engine calls `auth::refresh` once after the first 401, retries the original op, gets the second 401, emits `auth:expired`, and pauses pushes. Outbox rows preserved. Per §4 step 4 + phase-02 §7.25. |
| `engine_push_parks_outbox_row_when_server_returns_conflict_envelope` | Mock returns `{ accepted: [], conflicts: [{op_id, local, server}] }`. Assert: outbox row's `parked` flag flips to `1`; the row is no longer returned by `next_batch()`. `sync:conflict` event is emitted. Per §7.17. |
| `engine_push_unparks_outbox_row_when_resolver_calls_resolve_conflict` | Set `parked=1`; invoke `sync::resolve_conflict({ opId, choice: 'local' })`; assert `parked` flips back to `0`; the row is eligible for the next push pass. |
| `engine_push_idempotent_replay_on_op_id_collision` | Push op `X`; receive 200. Without deleting the local row, mark it dirty again (simulate a crashed ack); the second push's response is byte-identical (from the server's `ProcessedOp` cache). Local row is deleted after the second push too. |
| `engine_push_partial_batch_per_op_results_isolated` | Mock returns `{ accepted: [A, B, D, E], rejected: [C] }` for a 5-op batch. Assert: A/B/D/E deleted from outbox; C remains with `last_error` populated and `attempts` incremented. Per §6.3 partial-batch coverage. |
| `engine_pull_advances_cursor_in_same_tx_as_apply` | Pull returns 10 rows + `nextCursor`. Apply all 10 + persist cursor in one SQLite tx. If a SIGKILL fires mid-apply (test feature flag) -> reopen finds the cursor at its pre-pull value AND zero applied rows. Per §4 SyncEngine pull-step 3. |
| `engine_pull_cursor_uses_composite_at_id_format` | Cursor encodes as `<rfc3339_at>|<uuid_id>`. Two records updated at the same ms with different ids produce distinct cursor values; the cursor is strictly monotonic across the stream. Per §7.19. |
| `engine_pull_applies_delete_vs_edit_lww_correctly` | Local row `{ updated_at: T2, deleted_at: T2 }`, incoming `{ updated_at: T1, deleted_at: null }` -> local kept (deletion wins via later timestamp). Per §7.16. |
| `engine_pull_applies_delete_vs_edit_tiebreak_deletion_wins_on_equal_timestamp` | Same `updated_at`, one side has `deleted_at != null` -> deletion wins. Per §7.16. |
| `engine_pull_parks_manual_policy_conflict_unconditionally` | Local + incoming both modified on `settings` (manual policy from phase-02 §7.19) -> the engine parks the conflict (does not apply). Per §7.16. |
| `engine_pull_rejects_audit_log_push_with_deleted_at_not_null` | Per §7.21 + §7.31: a pulled `audit_log` row with `deleted_at != null` is rejected by `accept_audit_log_pull` with `AuditImmutable`. Server should never send one, but the client defends in depth. |
| `engine_startup_reconcile_outbox_calls_lookup_op_and_deletes_acked` | Per §7.20: pre-seed 5 outbox rows with `attempts=1` (crashed before ack). Mock `/sync/lookup-op` returns `{ found: [op1, op3, op5] }`. After reconcile: those 3 rows deleted; the remaining 2 re-enter the retry loop. |
| `engine_startup_reconcile_skips_when_outbox_empty_or_all_attempts_zero` | No `/sync/lookup-op` call is made when there are no `attempts > 0` rows. Optimization: avoid the round-trip on the happy path. |
| `engine_emits_metrics_events_sync_push_ok_on_2xx` | Per §7.34: after a successful push, the local `metrics_events` table has one row with `kind = 'sync_push_ok'`, `payload_json` containing `batch_size` and `duration_ms`. Row is NOT marked `dirty=1` (metrics are local-only). |
| `engine_emits_metrics_events_sync_push_fail_on_non_2xx` | After a 503, a row with `kind = 'sync_push_fail'`, `payload_json.http_status = 503`. |
| `engine_emits_metrics_events_sync_pull_ok_and_fail` | Mirror for pull. |
| `engine_emits_metrics_events_sync_conflict_when_parked` | Conflict envelope received -> row with `kind = 'sync_conflict'`, `payload_json.op_id` matches, `payload_json.auto_resolved = false`. Per §7.34 + §7.16 soak harness gating. |
| `embedded_mode_disabled_when_env_var_unset` | Per §7.35: spawn `lib.rs::run` with `IDC_EMBEDDED_MODE` unset; assert the embedded HTTP server is not mounted; assert the log line `embedded_mode=disabled` appears at INFO level. |
| `embedded_mode_enabled_only_when_env_var_eq_1` | Set `IDC_EMBEDDED_MODE=1`; assert the embedded surface is mounted. `IDC_EMBEDDED_MODE=true` or `IDC_EMBEDDED_MODE=yes` are NOT accepted (strict `"1"` only) -- the test pins this. |
| `app_state_construction_populates_db_pool_and_device_id_at_boot` | Per §7.2: `AppState::new()` initialises `db_pool` (verifiable via a trivial `SELECT 1`), `sync_engine` (a non-null Arc), `device_id` (the `sync_state.device_id` value), and `user_context` / `settings_cache` are `None` / empty (populated by phase-02). |
| `tracing_redaction_layer_scrubs_password_token_email_in_event_serialization` | Per §7.14: emit a `tracing::info!("login", password = "secret123", email = "x@y.z")` event; the captured serialised event has `[REDACTED]` for both fields. The bytes `secret123` never appear in the event stream. |

### §2.2 Tauri IPC handler tests

One test per command. Happy + at least one error path.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `sync_status` | `status_returns_snapshot_with_state_and_counts` -> assert the response includes `status: SyncStatus`, `queued_ops: u32`, `last_pushed_at: Option<RFC3339>`, `last_pulled_at: Option<RFC3339>`. | `status_returns_snapshot_when_engine_not_initialised` -> assert a safe default (`Offline`) rather than an error -- the IPC must be infallible. |
| `sync_trigger_push` | `trigger_push_wakes_the_push_loop` -> instrument the engine; after the call the next-batch poll fires within 100ms. | `trigger_push_idempotent_when_loop_already_running` -> calling twice in rapid succession does NOT spawn a second loop. |
| `sync_trigger_pull` | `trigger_pull_wakes_the_pull_loop` -> mirror. | `trigger_pull_idempotent` -> mirror. |
| `sync_list_conflicts` | `list_conflicts_returns_empty_when_none_parked` -> empty array. | `list_conflicts_paginates_via_limit_offset` -> seed 250 parked rows; `limit=100, offset=100` returns rows 100-199. |
| `sync_resolve_conflict` | `resolve_conflict_keep_local_replays_outbox_row` -> seed parked conflict; call resolve with `choice: 'local'`; assert outbox `parked` flag flips to `0`; the engine retries within 1s. | `resolve_conflict_rejects_unknown_op_id_with_typed_error` -> `AppError::NotFound`. |
| `sync_outbox_count` | `outbox_count_returns_zero_when_empty_and_n_when_populated` -> seed N rows with `parked=0`, returns N. Rows with `parked=1` excluded from count. Per §7.4. | (read-only; no error path beyond schema validation) |
| `device_info` | `device_info_returns_uuid_v4_and_semver_version` -> assert the shape. | (boot-cached; no error path) |
| `config_set_sync_server_url` | `set_sync_server_url_persists_to_store_and_reinitialises_engine` -> after a successful set, the engine reads the new URL on its next loop tick. Per §7.22. | `set_sync_server_url_rejects_invalid_url_via_validation_error` -> non-URL input -> `AppError::Validation`. |
| `config_get_sync_server_url` | `get_sync_server_url_returns_persisted_value_or_null_for_fresh_install` -> per §7.22 fresh-install path: returns `None`, engine is in `Offline` mode. | (read-only) |

Notes: IPC tests construct `AppState` directly, register the same services the runtime uses, and exercise the `#[tauri::command]` async fn (callable as a plain async fn in tests). Each test asserts the serialized error shape, not the Rust enum -- the frontend only sees the JSON, so the JSON is the contract. Sets the convention for every later phase.

### §2.3 Sync server route handlers

File: `sync-server/test/sync/foundation-phase01.test.ts` (NEW).

DB: real Prisma test DB (Postgres via `testcontainers`); per-test teardown via `prisma.$transaction([prisma.<everyModel>.deleteMany(...), ...])`.

| Route | Test | Asserts |
|-|-|-|
| `GET /healthz` | `healthz_returns_status_ok_when_db_reachable` | 200 + `{ status: 'ok', version }`. No auth required. |
| `GET /healthz` | `healthz_returns_status_fail_when_db_unreachable` | Kill the DB connection; response body has `status: 'fail'`, HTTP 200 (per phase-09 §3 healthz wiring; phase-01 ships the stub that phase-09 extends). |
| `POST /sync/push` | `push_accepts_audit_log_additive_rows` | 200 + `{ accepted: [op_id], conflicts: [] }`; row persisted with `entityIdTenant` from JWT, `originDeviceId` from header. |
| `POST /sync/push` | `push_is_idempotent_on_op_id_via_processed_op_cache` | Replay the same `op_id` -> identical response body byte-for-byte. Row count in `audit_log` unchanged. Per §4 SyncPushService step 1.i. |
| `POST /sync/push` | `push_idempotent_response_is_byte_identical_not_just_status_match` | The `ProcessedOp.response_hash` matches; the JSON body returned on replay equals the first response (no re-serialization drift). |
| `POST /sync/push` | `push_rejects_unsupported_op_delete_in_v1` | Per §7.15: body with `op = 'delete'` -> 422 with `error.code = 'UNSUPPORTED_OP'`. The v1 server never accepts `delete`. |
| `POST /sync/push` | `push_rejects_audit_log_row_with_deleted_at_not_null` | Per §7.21: 422 with `error.code = 'AUDIT_IMMUTABLE'`. The local pruner sets `deleted_at` WITHOUT marking `dirty=1`; if the rule is broken locally, the server still rejects. |
| `POST /sync/push` | `push_rejects_payload_missing_op_id_with_400` | TypeBox validation fails on the missing required field. |
| `POST /sync/push` | `push_rejects_payload_with_mismatched_entity_id_403` | JWT `entityId` claim != payload `entity_id_tenant` -> 403 (tenant guard). |
| `POST /sync/push` | `push_emits_audit_log_processed_op_response_hash_stable_across_replays` | The second replay finds an existing `ProcessedOp` row whose `response_hash` matches the recomputed hash of the canonicalized response payload. Drift indicates a server bug. |
| `POST /sync/push` | `push_returns_partial_batch_per_op_results_when_one_op_invalid` | Mixed batch of 5 ops where op 3's payload fails TypeBox -> response has 4 in `accepted` and op 3 in `rejected` with `error.code`. Per §6.3. |
| `GET /sync/pull` | `pull_returns_audit_log_rows_since_cursor` | Seed 5 audit rows; pull with empty cursor -> all 5 returned, `nextCursor` advanced past the last row's `(updated_at, id)`. |
| `GET /sync/pull` | `pull_excludes_other_tenants_rows` | Two tenants seeded; the JWT's tenant gets only its rows. |
| `GET /sync/pull` | `pull_respects_limit_and_returns_has_more` | Seed 30 rows; `limit=10` -> 10 rows + `hasMore=true` + `nextCursor` reflects the 10th row's `(updated_at, id)`. |
| `GET /sync/pull` | `pull_cursor_is_strictly_monotonic_under_concurrent_writes` | Two concurrent inserts at the same ms with different IDs. Pull cursor advances past both; the second pull (with the first's cursor) returns zero new rows AND advances no further. |
| `GET /sync/pull` | `pull_sets_pulled_at_on_returned_rows` | Per §7.32: after a successful pull, the row's `pulledAt` column is populated. |
| `POST /sync/conflicts/:opId/resolve` | `resolve_conflict_keep_local_marks_parked_resolved` | Set `resolvedAt` and `resolvedByUserId`; emit an `audit_log` row with `action='conflict_resolve'` (phase-08 §3 §7 cross-coupling owned there; this test asserts phase-01's stub path -- the audit emission lands in phase-08-test). |
| `POST /sync/conflicts/:opId/resolve` | `resolve_conflict_keep_server_returns_canonical_row` | Choice `server` -> response carries the server-canonical row payload. |
| `POST /sync/conflicts/:opId/resolve` | `resolve_conflict_merged_requires_valid_payload` | Choice `merged` with an invalid payload (missing required field) -> 400. |
| `POST /sync/lookup-op` | `lookup_op_returns_found_op_ids_only` | Per §7.20: body `{ op_ids: [A, B, C] }`, server-side persistence has A and C -> response `{ found: [A, C] }`. Tenant-scoped. |
| `POST /sync/lookup-op` | `lookup_op_pure_read_no_side_effects` | After the call, no row in `audit_log`, `ProcessedOp`, or `ConflictParked` was created or modified. |

### §2.4 React Query mutation / query flows

File: `src/features/sync/__tests__/queries.test.tsx`. Mocked IPC via `vi.mock('@/lib/ipc', ...)` returning typed stubs.

RTL invariant (mandatory): every component / hook test that renders DOM MUST run in both `dir=ltr` AND `dir=rtl`. Use `describe.each([['ltr'], ['rtl']])(...)` and assert layout invariants per `.claude/rules/design-system.md` §12. Sets the convention for every later phase.

| Hook | Test | Asserts |
|-|-|-|
| `useSyncStatus` | `subscribes_to_sync_status_event_and_returns_snapshot` | Stub the event listener; after `sync:status -> 'pushing'` fires, the hook returns `{ status: 'pushing' }`. |
| `useSyncStatus` | `outbox_count_updates_via_2_second_poll` | Per §7.4: vitest fake timer; after 2 ticks, the count refresh fires. |
| `useSyncConflicts` | `returns_empty_list_when_engine_emits_no_conflicts` | -- |
| `useSyncConflicts` | `appends_to_list_on_sync_conflict_event` | Fire 3 `sync:conflict` events; assert list has 3 items; assert duplicates by `opId` deduped. |
| `useConflictResolve` (mutation) | `invalidates_sync_conflicts_key_after_resolve` | After `mutateAsync({ opId, choice: 'local' })`, observe `queryClient.invalidateQueries({ queryKey: ['sync','conflicts'] })`. |
| `useConflictResolve` | `surfaces_typed_app_error_on_resolve_failure` | IPC rejects with `{ kind: 'NotFound' }`; mutation error has that shape. |

Components covered separately (each runs `describe.each([['ltr'], ['rtl']])`):
- `<SyncPill>` renders all five states: idle (neutral), pushing (info pulse), pulling (info pulse), offline (gold), error (crimson). Per `.claude/rules/design-system.md` §5.2 status pill convention + §1.4 semantic colors.
- `<SyncPill>` renders the pending-count badge when `outboxCount > 0`. Per §7.4. Badge color follows the design-system §5.4 alert/warn tints.
- `<SyncPill>` `onClick` navigates to `/sync/conflicts` when `status === 'error' || outboxCount > 0`. Per phase-08 §7.14 (forward-receipt). Phase-01 ships the wiring; phase-08 owns the route.
- `<LanguageToggle>` switches between `ar` and `en`; persists via `tauri-plugin-store`; on switch, `<html dir>` flips immediately.
- `<RtlBoundary>` sets `<html dir>` to match the i18n state on mount and on language change.
- `<AppShell>` renders sidebar + topbar + main + status-bar in the correct order, with the `<SkipToContent>` link as the first focusable element (per §7.24). RTL: sidebar mirrors to the right edge.
- `<Breadcrumbs>` derives crumbs from `useMatches()` and renders `<Link>` per `handle.crumb`. Per §7.13. Fallback to route ID when `handle.crumb` is absent.
- `<StatusBar>` shows sync pill + last-synced timestamp + build version. Per §7.13.
- `<SkipToContent>` is `sr-only` until focused; on focus, becomes visible and targets `<main id="main-content">`. WCAG 2.4.1. Per §7.24.
- `<FirstLaunchSetupModal>` (per §7.22) renders when no `config/syncServerUrl` is persisted; submit dispatches `config::set_sync_server_url`; on success closes and re-enables the engine.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Every route in this phase. Ajv against `/documentation/json`.

Harness: `sync-server/test/contract/foundation-phase01-contract.test.ts`. On boot, fetch `GET /documentation/json`, dereference with `@apidevtools/json-schema-ref-parser`, compile relevant subschemas with Ajv 8 + `ajv-formats`. For each canonical payload, validate the actual response against the schema.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /healthz` (response) | `HealthResponseSchema` | Captured live response; validates `status in ['ok','fail']`, `version` matches semver. |
| `POST /sync/push` (request) | `SyncPushBodySchema` (`{ ops: PushOp[] }`) | `fixtures/payloads/sync-push-audit-log-insert.json`. Each op has `op_id`, `entity`, `entity_id`, `op: 'upsert'`, `payload` (MessagePack base64). MUST validate. |
| `POST /sync/push` (request, negative) | `SyncPushBodySchema` | `fixtures/payloads/sync-push-unsupported-op-delete.json` MUST fail Ajv with `error.message` mentioning `op` and `'upsert'` only (per §7.15 v1 invariant). |
| `POST /sync/push` (response) | `SyncPushResponseSchema` (`{ accepted: string[]; conflicts: ConflictResponse[]; rejected?: RejectedOp[] }`) | Captured live response after the canonical push. Validates `accepted: array of UUIDs`, `conflicts: array of conflict envelopes`. |
| `GET /sync/pull` (response) | `SyncPullResponseSchema` (`{ changes: ChangeRow[]; nextCursor: string }`) | Captured live response for the seeded tenant. Each `entity: 'audit_log'` row MUST validate -- this is the only entity in phase-01 that pulls. |
| `POST /sync/conflicts/:opId/resolve` (request) | `ConflictResolveBodySchema` | `fixtures/payloads/conflict-resolve-keep-local.json`, `...keep-server.json`, `...merged.json`. Each MUST validate. |
| `POST /sync/conflicts/:opId/resolve` (response) | `ConflictResolveResponseSchema` | Captured live response. Validates `{ status: 'applied', resolvedAt }`. |
| `POST /sync/lookup-op` (request) | `LookupOpBodySchema` | `fixtures/payloads/lookup-op-canonical.json` with `op_ids: array of UUIDs`. MUST validate. |
| `POST /sync/lookup-op` (response) | `LookupOpResponseSchema` | `{ found: string[] }`. |
| `ErrorResponseSchema` (cross-cutting) | per §7.26 | All 400/401/403/404/409/422/500 responses on every route in this phase reference this schema. Captured negative-path responses MUST validate. |

### §3.2 IPC shape contract

Diff Rust `serde` JSON shape vs TS `Zod` declaration. Fail on drift.

Harness: `src/test-utils/ipc-contract.test.ts` (NEW for this phase; consumed by every later phase). Starts the Tauri binary in test mode, calls each command with a canonical input, captures the JSON, runs `Schema.safeParse(...)`. Companion Rust test (`src-tauri/tests/sync_contract_phase01.rs`) calls each command in-process and serializes the result to `serde_json::Value`, then writes the JSON to a temp file the TS harness reads -- two-process diff.

The last row is FIXED -- every phase that adds an IPC command also exercises the shared error envelope. Do not remove it.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `sync_status` | `SyncStatusSnapshot` (`{ status, queued_ops, last_pushed_at, last_pulled_at, conflicts_count }`) | `SyncStatusSnapshotSchema = z.object({ status: z.enum([...]), queued_ops: z.number().int().nonnegative(), last_pushed_at: z.string().datetime().nullable(), last_pulled_at: z.string().datetime().nullable(), conflicts_count: z.number().int().nonnegative() })` |
| `sync_trigger_push` | `()` | `z.void()` |
| `sync_trigger_pull` | `()` | `z.void()` |
| `sync_list_conflicts` | `Vec<Conflict>` | `z.array(ConflictSchema)` |
| `sync_resolve_conflict` | `()` | `z.void()` |
| `sync_outbox_count` | `u32` | `z.number().int().nonnegative()` |
| `device_info` | `DeviceContext { device_id, app_version }` | `DeviceContextSchema` |
| `config_set_sync_server_url` | `()` | `z.void()` |
| `config_get_sync_server_url` | `Option<String>` | `z.string().url().nullable()` |
| (Error envelope -- fixed) | `AppError` serialized via `Serialize` impl per §7.27 | `AppErrorSchema = z.object({ kind: z.enum(['NotAuthenticated', 'Auth', 'Forbidden', 'NotFound', 'Validation', 'Conflict', 'Db', 'Internal', 'Sync', 'Settings', ...]), message: z.string(), details: z.record(z.unknown()).optional() })` -- one shared schema referenced by every command's error path. Phase-01 establishes the baseline `kind` enum; later phases extend it via `#[from]`. |

The harness MUST also assert the inverse: every Zod-declared field is present in the Rust JSON. A field added on either side without updating the other fails the contract test.

### §3.3 Sync envelope contract

- **Push payload conforms.** `AuditLogPushPayload` (Rust) serialized to JSON -> validate against `SyncPushBodySchema` (TypeBox). Fixture: `fixtures/payloads/audit-log-push-canonical.json`.
- **Pull payload conforms.** Server's `SyncPullResponseSchema` JSON output (with `entity: 'audit_log'`) -> validate against a mirrored Zod schema on the client.
- **Conflict-resolution policy declared and matches expectation.** Assert the policy registry returns `('audit_log', 'additive-only')` per phase-01 §4 sync-semantics table + §7.16 delete-vs-edit carve-out (§7.31).
- **Versioned envelope.** Push body carries `envelope_version: 1`. A stub at `envelope_version: 999` is rejected with `error.code = 'UNSUPPORTED_ENVELOPE_VERSION'`. The engine's forward-compat guard.
- **Snapshot files** (per `.claude/rules/testing.md` §10):
  - `expected/sync/audit-log-push-canonical.json.sha256` -- the canonical push payload for `audit_log` (the only syncable entity in phase-01).
  - `expected/sync/audit-log-pull-row-canonical.json.sha256` -- the canonical pull row.
  - `expected/sync/sync-push-response-empty-canonical.json.sha256` -- empty batch response.
  - `expected/sync/sync-push-response-conflict-canonical.json.sha256` -- conflict envelope shape.
  Canonicalize via stable canonical JSON helper; hash the bytes; commit the hash.

---

## §4 E2E Tests (Pyramid Layer 4)

WebdriverIO + `tauri-driver`. Specs live under `e2e/specs/foundation/`. Every selector is `data-testid` per `.claude/rules/testing.md` §14 anti-patterns -- never CSS classes, never DOM position. Sets the convention for every later phase.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `app-boot-with-sync-pill-idle.e2e.ts` | Any logged-in user (use bootstrap superadmin from phase-02 §7.21 -- but phase-01 E2E uses a stub auth wrapper because phase-02 is not yet built) | 1) Boot the binary. 2) Assert the `<AppShell>` renders. 3) Assert `<SyncPill>` is in `idle` state. 4) Assert `<StatusBar>` shows the last-synced timestamp. | Engine boots without errors; pill renders within 500ms of paint. |
| `language-toggle-flips-html-dir.e2e.ts` | Any | 1) Click `<LanguageToggle>` to switch to `ar`. 2) Assert `<html dir="rtl">`. 3) Switch back to `en`. 4) Assert `<html dir="ltr">`. | `<RtlBoundary>` applies the dir attribute synchronously. Layout mirrors -- screenshot diff confirms. |
| `first-launch-setup-prompts-for-sync-url.e2e.ts` | Fresh install | 1) Clear `tauri-plugin-store` (test fixture). 2) Boot. 3) Assert `<FirstLaunchSetupModal>` opens. 4) Enter `http://localhost:3161`. 5) Submit. | Per §7.22: `config/syncServerUrl` persisted; modal closes; engine re-initialises; `<SyncPill>` transitions to `idle` from `offline`. |
| `audit-log-push-drains-on-network.e2e.ts` | Stub-authed user | 1) Boot with the test server reachable. 2) Trigger a synthetic `audit_log` write (use a debug IPC `__debug__::seed_audit_row`). 3) Assert `<SyncPill>` shows `pushing` briefly. 4) Assert `<SyncPill>` returns to `idle`. 5) Assert outbox count is 0. | The full push round-trip completes in less than 1s p95. `audit_log` row exists on the server. |
| `breadcrumbs-render-from-route-handle.e2e.ts` | Any | 1) Navigate to a test route with `handle: { crumb: () => 'Test Page' }`. 2) Assert `<Breadcrumbs>` shows the crumb. | Per §7.13. |

### §4.2 Failure-path flows

- **`offline-audit-write-drains-on-reconnect.e2e.ts`** -- Set `--offline` flag on tauri-driver; trigger an `audit_log` write; assert UI confirms; sync pill shows `offline`; outbox row persists. Lift the offline flag; assert pill goes `pushing -> idle`; assert server has the row.
- **`server-5xx-during-push-retries-with-backoff.e2e.ts`** -- WireMock the sync server to return 503 three times then 200; trigger a write; assert outbox `attempts` advances; assert eventual drain; assert no row duplication.
- **`token-expiry-mid-push.e2e.ts`** -- Force JWT to expire (clock-skew the test rig); trigger a push; assert one 401 -> automatic refresh -> retry succeeds. Assert no duplicate audit row.
- **`session-expired-after-second-401.e2e.ts`** -- Force two consecutive 401s; assert `auth:expired` event fires; assert the engine pauses pushes; assert outbox is preserved (no clearing); assert `<SyncPill>` is in `error` state. Per phase-02 §7.25.
- **`conflict-envelope-parks-outbox-row.e2e.ts`** -- Server returns `{ conflicts: [{op_id, local, server}] }` for a synthetic op. Assert: outbox row's `parked` flag flips to 1; `<SyncPill>` shows `error`; the pending-conflict badge on the pill (phase-08 §7.14 wiring) renders the count. Per §7.17.
- **`outbox-startup-reconcile-finds-acked-ops.e2e.ts`** -- Pre-seed the local outbox with 5 rows having `attempts=1` (simulating a crashed prior session). Boot; assert `/sync/lookup-op` is called; assert the rows returned by `found:` are deleted locally; assert the engine resumes normal operation. Per §7.20.
- **`unsupported-delete-op-rejected-engine-and-server.e2e.ts`** -- Manually construct an outbox row with `op = 'delete'` (bypass the engine). Assert the engine layer rejects with `EngineError::UnsupportedOp` (per §1.1 invariant). If the engine is bypassed entirely and the row reaches the server, the server rejects 422. Per §7.15.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

Two binaries, shared sync server.

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-audit-log-push-pull.e2e.ts` | Device A pushes one `audit_log` row; Device A reconnects. Device B pulls. | Device B's local `audit_log` has the row. The row's `pulledAt` is populated on the server. |
| `two-device-cursor-monotonicity.e2e.ts` | Device A pushes 100 rows in parallel batches; Device B pulls in batches of 10. | Device B's local audit count is 100; cursor advanced strictly monotonically; no duplicates. |
| `two-device-additive-policy-survives-same-row-id-conflict.e2e.ts` | Both devices push `audit_log` rows with the SAME `op_id` (simulating a clone/sync glitch). | Server returns the cached response for the second push; only one row exists on the server; both devices' local outbox drains correctly. Per §4 additive-only policy + idempotency. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: app shell in both dir=ltr and dir=rtl.** Verify the sidebar position (right in RTL, left in LTR), the `<SkipToContent>` link appears on first Tab keypress, the `<StatusBar>` renders the sync pill at the leading edge in both directions.
- **`<SyncPill>` five states.** Force each state via test-only IPCs; visually verify color (idle = neutral, pushing/pulling = info blue, offline = gold, error = crimson) and pulse animation per design-system §4.3.
- **First-launch flow.** Fresh install; verify `<FirstLaunchSetupModal>` renders RTL when OS locale is ar; the sync URL field accepts http+https; submission persists.
- **Keyboard navigation.** Tab through `<AppShell>`: `<SkipToContent>` first, then `<LanguageToggle>`, then sidebar items, then main content. Verify focus rings are visible per `.claude/rules/design-system.md` §3.3 + §7.25.
- **Screen reader announcements.** With NVDA / VoiceOver enabled, verify: `<SyncPill>` state changes announce via `aria-live="polite"`; modal open/close announces `role="dialog"`.

### §5.2 Cross-references to `personas.md`

Phase 01 surfaces are exercised end-to-end by:
- `personas.md` -> **P3 Mariam the Superadmin** -> steps 1-3 (boots the app, observes the sync pill, watches the outbox drain after editing settings). Required for §8 DoD.
- All other personas (P1, P2, P4, P5) transitively depend on phase-01 infrastructure but their day-scripts exercise higher-layer surfaces. Optional reinforcement.

**Canonical: P3 Mariam the Superadmin.** P3 MUST pass for §8 DoD to flip to `complete`.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **Asia/Baghdad fixed offset.** `audit_log.at` is stored as RFC3339 with `Z` (UTC). Display in the audit page (phase-08) uses Baghdad local. Phase-01 owns the storage invariant: a `tracing` event timestamped at `23:59:30 +03:00` local stores as the equivalent UTC; round-trips byte-stable. Asserted in `audit_at_round_trips_through_baghdad_local_time`.
- **Clock skew vs server.** Device clock is 10 min ahead of the server. Push an audit row; server stamps `updatedAt` as authoritative; pull-back replaces the local `updated_at` with the server stamp. Asserted in `audit_pullback_uses_server_updated_at`. Per `.claude/rules/offline-first.md` "Common Pitfalls".
- **DST defensive.** Iraq has no DST. CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `src-tauri/src/sync/` and `src-tauri/src/domains/sync/`; only `chrono::FixedOffset::east_opt(3 * 3600)`. Sets the convention every later phase inherits.
- **Backoff time math.** `compute_backoff` (§1.1) uses `chrono::Duration::seconds` and `Utc::now()`; the test pins that no `chrono_tz` API is in the call graph.

### §6.2 i18n & RTL

- **en/ar swap on app shell.** Snapshot `<AppShell>`, `<Sidebar>`, `<StatusBar>` in both locales; assert every visible string comes from `common.*` or `errors:sync.*` i18n keys -- no string literals in JSX. Asserted by a `grep`-style test in §2.4 component tests + the §7.10 lint script in CI.
- **Arabic-Indic numerals on numeric chrome.** `<SyncPill>` outbox-count badge renders Arabic-Indic digits when `settings.arabic_numerals === true`. `<StatusBar>` build version (e.g., `0.1.0`) is technical and stays ASCII. Per `.claude/rules/design-system.md` §11.
- **RTL layout invariants.** `<SkipToContent>` first-focusable in both directions; eyebrow rule (when used by phase-02+) mirrors to the right edge; `<SyncPill>` dot leads its label in both directions.
- **Mixed-direction text in error toasts.** A toast with mixed Arabic + English content (e.g., a server error message in `ar` with a UUID `op_id` in ASCII) renders without bidi mangling; the UUID stays LTR via `bdi` or LRM marks.
- **i18n key inventory baseline.** Per §7.10 + §7.30: the `common`, `errors`, `receipts` namespaces exist on both locales at the end of this phase. The `errors:sync.*` keys are registered (`network_offline`, `server_unavailable`, `auth_expired`, `already_resolved`). Phase-08 lint script will eventually fail CI if a sync-error variant lacks an entry on either locale; phase-01 must ship the baseline.

### §6.3 Offline & Network

- **Full offline mode.** `offline-audit-write-drains-on-reconnect.e2e.ts` (§4.2). Writes work fully offline; the UI never blocks on a network call for any read. The sync pill shows `offline` until network returns.
- **Intermittent connection.** Push 5 ops; drop the connection mid-3rd op; assert the engine retries from op 3, not op 1 (the cursor advances after each acked op). Test: `intermittent-push-resumes-cleanly.e2e.ts`. Per phase-04 pattern (forward-applied; phase-01 owns the engine behavior).
- **Token expiry mid-sync.** `token-expiry-mid-push.e2e.ts` (§4.2). One 401 triggers refresh + retry once; second 401 emits `session_expired`, pauses pushes. Per §4 step 4 + phase-02 §7.25.
- **Server returns 5xx.** `server-5xx-during-push-retries-with-backoff.e2e.ts` (§4.2). Exponential backoff: `1s, 2s, 4s, ..., 60min` cap. The `outbox.attempts` advances and `last_error` is populated for surfacing in the sync status UI.
- **Partial-batch push.** Push 50 ops where op 27 violates a server-side TypeBox check. Assert ops 1-26 are `applied`, op 27 is `rejected` with a reason, ops 28-50 are still `applied`. The engine does NOT roll back the whole batch. Integration test `partial_batch_push_handles_per_op_results` from §2.1.
- **Conflict envelope.** Server returns `{ accepted: [], conflicts: [{op_id, local, server}] }`; assert outbox `parked=1`; assert the row is no longer retried; assert `<SyncPill>` shows `error`. Per §7.17.
- **Outbox reconciliation on startup.** Per §7.20: pre-seed rows with `attempts > 0`; assert `/sync/lookup-op` is called on boot; assert acked rows are deleted; remaining rows enter the retry loop. Asserted in §2.1 + §4.2.

### §6.4 Concurrency & Conflicts

- **2-device same row.** N/A in phase-01 -- the only entity is `audit_log` (additive-only); two devices writing the same `op_id` get idempotent dedup via `ProcessedOp`. Tested in §4.3 `two-device-additive-policy-survives-same-row-id-conflict.e2e.ts`.
- **3-device chain on cursor monotonicity.** Devices A, B, C all push audit rows; D pulls. D's audit table contains all rows from A+B+C; cursor advanced strictly monotonically; no duplicates. Asserted in `three_device_cursor_monotonicity` (Rust integration).
- **Conflict policy invocation.** Assert the policy registry returns `'additive-only'` for `audit_log`. Assert no `manual` 409 response is ever emitted for an `audit_log` push (this entity never parks). Per §4 sync-semantics + §7.21.
- **Conflict resolver round-trip.** N/A for `audit_log` -- the entity's policy is `additive-only`, it never parks. Phase-01 ships the resolver mechanism (`/sync/conflicts/:opId/resolve` + `sync::resolve_conflict` IPC) -- the UI lands in phase-08. Phase-01 tests the mechanism's correctness (resolve replays the outbox row, server records `resolvedAt`); the resolver's UI round-trip is owned by phase-08 test.
- **Delete-vs-edit reconciliation.** Per §7.16: local soft-delete at T2 wins over incoming edit at T1; tie goes to deletion. Asserted in `engine_pull_applies_delete_vs_edit_lww_correctly` (Rust integration). This invariant is consumed by every LWW entity in phases 02-05.

### §6.5 Crash & Recovery

- **SIGKILL during outbox enqueue + audit transaction.** Spawn the binary in a test harness, fire a synthetic audit write, kill the process between (a) audit-row INSERT and (b) outbox-row INSERT (instrument via a feature-gated `panic!`). Reopen; assert: no audit row, no outbox row. The tx atomicity holds. Test: `crash_mid_audit_outbox_leaves_no_partial_state`.
- **SIGKILL during pull-apply transaction.** Per §2.1: a SIGKILL between apply and cursor advance rolls back both. The cursor never advances past unapplied rows.
- **SQLite WAL after crash.** Kill the binary while WAL has uncommitted frames. Reopen with `journal_mode=WAL` + `busy_timeout=5000`; assert recovery is clean, no orphan WAL files, all queries succeed. Test: `wal_recovery_after_audit_crash`.
- **Disk full.** Mount a tmpfs sized just below the migration footprint + 1 row; attempt an audit write; assert `AppError::Db` with a clear "disk full" message; no half-written row. Test: `disk_full_on_audit_write_returns_typed_error` (gated `--ignored` in CI).
- **Startup reconcile after crashed prior session.** Per §7.20 + §4.2 `outbox-startup-reconcile-finds-acked-ops.e2e.ts`. Pre-seed `attempts > 0` rows; assert reconcile path runs on boot.

### §6.6 Scale & Performance

- **10k `audit_log` rows.** `audit_log_*` index lookup over 10k rows: < 30 ms p99. The `audit_log_tenant_at` composite (§7.9) drives the query. `EXPLAIN QUERY PLAN` confirms.
- **Outbox drain throughput.** Backlog of 500 audit ops -> drain at >= 50 ops/sec (default SLO from `.claude/rules/testing.md` §9). Asserted in `perf_outbox_drain_audit_backlog`.
- **Pull at scale.** Pull a 1000-row page from the server: server-side handler < 200 ms p95; client-side apply (including delete-vs-edit reconciliation) < 500 ms p95. Per §9 defaults.
- **Cursor advance over a 12-month backlog.** Synthetic 100k audit rows on the server; cold-pull from a fresh client; the engine drains the backlog in <= 30 min (informational; not gated -- the typical clinic doesn't reach this).
- **`ProcessedOp` lookup latency.** A 100k-row `ProcessedOp` cache: `has(op_id)` < 5 ms p99. The table's PK on `op_id` covers this.

### §6.7 Security & Permissions

- **JWT tampering.** Alter `role` claim from `receptionist` to `superadmin` and replay against `/sync/push`. Server rejects 401 (signature invalid) -- never trusts the claim shape; verifies RS256. Phase-01 owns the verifier; the test asserts the boundary. Cross-cutting full matrix in `security.md`.
- **JWT public-key pinning at boot.** Per phase-02 §7.10 (forward-receipt): on boot, GET `/auth/jwks`; compare against stronghold's pinned `jwt/publicKey`; refuse startup if mismatched without `--reset-jwt-pin`. Phase-01 ships the bootstrap function stub; phase-02 wires the full call. Phase-01 test asserts the function signature exists and the comparison logic is correct.
- **Tenant cross-boundary blocked at `/sync/push`.** A push with `entity_id_tenant != JWT.entityId` returns 403. Tested in §2.3.
- **Replay-after-revoke.** Phase-01 owns refresh-token revocation only via the `auth::expired` event handling; full replay matrix in `security.md`. Cross-reference only.
- **`audit_log` write privilege.** Any authenticated client can push `audit_log` rows for its tenant (additive-only). Per §4 sync-semantics. The role-gate matrix for higher-privilege entities (users, settings) lands in phase-02 test.
- **`/sync/lookup-op` is tenant-scoped read.** Per §7.20: a token for tenant A cannot look up op_ids belonging to tenant B. Asserted in §2.3.
- **`config_set_sync_server_url` accepts only http+https schemes.** Asserted in §2.2 error path.

### §6.8 Data Integrity

- **Migration replay forward.** `001_foundation.sql` is idempotent on fresh DB AND on a DB seeded with synthetic baseline data. All `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` succeed twice. Asserted in `migration_001_idempotent_on_fresh_db` + `migration_001_idempotent_on_populated_db`.
- **Migration replay against populated DB.** Per phase-02 §7 receipts: when phase-02 lands the FK from `audit_log.actor_user_id -> users(id)`, the rebuild migration is idempotent. Phase-01 ships the table without the FK; phase-02 adds it. Phase-01 test asserts the rebuild path leaves rows intact.
- **FK enforcement on `outbox` and `sync_state`.** Both tables are infrastructure with no FKs in phase-01 (the audit-log FK to `users` lives in phase-02). The test asserts the schema is correct.
- **`sync_state` singleton invariant.** Per §1 schema `CHECK (id = 1)`: only one row exists. Asserted in `sync_state_singleton_constraint_enforced` (§2.1).
- **CHECK constraint enforcement.** Per §7.15: any row with `outbox.op != 'upsert'` is rejected at the SQLite layer. Per §1: `metrics_events.kind` is closed-enum CHECK; any unknown kind is rejected. Asserted in §2.1.
- **`audit_log` append-only invariant.** No `UPDATE audit_log SET delta = ?` path exists in the repository (compile-time check via `trybuild` in §2.1). The only allowed mutation is `vacuum_unsynced_safe` (phase-08 §7.1) which sets `deleted_at` WITHOUT marking `dirty=1`.
- **`ProcessedOp` retention.** Per §7.18: the daily vacuum purges rows older than 30 days. Phase-01 ships the schema + the job stub; phase-08 owns the scheduling. Phase-01 test asserts the vacuum SQL is `DELETE FROM processed_ops WHERE processed_at < now - INTERVAL '30 days'` and that one execution emits exactly one `audit_log` row with `action='vacuum'`.
- **`metrics_events` retention.** Per §7.28 + phase-08 §7.21: 30-day retention via the same vacuum that prunes `audit_log`. Phase-01 ships the table; phase-08 ships the vacuum execution. Phase-01 test asserts the table is local-only (`origin_device_id IS NULL`, no `dirty` column).
- **`sync_version` monotonicity.** Every mutation to a syncable row increments `version` by exactly 1. Phase-01 verifies the contract via the `with_audit` helper's invariant: a single `with_audit` call bumps `version` by 1. Asserted in `with_audit_bumps_version_by_one_per_invocation`.

---

## §7 Performance SLOs (this phase's surfaces)

Default SLOs in `.claude/rules/testing.md` §9 apply unless overridden. The `Default?` column declares whether the threshold is the §9 default (`yes`) or a phase-specific override (`no`).

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `with_audit` full transaction (audit + business + outbox) | < 30 ms p99 | no (tighter than §9's 200ms lock SLO because audit-only transactions are simpler than visit locks) | `perf_with_audit_typical_under_30ms` | This is the hot path every later phase rides on; budget tight. |
| Tauri (SQLite) | `outbox.next_batch(limit=50)` query | < 5 ms p99 | yes | `perf_outbox_next_batch_under_5ms` | Default single-record / index-driven SLO; the `outbox_next_attempt` partial index drives it. |
| Tauri (SQLite) | `audit_log` query for the audit page (90-day window, tenant-scoped) | < 30 ms p99 | yes | `perf_audit_log_tenant_at_window_query` | Default list-query SLO; index-driven via `audit_log_tenant_at`. |
| Tauri (SQLite) | `sync_state` read (cursor + device_id) | < 5 ms p99 | yes | `perf_sync_state_read` | Single-row PK lookup. |
| Tauri (SQLite) | Cold-start: first paint after launch | < 3 s p99 | yes | `perf_cold_start_first_paint` | Per §9. |
| Sync engine | Outbox drain throughput | >= 50 ops/sec | yes | `perf_outbox_drain_throughput` | Per §9. |
| Sync engine | Push round-trip (single op) | < 1 s p95 | yes | `perf_push_single_op_round_trip` | Per §9. |
| Sync engine | Pull (typical batch of 100 ops) | < 2 s p95 | yes | `perf_pull_typical_batch` | Per §9. |
| Sync engine | 8-hour soak steady-state outbox depth | <= 800 rows | yes | `perf_outbox_steady_state_under_800` | Per §9. Phase-08 owns the soak harness; phase-01 ships the metric. |
| Sync server (Postgres) | `/sync/push` for a 50-op audit_log batch | < 200 ms p95 | yes | `perf_server_push_50_audit_ops` | Per §9. |
| Sync server (Postgres) | `/sync/pull` for a 100-row audit_log page | < 200 ms p95 | yes | `perf_server_pull_100_audit_rows` | Per §9. |
| Sync server (Postgres) | `/sync/lookup-op` for a 500-op_id batch | < 50 ms p95 | no | `perf_server_lookup_op_500_ids` | Per §7.20; the call is pure-read and frequent on every boot. |
| Frontend | `<AppShell>` first paint after route navigation (warm cache) | < 100 ms | -- | `perf_app_shell_warm_paint` | Asserted via React Profiler in the E2E rig. |
| Frontend | `<AppShell>` first paint cold (no cache) | < 300 ms | -- | `perf_app_shell_cold_paint` | One IPC + one render pass. |
| Frontend | `<SyncPill>` state transition latency | < 50 ms | -- | `perf_sync_pill_transition` | Subscribes to `sync:status` event; render must keep up. |

Perf tests run in `cargo test --test sync_perf_phase01 --release` + `vitest run --mode benchmark`. Variance failures are real bugs, not flakes -- fix the variance, do not relax the threshold.

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI (`cargo test -p app_lib --lib` + `vitest run --project unit`).
- [ ] All §2 integration tests green in CI:
  - `cargo test --test sync_phase01 --test sync_commands_phase01`
  - IPC handler tests for all 9 commands listed in §2.2.
  - `pnpm --filter sync-server test -- sync/foundation-phase01`
  - `vitest run --project integration`
- [ ] All §3 contract tests green in CI (`pnpm test:contract`).
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- foundation/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P3 Mariam the Superadmin** runs end-to-end and passes (record date / runner in row below).
- [ ] §6 all eight edge categories addressed (no empty subsections).
- [ ] §7 SLOs met for every row; override rows have a recorded rationale in the test source.
- [ ] Coverage gates met per §1.3:
  - [ ] `domains::sync::domain` >= 90%
  - [ ] `domains::sync::service` >= 90%
  - [ ] `domains::sync::infrastructure` >= 75%
  - [ ] `sync` (engine push/pull loops) >= 95%
  - [ ] `observability` (RedactionLayer) >= 90%
  - [ ] Frontend shell + stores (per §1.3 glob) >= 90% on stores and schemas, >= 60% on components
  - [ ] Sync server domain + service >= 90%
  - [ ] Sync server presentation (push / pull / resolve / lookup-op) >= 85%
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed where `.claude/rules/testing.md` §10 applies:
  - `expected/sync/audit-log-push-canonical.json.sha256`
  - `expected/sync/audit-log-pull-row-canonical.json.sha256`
  - `expected/sync/sync-push-response-empty-canonical.json.sha256`
  - `expected/sync/sync-push-response-conflict-canonical.json.sha256`
- [ ] `testing-status.md` row updated (Unit / Integration / Contract / E2E / Manual counts, Coverage %, Started / Completed dates, Open Defects).
- [ ] Lint, typecheck, build all green (`pnpm lint && pnpm build && cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd ../sync-server && pnpm lint && pnpm typecheck && pnpm test`).

**Persona run record:**

The first row is the **canonical persona** -- the one persona script that gates `complete` per `.claude/rules/testing.md` §11. Pick exactly one from `personas.md`. Additional rows are optional reinforcement runs.

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P3 Mariam the Superadmin** | -- | -- | -- | -- |
| P1 Asma the Accountant (reinforcement) | -- | -- | -- | Optional, exercises sync pill + outbox + audit_log push during daily-close ops. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P01-G01 -- JWT public-key pinning behavioural coverage (CRITICAL)

- **Source:** phase-01.md "Proves" + §6.7
- **Target test section:** §6.7 / §2.1
- **Category:** Incomplete Coverage

The build spec promises JWT public-key pinning at boot: bootstrap fetch from `/auth/jwks`, compare against stronghold's pinned `jwt/publicKey`, refuse startup on mismatch unless `--reset-jwt-pin` is passed. The current test plan only asserts "the function signature exists and the comparison logic is correct" (§6.7) -- no behavioural test of the actual bootstrap path, the refuse-on-mismatch failure mode, or the override flag. Phase-01 ships the bootstrap function and MUST cover its observable boot behaviour even though phase-02 wires the full call.

New §2.1 rows in `sync_phase01.rs`:

| Scenario | Asserts |
|-|-|
| `jwt_pin_bootstrap_fetches_and_persists_on_fresh_install` | Fresh stronghold (no pinned key); start the boot path with `wiremock` serving `/auth/jwks` returning a known RS256 public key; assert the key is persisted to stronghold's `jwt/publicKey` slot; assert boot completes. |
| `jwt_pin_bootstrap_refuses_startup_on_mismatch` | Pre-seed stronghold with key K1; `wiremock` serves `/auth/jwks` returning key K2; assert boot returns `Err(AppError::Auth)` with `kind = 'JwtPinMismatch'`; assert no Tauri windows are created; assert the persisted key is unchanged. |
| `jwt_pin_bootstrap_reset_flag_overwrites_persisted_key` | Pre-seed stronghold with key K1; start with `--reset-jwt-pin` (env / CLI flag per §6.7); `wiremock` serves key K2; assert stronghold's `jwt/publicKey` slot now holds K2; assert boot completes. |
| `jwt_pin_bootstrap_offline_uses_persisted_key_without_fetch` | Pre-seed stronghold with key K1; `wiremock` is unreachable; assert boot completes (offline-first: pinned key is sufficient); assert no `auth:expired` event fires; assert one `metrics_events` row with `kind='jwt_pin_offline'`. |

### §9.2 P01-G02 -- Tauri capabilities allowlist contract (CRITICAL)

- **Source:** phase-01.md §3 Tauri capabilities
- **Target test section:** §2.1 / §6.7
- **Category:** Missing Integration Test

The build spec restricts `capabilities/default.json` to `store/stronghold/os/path/dialog/log` and explicitly forbids `http:default` (offline-first invariant -- all HTTP must go through the engine, never raw frontend fetch). No current test pins this contract; a regression that re-enables `http:default` or drops `stronghold` is silently shipped.

New §2.1 row in `sync_phase01.rs` (or a co-located `capabilities_phase01.rs` file):

| Scenario | Asserts |
|-|-|
| `capabilities_default_json_matches_phase01_allowlist` | Parse `src-tauri/capabilities/default.json`; assert the `permissions` array contains EXACTLY `core:default`, `store:default`, `stronghold:default`, `os:default`, `path:default`, `dialog:default`, `log:default` (or the granular per-plugin equivalents the phase declares); assert `http:default` is ABSENT; assert no `shell:default` or `notification:default` (the latter is also P01-G17). Diff is byte-stable across runs. |

### §9.3 P01-G03 -- tauri-plugin-fs registration and log fs-scope (HIGH)

- **Source:** phase-01.md §7.1
- **Target test section:** §2.1 / §6.8
- **Category:** Missing Integration Test

Per §7.1, phase-01 registers `tauri-plugin-fs` and declares an `fs:scope` capability for the log directory so phase-05 receipts and phase-08 audit exports can write without bespoke per-feature shell-outs. Without a test, a missing `.plugin(fs::init())` in `lib.rs` or a dropped `fs:scope` glob compiles cleanly but breaks shipped builds the moment a receipt or log line tries to write.

New §2.1 rows:

| Scenario | Asserts |
|-|-|
| `tauri_plugin_fs_registered_in_lib_rs_run` | Boot the binary in test mode; assert `app.handle().plugin_handle::<fs::Fs>()` returns `Ok` (plugin is mounted); assert calling the plugin's read/write API on a path inside the declared scope succeeds. |
| `fs_scope_capability_grants_log_directory_writes` | Parse `capabilities/default.json` (or `main.json`); assert the `fs:scope` entry includes the per-OS log directory glob (`$APPLOG/*` or equivalent); assert a write inside the scope succeeds and a write OUTSIDE the scope returns `AppError::Forbidden`. |

### §9.4 P01-G04 -- sync:progress event emission during drain (HIGH)

- **Source:** phase-01.md §4 SyncEngine emits `sync:progress`
- **Target test section:** §2.4 / §4.1
- **Category:** Missing Edge Coverage

The build spec promises the engine emits `sync:progress { pushed, total }` events during drain so the `<SyncPill>` and any future progress UI can reflect partial-batch progress. The test plan covers `sync:status` and `sync:conflict` exhaustively but never asserts `sync:progress` payload shape or emission cadence.

New §2.4 row (mocked IPC event stream):

| Hook | Test | Asserts |
|-|-|-|
| `useSyncStatus` | `receives_sync_progress_events_during_push_drain` | Stub the event listener; engine fires `sync:progress { pushed: 1, total: 5 }`, `{ pushed: 3, total: 5 }`, `{ pushed: 5, total: 5 }` for a 5-op batch. Assert the hook exposes `progress: { pushed, total }`; assert intermediate values are observed (not just terminal); assert `progress` resets to `null` once `status` returns to `idle`. |

New §4.1 row:

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `sync-progress-event-emits-per-acked-op.e2e.ts` | Stub-authed user | 1) Seed a 10-op `audit_log` backlog. 2) Trigger drain. 3) Capture all `sync:progress` events via a test-only IPC `__debug__::capture_events`. | At least 10 progress events emitted; `pushed` monotonically increases from 1 to 10; final event has `pushed === total`; no progress event fires while status is `idle`. |

### §9.5 P01-G05 -- LWW tiebreak via originDeviceId lex ordering (HIGH)

- **Source:** phase-01.md §4 SyncPushService step 5 LWW tiebreak
- **Target test section:** §2.3 / §6.4
- **Category:** Missing Edge Coverage

§4 step 5 specifies a deterministic LWW tiebreak: when `version` AND `updated_at` are equal (genuine collision across two devices clocked identically), the row with the lexicographically smaller `originDeviceId` wins. The current §6.4 covers `version` and `updated_at` paths and the delete-vs-edit tie, but never the device-id tiebreak.

New §2.3 row in `sync-server/test/sync/foundation-phase01.test.ts`:

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_lww_tiebreak_uses_origin_device_id_lex_ordering` | Seed a server row with `version=3, updated_at=T, originDeviceId='device-zz...'`. Push an op for the same `entity_id_tenant + entity` with `version=3, updated_at=T, originDeviceId='device-aa...'`. Assert the incoming row wins (server row replaced); replay the same scenario with reversed device IDs and assert the existing row is preserved; assert response is `{ accepted: [op_id], conflicts: [] }` (not parked -- tiebreak is deterministic, not a conflict). |

### §9.6 P01-G06 -- Reject non-audit_log entity on /sync/push in v1 (HIGH)

- **Source:** phase-01.md §4 SyncPushService step 1.ii
- **Target test section:** §2.3
- **Category:** Missing Integration Test

Per §4 SyncPushService step 1.ii, in v1 the only syncable entity is `audit_log` -- any push payload referencing a different entity name must be rejected with 422 `UNKNOWN_ENTITY`. The current test set covers `op = 'delete'` rejection (P01-G15 in build) but not the entity-name allowlist.

New §2.3 row:

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_rejects_non_audit_log_entity_with_422_unknown_entity` | Body with `op.entity = 'visit'` (or any string other than `'audit_log'`); response is HTTP 422 with `error.code = 'UNKNOWN_ENTITY'`, `error.details.entity = 'visit'`, `error.details.allowed = ['audit_log']`. Row count in every server table is unchanged. |

### §9.7 P01-G07 -- SyncCursor compound @@id contract (HIGH)

- **Source:** phase-01.md §7.19 SyncCursor compound `@@id`
- **Target test section:** §2.3 / §3.3
- **Category:** Missing Contract Test

§7.19 declares `SyncCursor` uses a compound primary key `@@id([deviceId, entityIdTenant])` so each (device, tenant) pair gets an independent pull cursor. A regression to a single `@id` field would silently collapse cursors across devices or tenants -- catastrophic data leak and broken pull pagination. The current plan never schema-asserts the compound PK.

New §2.3 / §3.3 row:

| Surface | Test | Asserts |
|-|-|-|
| Prisma schema | `sync_cursor_schema_has_compound_at_at_id_device_id_plus_entity_id_tenant` | Parse `sync-server/prisma/schema.prisma` (or introspect via `prisma.$queryRaw`); assert the `SyncCursor` model declares `@@id([deviceId, entityIdTenant])` in exactly that column order; assert no `@id` exists on a single field; run `prisma migrate diff --from-schema-datamodel --to-schema-datasource` and assert empty diff. Negative: temporarily replace with a single `@id` field in a fixture schema and assert the test fails loudly. |

_Corrected 2026-05-13 per Pass 2 P01-G19: the original "Asserts" cell named `[entityIdTenant, entity]`, which does not match the build-spec declaration at phase-01.md line 650 (`@@id([deviceId, entityIdTenant])`); see §10.1 for the audit trail._

### §9.8 P01-G08 -- 500-row per-entity pull cap SLO (HIGH)

- **Source:** phase-01.md §4 SyncPullService step 2
- **Target test section:** §7
- **Category:** Missing Performance SLO

§4 SyncPullService step 2 caps each pull at 500 rows per entity to keep response latency bounded. The current §7 SLO table lists "Pull (typical batch of 100 ops) < 2 s p95" but no SLO row pins the 500-row HARD CAP behaviour: a server with 5000 unpulled rows for a tenant must return exactly 500 rows + `hasMore=true` + `nextCursor`, never the full set.

New §7 row:

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Sync server (Postgres) | `/sync/pull` with 5000 unpulled `audit_log` rows for one tenant (cap behavior) | exactly 500 rows returned + `hasMore=true` + valid `nextCursor`; response < 250 ms p95 | no (override of §9 200ms default; the cap test exercises the worst-case page) | `perf_server_pull_caps_at_500_rows_per_entity` | Per §4 SyncPullService step 2; prevents unbounded response growth on first-cold-pull. The cursor in `nextCursor` MUST be strictly greater than the 500th row's `(updated_at, id)` so the next page makes forward progress. |

### §9.9 P01-G09 -- AuditLog server-side composite index (MEDIUM)

- **Source:** phase-01.md §7.33 AuditLog server indexes
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.33 declares the server `AuditLog` model carries `@@index([entityIdTenant, at(sort: Desc)])` to keep tenant-scoped audit queries (phase-08's audit page) index-driven. No current test asserts the index ships in the Prisma migration or that the audit-page query plan uses it.

New §2.3 row:

| Surface | Test | Asserts |
|-|-|-|
| Prisma / Postgres | `audit_log_server_composite_index_present_and_used` | After `prisma migrate deploy`, introspect `pg_indexes WHERE tablename = 'AuditLog'` -- assert an index exists whose definition contains `(entityIdTenant, at DESC)`. Then run `EXPLAIN (FORMAT JSON) SELECT * FROM "AuditLog" WHERE "entityIdTenant" = $1 ORDER BY at DESC LIMIT 50` against a 10k-row fixture -- assert the plan uses the composite index (not a Seq Scan, not a different index). |

### §9.10 P01-G10 -- a11y CI script wired into DoD and coverage (MEDIUM)

- **Source:** phase-01.md §7.11 `pnpm a11y` axe-core
- **Target test section:** §1.3 / §8
- **Category:** Missing Coverage Gate

§7.11 adds a `pnpm a11y` script that runs axe-core against `<AppShell>`. Only a manual NVDA / VoiceOver step appears in §5.1; the automated axe pass is not in the coverage gate table or the §8 DoD checklist. Without wiring, the script may exist but never run in CI.

New §1.3 row:

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src/components/shell/**` (a11y axe rules) | 0 axe violations of severity `serious` or `critical` | `pnpm a11y -- --include="src/components/shell/**" --tags=wcag2a,wcag2aa --severity=serious,critical` |

New §8 DoD checkbox (insert into the coverage-gates block):

- [ ] `pnpm a11y` against `<AppShell>` returns 0 `serious` / `critical` violations (axe-core, WCAG 2.0 AA). Manual NVDA / VoiceOver step in §5.1 reinforces but does not replace the automated run.

### §9.11 P01-G11 -- ProcessedOp daily vacuum behavioural assertions (MEDIUM)

- **Source:** phase-01.md §7.18 ProcessedOp daily vacuum
- **Target test section:** §2.3 / §6.8
- **Category:** Missing Integration Test

§7.18 schedules a daily vacuum at 03:30 local that purges `ProcessedOp` rows older than 30 days and emits exactly one `audit_log` row with `action='vacuum'`. The plan currently asserts the SQL form and audit emission for the local pruner (§6.8) but never the server-side scheduling, the once-per-run audit invariant, or idempotency on re-run within the same day.

New §2.3 rows:

| Route / Job | Test | Asserts |
|-|-|-|
| `ProcessedOpVacuumJob` | `processed_op_vacuum_runs_at_0330_local_with_fake_clock` | Use the test scheduler with `now = 2026-05-13T03:30:00+03:00`; assert the job fires exactly once; assert `ProcessedOp` rows with `processedAt < now - 30d` are deleted; rows within 30d are preserved. |
| `ProcessedOpVacuumJob` | `processed_op_vacuum_emits_exactly_one_audit_row_per_run` | Pre-seed 50 stale rows; trigger the job; assert exactly ONE `audit_log` row with `action='vacuum'`, `entity='processed_op'`, `delta.deleted_count = 50` (not one per deleted row). |
| `ProcessedOpVacuumJob` | `processed_op_vacuum_idempotent_on_same_day_re_run` | Trigger the job at 03:30; trigger again at 03:35 (same local day); assert the second run is a no-op (0 rows deleted, no new `audit_log` row -- the audit row is per-distinct-day, not per-invocation). |

### §9.12 P01-G12 -- Cursor writes go only to SQLite, never to plugin-store (MEDIUM)

- **Source:** phase-01.md §7.3 store carries only UI prefs
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.3 pins that `tauri-plugin-store` carries ONLY UI preferences (locale, theme, sync-server URL) -- sync cursors, outbox state, and `sync_state.device_id` live exclusively in SQLite. A regression that double-writes the cursor (e.g., for "convenience" or "faster reads") silently introduces dual-source-of-truth bugs.

New §2.1 row:

| Scenario | Asserts |
|-|-|
| `cursor_writes_persist_only_to_sqlite_never_to_plugin_store` | After a successful pull that advances the cursor, assert `sync_state.last_pull_cursor` reflects the new value AND `tauri-plugin-store` has NO key matching `/^sync\.|cursor|sync_state/i`. The test inspects the plugin-store's persisted JSON file directly. Mirror assertion on push: outbox state never leaks into the plugin-store. |

### §9.13 P01-G13 -- Server-side parking on push for manual-policy entities (MEDIUM)

- **Source:** phase-01.md §4 manual policy parks on server
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§4 sync-semantics declares that `manual`-policy entities (e.g., phase-02's `settings`) park on the SERVER (creating a `ConflictParked` row) when a push collides. Phase-01 ships the mechanism. The current plan covers local-engine parking on receipt of a conflict envelope (§2.1 `engine_push_parks_outbox_row_when_server_returns_conflict_envelope`) but never the server-side path that produces the envelope.

New §2.3 row:

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_parks_manual_policy_collision_in_conflict_parked_table` | Register a stub `manual`-policy entity name in the policy registry for the test (or use phase-02's `settings` once available). Seed an existing server row at `(version=2, updated_at=T1)`; push a colliding op `(version=2, updated_at=T2)` from another device. Assert: response is `{ accepted: [], conflicts: [{op_id, local_payload, server_payload}] }`; assert one `ConflictParked` row exists with `op_id`, `resolvedAt = null`; assert the underlying server row is UNCHANGED (manual policy does not auto-resolve). |

### §9.14 P01-G14 -- audit_log createMany skipDuplicates idempotency (MEDIUM)

- **Source:** phase-01.md §3 `createMany skipDuplicates`
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§3 declares `/sync/push` writes audit rows via `prisma.auditLog.createMany({ data, skipDuplicates: true })` so a client retry that ships the same `id` lands idempotently without a transaction abort. The current plan covers `ProcessedOp.op_id` idempotency at the envelope level but never the row-level `id`-collision dedup that protects against torn writes between envelope ack and local outbox delete.

New §2.3 row:

| Route | Test | Asserts |
|-|-|-|
| `push_audit_log_create_many_skip_duplicates_dedups_by_id` | Seed an existing `audit_log` row with `id = 'aaaa-...'`. Push a batch containing ops where one references a fresh `audit_log` row whose `id` happens to collide with `'aaaa-...'` (synthetic; simulates a torn retry where the local row was repaired and re-sent under a fresh `op_id` but same audit `id`). Assert: response `accepted` contains the op (the push as a whole succeeded); audit_log row count for that id is exactly 1 (not 2, not an error); the pre-existing row's fields are preserved (skipDuplicates does NOT overwrite). |

### §9.15 P01-G15 -- Both outbox rows enqueued in same tx (MEDIUM)

- **Source:** phase-01.md §4 with_audit step 6
- **Target test section:** §2.1
- **Category:** Missing Edge Coverage

`AuditWriter::with_audit` step 6 enqueues TWO outbox rows on every successful tx: one for the business entity, one for `audit_log`. The current §2.1 asserts the literal step order (`InsertAudit -> BusinessWrite -> EnqueueOutbox`) but never asserts that BOTH outbox rows commit atomically -- a regression that enqueues only the business outbox row would lose audit-sync forever (audit rows would never push).

New §2.1 row in `sync_phase01.rs`:

| Scenario | Asserts |
|-|-|
| `with_audit_enqueues_both_business_and_audit_outbox_rows_in_same_tx` | Run a `with_audit` call against a stub business entity; assert AFTER COMMIT the `outbox` table has EXACTLY two new rows -- one with `entity='<business>'` and one with `entity='audit_log'`; both rows share the same `created_at` (within 1ms); both have `parked=0, attempts=0`. Then force a panic between the two enqueues (instrumented via a feature-gated hook) and assert reopen finds ZERO outbox rows (atomicity holds for the pair). |

### §9.16 P01-G16 -- Husky + lint-staged pre-commit hook validation (LOW)

- **Source:** phase-01.md §7.29 Husky + lint-staged
- **Target test section:** §8
- **Category:** Missing Coverage Gate

§7.29 wires `.husky/pre-commit` to run `pnpm lint-staged` on staged files. The DoD lists "lint passes" but never verifies the HOOK exists and is executable, nor that `lint-staged` is configured. A repo where someone disables husky locally (`HUSKY=0`) drifts unnoticed.

New §8 DoD checkbox:

- [ ] `.husky/pre-commit` exists, is executable (`-rwxr-xr-x`), and invokes `pnpm lint-staged`. `package.json` declares the `lint-staged` config map. Verified by `test/repo/husky-lint-staged.test.ts` running `stat` + `grep` against the committed files (no actual commit performed).

### §9.17 P01-G17 -- Capability lint forbids notification:default (LOW)

- **Source:** phase-01.md §7.23 no `notification:default`
- **Target test section:** §2.1 / §6.7
- **Category:** Missing Integration Test

§7.23 explicitly forbids `notification:default` in capabilities -- IDC's notification surface is in-app toasts only, never OS-level notifications. The capabilities lint test added in P01-G02 covers the allowlist positively; this gap pins the NEGATIVE assertion explicitly so a future "let's add OS notifications" PR is caught at the lint layer, not at security review.

New §2.1 row (co-located with the P01-G02 test in `capabilities_phase01.rs`):

| Scenario | Asserts |
|-|-|
| `capabilities_explicitly_forbid_notification_default` | Parse `src-tauri/capabilities/default.json` AND any per-window `main.json`; assert NO entry matches `notification:default` or `notification:*`. Negative-path: introduce a fixture capabilities file with `notification:default` present and assert the test fails with a message naming the forbidden permission. |

### §9.18 P01-G18 -- focus-visible class assertion on shadcn overrides (LOW)

- **Source:** phase-01.md §7.25 focus-visible overrides
- **Target test section:** §2.4
- **Category:** Missing Edge Coverage

§7.25 requires every shadcn-overridden component (`<Button>`, `<Input>`, `<Select>`, etc.) to carry a `focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2` (or equivalent) Tailwind utility set. The current §6.2 covers RTL layout but never automated-asserts that keyboard-only focus rings remain after Tailwind v4 upgrades or shadcn template syncs.

New §2.4 component test rows (run inside the existing `describe.each([['ltr'], ['rtl']])`):

- `<SyncPill>` button: rendering with `tabIndex=0` and dispatching a keyboard focus event leaves a `focus-visible:ring-*` class on the element; mouse focus does NOT (matches `:focus-visible` semantics).
- `<LanguageToggle>` button: same assertion as above.
- `<SkipToContent>` link: focus-visible utilities present; the link transitions from `sr-only` to visible on focus.
- A unit-style test in `src/test-utils/shadcn-overrides.test.ts` that walks every component in `src/components/ui/` and asserts the rendered className contains at least one `focus-visible:` utility token. Drift on a shadcn sync that drops focus-visible utilities fails this test in CI.

## §10 Gap Analysis Pass 2 Additions

These gaps are sourced from `gap-analysis-pass-2.md` and attach to the same surfaces Pass 1 covered, mirroring the §9 subsection format (Source / Target / Category trio + narrative + concrete test rows). §10.1 (P01-G19) is a tracking entry only: the underlying defect was a wrong-shape compound-PK assertion that has already been amended in place inside §9.7, and §10.1 records that correction for audit purposes.

### §10.1 P01-G19 -- SyncCursor compound PK correction (CRITICAL)

- **Source:** phase-01.md §7.19 SyncCursor compound `@@id` (line 650)
- **Target test section:** §9.7 (amended in place)
- **Category:** Missing Contract Test

Tracking entry. Pass 1 §9.7 originally asserted `SyncCursor` declared `@@id([entityIdTenant, entity])`, which is not what phase-01.md line 650 actually declares. The build spec declares `@@id([deviceId, entityIdTenant])` -- the cursor key is per (device, tenant), not per (tenant, entity). Without this correction the §9.7 contract test would pass green against a Prisma schema that does not match the spec, falsely certifying a non-existent invariant and missing the real regression risk (a single `@id` collapsing cursors across devices). The §9.7 test row has been edited in place: function renamed to `sync_cursor_schema_has_compound_at_at_id_device_id_plus_entity_id_tenant`, the asserted column tuple is now `[deviceId, entityIdTenant]` in that order, and a dated "Corrected" note was appended inside the §9.7 subsection. No new test row lives here in §10.1.

### §10.2 P01-G20 -- metrics_events.kind CHECK vs jwt_pin_offline emitter (CRITICAL)

- **Source:** phase-01.md §7.28 `metrics_events.kind` CHECK constraint, §9.1 emitter
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§9.1 (Pass 1) asserts the JWT public-key pinning path emits a `metrics_events` row with `kind='jwt_pin_offline'` when the offline grace window starts. But §7.28's CHECK list for `metrics_events.kind` enumerates a closed set of literals and `jwt_pin_offline` is not in it -- the INSERT would fail at runtime with a CHECK violation, and §9.1's test as written would either fail in setup or (worse) be authored against a relaxed local schema and pass falsely in CI. No existing test enforces CHECK-vs-emitter symmetry across the union of kinds the codebase emits. A new integration test inventories every `kind=` literal across Rust + TS sources and asserts each appears in the §7.28 CHECK list.

| Scenario | Asserts |
|-|-|
| `metrics_events_kind_check_list_is_superset_of_all_emitted_kinds` | Grep `src-tauri/` and `src/` for `kind:\s*"[a-z_]+"` and `'kind',\s*"[a-z_]+"` patterns to collect the set of emitted kinds (must include `jwt_pin_offline`, `sync_push_success`, `sync_pull_success`, etc.). Read the §7.28 CHECK clause from `src-tauri/migrations/<NNN>_metrics_events.sql`; parse the `IN (...)` list. Assert emitted-kinds set is a subset of CHECK list. Negative: emit a synthetic `kind='__not_in_check__'` row via `sqlx::query!`; assert the SQLite returns SQLITE_CONSTRAINT_CHECK (code 19) with constraint name including `metrics_events_kind_chk`. Per §7.28 + §9.1. |

### §10.3 P01-G21 -- IDC_SYNC_SERVER_URL env override (HIGH)

- **Source:** phase-01.md §7.22 sync server URL configuration
- **Target test section:** §2.1 / §2.2
- **Category:** Missing Integration Test

§7.22 declares two paths to discover the sync server URL: a persisted `tauri-plugin-store` value under `config/syncServerUrl` (covered by §2.2 IPC tests for `config::set_sync_server_url` / `config::get_sync_server_url`) AND an `IDC_SYNC_SERVER_URL` env-var override "for dev." The env-var path has no test row. A regression that ignored the env var (or read it once at module load and never re-read after `SyncEngine::new`) would silently break the dev workflow and only surface in manual smoke. The new test pins precedence and read timing.

| Scenario | Asserts |
|-|-|
| `sync_engine_reads_env_var_override_at_boot_and_takes_precedence_over_store` | Set `IDC_SYNC_SERVER_URL=https://env-override.example.com` in the test process env. Pre-seed the plugin-store with `config/syncServerUrl=https://store-value.example.com`. Boot `SyncEngine::new(SyncConfig::from_runtime())`. Assert the engine's effective base URL is the env-var value (`https://env-override.example.com`). Unset the env var; rebuild the engine; assert it falls back to the store value. Empty env var (`IDC_SYNC_SERVER_URL=""`) is treated as unset (engine uses store). Per §7.22. |

### §10.4 P01-G22 -- Server-side delete-vs-edit tie-break on /sync/push (HIGH)

- **Source:** phase-01.md §7.16 SyncPushService step 5 (delete-vs-edit)
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.16 specifies that when a push batch contains an `op_kind='delete'` AND an `op_kind='update'` for the same row id with equal `updated_at`, the server picks deletion (tie-goes-to-deletion). The Pass 1 plan tests this rule on the engine's pull side (LWW resolver) but never on the server's push acceptance path. A regression that reordered SyncPushService step 5 to prefer the later-in-batch op would break the documented invariant without any test red flag.

| Scenario | Asserts |
|-|-|
| `push_accepts_delete_when_update_and_delete_share_updated_at_for_same_id` | POST `/sync/push` with a batch containing two envelopes for the same `audit_log` row id: one `op_kind='delete'` and one `op_kind='update'`, both with byte-identical `updated_at` timestamps. Order the array (a) delete-then-update and (b) update-then-delete across two runs. In both runs assert the final server row has `deleted_at IS NOT NULL` (deletion wins) and the response `processed[]` lists both `op_ids` as accepted. Per §7.16 step 5. |

### §10.5 P01-G23 -- /sync/lookup-op JWT 401 negative (HIGH)

- **Source:** phase-01.md §7.20 (SyncEngine startup-replay route), §3 server-routes table
- **Target test section:** §2.3 / §6.7
- **Category:** Missing Integration Test

§3 declares "all non-`/healthz` routes require JWT," and §7.20 introduces `POST /sync/lookup-op` as a new lightweight route for boot-time outbox reconciliation. The Pass 1 plan tests the positive path (returns `{ found: [op_ids] }`) but never the negative auth path. An auth-plugin regression that allowed lookup-op through unauthenticated would leak a tenant op-id existence oracle. Standard 401 contract test, but it has to be explicitly written -- the auth-plugin test suite covers a generic protected route, not this one.

| Scenario | Asserts |
|-|-|
| `lookup_op_rejects_request_without_bearer_token_with_401` | POST `/sync/lookup-op` with body `{ op_ids: ["op-1","op-2"] }` and NO `Authorization` header. Assert response 401, body `{ code: 'unauthorized', message: <string>, traceId: <non-empty> }`, no `Set-Cookie`, no leaked op-id info in the message. Repeat with malformed bearer (`Authorization: Bearer not.a.real.jwt`), expired bearer, and bearer signed by the wrong key; all four return 401 with the same envelope shape. Per §7.20 + §3. |

### §10.6 P01-G24 -- RedactionLayer patient_name and email regex (HIGH)

- **Source:** phase-01.md §7.14 RedactionLayer
- **Target test section:** §1.1 / §2.1
- **Category:** Missing Edge Coverage

§7.14 declares the tracing RedactionLayer regex matches `password|token|hash|patient_name|email` field names and substitutes `[REDACTED]` in the log line. Pass 1 tests cover `password|token|hash` (the auth-flavoured trio) but never `patient_name` or `email`, which are PHI-flavoured -- exactly the class the regex was added for. A regression that dropped `patient_name` from the alternation would silently leak names into log files. The new tests exercise the missed alternation branches and assert structural redaction (key remains, value is masked).

| Scenario | Asserts |
|-|-|
| `redaction_layer_masks_patient_name_value_in_log_line` | Emit `tracing::info!(patient_name = "Ali Hassan", id = "p-1", "lookup")`. Capture the rendered log line via a `tracing_subscriber::fmt` test writer. Assert the rendered line contains `patient_name=[REDACTED]` (or `patient_name="[REDACTED]"` depending on formatter) and does NOT contain the literal `"Ali Hassan"`. The `id="p-1"` field is unredacted. Per §7.14. |
| `redaction_layer_masks_email_value_in_log_line` | Emit `tracing::info!(email = "ali@example.com", "login_attempt")`. Captured line contains `email=[REDACTED]` and does NOT contain the literal `ali@example.com`. Per §7.14. |

### §10.7 P01-G25 -- ErrorResponseSchema traceId presence (HIGH)

- **Source:** phase-01.md §7.26 ErrorResponseSchema
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.26 declares the shared `ErrorResponseSchema` TypeBox shape `{ code, message, details, traceId }` and mandates every error response on every server route conforms to it. Existing §3.1 Swagger contract tests assert the shape against the schema reference, but no test asserts `traceId` is actually populated (non-null, non-empty string) on EVERY error path -- a TypeBox `Type.String()` would accept `""` schema-wise but break log correlation in practice. The new test inventories every error-return on every route and asserts a non-empty `traceId` is present in the body and matches an `x-trace-id` response header.

| Scenario | Asserts |
|-|-|
| `every_error_response_carries_non_empty_trace_id_matching_header` | Drive a synthesized 4xx and 5xx response from each Pass 1 route (`/auth/login` with bad creds -> 401; `/sync/push` with malformed envelope -> 422; `/sync/pull` with revoked token -> 401; `/sync/lookup-op` with no token -> 401; force 500 via a test-only `/__throw` route guarded by env flag). For each response: assert body matches `ErrorResponseSchema` via Ajv, assert `body.traceId` is a non-empty string of length >= 8, assert `response.headers['x-trace-id'] === body.traceId`. Per §7.26 + §3.1 row for `ErrorResponseSchema`. |

### §10.8 P01-G26 -- i18n namespace files load via i18next (MEDIUM)

- **Source:** phase-01.md §7.10 i18n bootstrap
- **Target test section:** §2.1 / §6.2
- **Category:** Missing Integration Test

§7.10 declares the i18n bootstrap loads namespaced JSON files at `src/i18n/locales/{ar,en}/{common,errors,receipts}.json` (six files total). The Pass 1 plan tests RTL layout swap and locale toggle behaviour but never asserts the six files exist on disk AND parse as JSON AND are registered with i18next under the documented namespace ids. A missing or empty file at install time would render `key.like.this` strings in the UI instead of translated copy -- ugly but easy to miss in a quick smoke. The new test pins both filesystem presence and i18next registration.

| Scenario | Asserts |
|-|-|
| `i18n_loads_all_six_namespace_files_for_ar_and_en` | For each `(lang, ns) in {ar,en} x {common,errors,receipts}` (six combinations): assert `fs.existsSync('src/i18n/locales/' + lang + '/' + ns + '.json')`; assert `JSON.parse(fs.readFileSync(...))` returns a non-empty object; after `i18next.init(...)` resolves, assert `i18next.hasResourceBundle(lang, ns)` returns true and `i18next.getResourceBundle(lang, ns)` returns the same parsed object. Negative: rename one file in a fixture sandbox and assert init throws a recognisable error (not a silent fallback). Per §7.10. |

### §10.9 P01-G27 -- /sync/pull omits pulledAt server-only field (MEDIUM)

- **Source:** phase-01.md §7.32 `pulledAt` server-only column
- **Target test section:** §2.3 / §3.1
- **Category:** Missing Contract Test

§7.32 declares `pulledAt` is a server-only column on `SyncCursor` (and on per-entity row metadata where applicable) used for server-side bookkeeping; PRD §9 mandates it MUST NEVER appear in `/sync/pull` response payloads to clients. No current contract test asserts the omission. A schema regression that added `pulledAt` to the response type would leak server-side scheduling state to clients and shift the canonical envelope hash.

| Scenario | Asserts |
|-|-|
| `pull_response_omits_pulled_at_field_on_every_row_and_envelope` | Seed the server with 50 `audit_log` rows. POST `/sync/pull` with a valid cursor; receive the response. Assert (a) the top-level envelope object has no `pulledAt` key, (b) every row in `rows[]` has no `pulledAt` key, (c) the response validates against the declared TypeBox `SyncPullResponseSchema` via Ajv in `strict` mode (additional-properties false), so an injected `pulledAt` would fail validation. Per §7.32 + PRD §9. |

### §10.10 P01-G28 -- tauri.conf.json bundle env IDC_EMBEDDED_MODE=0 (MEDIUM)

- **Source:** phase-01.md §7.35 `tauri.conf.json` bundle env
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.35 declares that shipped builds (`bundle.windows.env` and `bundle.macOS.env` in `tauri.conf.json`) MUST set `IDC_EMBEDDED_MODE=0` so the desktop binary boots in standalone mode rather than Business OS embedded mode. A bundle config that forgot this key, or set it to `1`, would ship a binary that polls for a non-existent BOS parent and never advances past the splash screen -- a fatal regression that no integration or E2E test currently guards.

| Scenario | Asserts |
|-|-|
| `tauri_conf_bundle_env_declares_idc_embedded_mode_zero_for_all_platforms` | Read `src-tauri/tauri.conf.json` via `serde_json`. Assert `bundle.windows.env.IDC_EMBEDDED_MODE === "0"` and `bundle.macOS.env.IDC_EMBEDDED_MODE === "0"` (string `"0"`, not the integer `0`, per Tauri env conventions). Negative: temporarily flip one to `"1"` in a fixture copy and assert the test fails. Per §7.35. |

### §10.11 P01-G29 -- axe-core on /login and /no-access routes (MEDIUM)

- **Source:** phase-01.md §7.11 a11y baseline
- **Target test section:** §1.3 / §6.7
- **Category:** Missing Edge Coverage

§7.11 specifies the a11y baseline runs axe-core against the routes `/login` and `/no-access`. Pass 1 §9.10 wired axe-core into CI but the configured scope walks `components/shell/**` (the chrome) rather than the two route surfaces named in the spec. Route-level a11y is therefore unverified: a `<form>` on `/login` missing labels, or `/no-access` rendering its message inside a non-landmark `<div>`, would not trip the existing CI gate.

| Scenario | Asserts |
|-|-|
| `axe_run_on_login_route_has_zero_violations` | Render `<Login>` (the route component) inside a `MemoryRouter` set to `/login`; pass the rendered container to `axe.run()` with rules `wcag2a, wcag2aa, wcag21aa`. Assert `result.violations.length === 0`. Test runs once in `dir=ltr` and once in `dir=rtl`. |
| `axe_run_on_no_access_route_has_zero_violations` | Same harness as above for `<NoAccess>` at `/no-access`. Assert zero violations under the same rule set, both directions. Per §7.11. |

### §10.12 P01-G30 -- outbox.parked column schema (MEDIUM)

- **Source:** phase-01.md §7.17 outbox parking column
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.17 added the `parked INTEGER NOT NULL DEFAULT 0 CHECK (parked IN (0,1))` column to the local `outbox` table; the partial retry index (`... WHERE attempts < 10 AND parked = 0`) relies on it. Pass 1 tests the parking BEHAVIOUR (conflict-on-push sets parked=1) but never asserts the column-level schema invariants. A migration that dropped the CHECK or changed the default would let `parked = 99` rows leak in, breaking the index predicate and corrupting the retry loop.

| Scenario | Asserts |
|-|-|
| `outbox_parked_column_has_correct_default_and_check_constraint` | Open a freshly migrated SQLite DB via `sqlx`. Query `PRAGMA table_info('outbox')`; find the `parked` row; assert `type='INTEGER'`, `notnull=1`, `dflt_value='0'`. Query `SELECT sql FROM sqlite_master WHERE name='outbox'`; assert the captured DDL contains `CHECK (parked IN (0, 1))` (allow whitespace variance). Negative: attempt `INSERT INTO outbox (... parked) VALUES (..., 2)` via raw `sqlx::query!`; assert it returns SQLITE_CONSTRAINT_CHECK error code 19. Per §7.17. |

### §10.13 P01-G31 -- ErrorResponseSchema canonical-shape snapshot (LOW)

- **Source:** phase-01.md §7.26 ErrorResponseSchema
- **Target test section:** §3.3 / §10 (snapshot rules)
- **Category:** Missing Snapshot

§7.26 declares the shared `ErrorResponseSchema` (`{ code, message, details, traceId }`) as the single error envelope every server route returns. §3.3 commits to hash-locked snapshots of canonical envelopes (push, pull) but no snapshot file exists for the canonical *error* envelope. Every phase's error paths inherit this shape; a serializer-side change that reordered keys, dropped `details`, or renamed `traceId` to `trace_id` would not trip any current contract test until a downstream phase noticed broken correlation. The fix is one new snapshot fixture and one DoD checkbox.

| Snapshot file | Asserts |
|-|-|
| `expected/sync/error-response-canonical.json.sha256` | Hash of the canonicalized JSON for a synthesized `ErrorResponseSchema` instance: `{ code: 'unauthorized', message: 'token expired', details: { reason: 'exp' }, traceId: '00000000-0000-0000-0000-000000000001' }`. Computed via the shared canonicalizer (sorted keys, no whitespace) used for push/pull snapshots. Committed alongside the existing phase-01 sync snapshots. §8 DoD grows one row: `[ ] expected/sync/error-response-canonical.json.sha256 (NEW for this phase -- canonical error envelope)`. Per §7.26 + §3.3 snapshot rules. |

### §10.14 P01-G32 -- shadcn override files present (LOW)

- **Source:** phase-01.md §7.25 shadcn overrides
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.25 names four specific shadcn override files (`button.tsx`, `icon-button.tsx`, `link.tsx`, `tabs.tsx`) under `src/components/ui/` that carry the project's design-token-aware variants and `focus-visible` utilities. Pass 1 §9.18 walks `src/components/ui/` generically and asserts every component carries a `focus-visible:` utility, but it never asserts the four NAMED files exist -- a `shadcn add` regression that overwrote one of them with the upstream default, or a refactor that renamed `icon-button.tsx` to `iconButton.tsx`, would slip past §9.18 (the directory walk would still find SOMETHING to assert against).

| Scenario | Asserts |
|-|-|
| `shadcn_override_files_present_at_documented_paths` | For each `file in ['button.tsx','icon-button.tsx','link.tsx','tabs.tsx']`: assert `fs.existsSync('src/components/ui/' + file)`; assert the file exports a React component (presence of `export function` or `export const` matching the component name); assert the file contains at least one `focus-visible:` Tailwind utility token in its source text (reinforcing §9.18 at the path level). Per §7.25. |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 7 Phase-01 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P01-G33 through P01-G39). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P01-G33 -- Six §5 plugin registrations beyond fs (HIGH)

- **Source:** phase-01.md §5 -- "Plugin registrations: tauri-plugin-sql, tauri-plugin-store, tauri-plugin-stronghold, tauri-plugin-os, tauri-plugin-dialog, tauri-plugin-log, tauri-plugin-fs".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§9.3 asserted `tauri-plugin-fs` registration; the other six §5 plugins are silently uncovered. A regression dropping `.plugin(stronghold::Builder::new(...).build())` from `lib.rs::run` compiles cleanly and breaks at the first stronghold call -- after the user has typed their password.

| Scenario | Asserts |
|-|-|
| `lib_rs_registers_all_seven_phase01_plugins` | Spin up the Tauri test harness; introspect the running app's plugin registry (via `app.handle().plugin_registry()` or by calling `app.handle().get_state::<sql::Sql>().is_some()` style probes per plugin). Assert all seven names appear: `sql`, `store`, `stronghold`, `os`, `dialog`, `log`, `fs`. A simpler static-analysis variant is acceptable: grep `src-tauri/src/lib.rs` for `.plugin(tauri_plugin_<name>::Builder::default().build())` for each of the seven names; either form closes the gap. |

### §11.2 P01-G34 -- AuditLog server composite indexes (3 total) (HIGH)

- **Source:** phase-01.md §7.33 -- `@@index([entityIdTenant, at(sort: Desc)])`, `@@index([entityIdTenant, entity, entityId, at])`, `@@index([entityIdTenant, actorUserId, at])`.
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§9.9 covers only the descending-`at` index. The other two indexes drive phase-08's entity-filtered and actor-filtered audit-page queries; without them, the audit page seq-scans at production scale.

| Scenario | Asserts |
|-|-|
| `audit_log_server_composite_indexes_all_three_present` | After `prisma migrate deploy` on the test Postgres: `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'AuditLog'`. Assert the result set contains all three composite indexes named in §7.33; for each, parse `indexdef` and confirm column order matches the declaration. Run a representative query for each index (`WHERE entity_id_tenant=$1 AND entity=$2 AND entity_id=$3 ORDER BY at DESC LIMIT 50`; `WHERE entity_id_tenant=$1 AND actor_user_id=$2 ORDER BY at DESC LIMIT 50`) under `EXPLAIN (ANALYZE, BUFFERS)`; assert each reports an `Index Scan` (NOT `Seq Scan`) using the matching index name. |

### §11.3 P01-G35 -- SyncEngine shutdown sequence (MEDIUM)

- **Source:** phase-01.md §4 SyncEngine step 5 -- "Shutdown: cancel via CancellationToken; drain in-flight HTTP; persist cursor to sync_state".
- **Target test section:** §2.1 / §6.5
- **Category:** Missing Integration Test

A leaked tokio task or an unflushed cursor on Ctrl+C silently regresses the engine's lifecycle contract.

| Scenario | Asserts |
|-|-|
| `sync_engine_shutdown_drains_http_and_persists_cursor` | Start the engine; trigger a long-running push (mock server holds the response for 2s); call `engine.shutdown()` mid-push. Assert: (a) the push completes (response received) before `shutdown().await` returns -- the engine drains, it does not abort; (b) `sync_state.last_pushed_cursor` reflects the post-drain cursor value (not the pre-push value); (c) `tokio_metrics::TaskMonitor` shows zero alive tasks tagged `sync-engine`; (d) a second `engine.shutdown()` is a no-op (idempotent). Per §4 SyncEngine step 5. |

### §11.4 P01-G36 -- SyncEngine boot subscribes to network status (MEDIUM)

- **Source:** phase-01.md §4 SyncEngine boot step 1 -- "Subscribe to network status (online/offline transitions) so the engine flips `<SyncPill>` state and pauses/resumes loops".
- **Target test section:** §2.1 / §6.3
- **Category:** Missing Integration Test

Without a test, the subscription could be silently dropped; state would only update on explicit IPC.

| Scenario | Asserts |
|-|-|
| `sync_engine_boot_subscribes_and_flips_state_on_network_transition` | Boot the engine with a mock `NetworkStatusProvider` initially online. Trigger an offline transition; assert (within 100ms): (a) engine state transitions to `offline`; (b) push loop is paused (a queued op stays in outbox, not attempted); (c) `<SyncPill>` event payload reflects offline. Trigger online transition; assert push loop resumes (queued op attempted within 200ms). The subscription MUST happen during boot, not on first IPC call -- assert by emitting an online->offline transition BEFORE any IPC has been invoked, and observe the state flip. |

### §11.5 P01-G37 -- ConflictResolveService merged-upsert happy path (MEDIUM)

- **Source:** phase-01.md §4 ConflictResolveService step 4 -- "if choice='merge', validate `merged` payload against entity schema then apply as a forced upsert with resolvedAt + resolvedByUserId".
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§2.3 tests only the negative branch (`resolve_conflict_merged_requires_valid_payload`). The success branch is untested.

| Scenario | Asserts |
|-|-|
| `resolve_conflict_merge_applies_forced_upsert_and_marks_resolved` | Park a `visits` conflict (`policy=manual`). POST `/sync/conflicts/{opId}/resolve { choice: 'merge', merged: <valid_visit_payload> }` (validated against `VisitPushSchema`). Assert: (a) response 200; (b) `Visit` row in Postgres matches the merged payload byte-for-byte (`updated_at`, `version`, all carried fields); (c) `ConflictParked` row's `resolvedAt` is non-null and within 1s of now; `resolvedByUserId` matches the JWT subject; (d) `audit_log` row written with `action='conflict_resolve'` and `delta.choice='merge'` in the SAME `prisma.$transaction` as the upsert. Per §4 ConflictResolveService step 4. |

### §11.6 P01-G38 -- ConflictResolveService audit emission at service layer (MEDIUM)

- **Source:** phase-01.md §4 ConflictResolveService step 6 -- "Always emit an audit_log row with action='conflict_resolve' regardless of choice".
- **Target test section:** §2.3 / §2.1
- **Category:** Missing Integration Test

§2.3 row `resolve_conflict_keep_local_marks_parked_resolved` defers audit emission to phase-08. But §4 puts the emission in phase-01's service. Phase-08 owns the UI; phase-01 owns the service-layer audit write.

| Scenario | Asserts |
|-|-|
| `resolve_conflict_writes_audit_row_in_same_tx_for_every_choice` | For each of `choice in ['local', 'server', 'merge']`: park a conflict, resolve it, assert exactly one new `audit_log` row exists with `action='conflict_resolve'`, `entity` matches the parked entity, `entity_id` matches the parked entity id, `delta.choice` matches the input choice, `actor_user_id` matches the JWT subject, `at` is within 1s of now. The audit row MUST live in the SAME `prisma.$transaction` as the resolution write -- assert by injecting a tx-rollback after the resolution write and confirming neither the `audit_log` row nor the `ConflictParked.resolvedAt` mutation persists. Per §4 ConflictResolveService step 6. |

### §11.7 P01-G39 -- audit_log duplicate-id ordering by created_at (LOW)

- **Source:** phase-01.md §6 verification step 8 -- "additive-only push of audit_log with duplicate `id`: accepts both and orders by `created_at`".
- **Target test section:** §2.3 / §6.4
- **Category:** Missing Edge Coverage

§9.14 has the server use `createMany skipDuplicates` (one row survives). The build spec verification step 8 claims "accepts both and orders by `created_at`". The two are in tension. Reconcile before authoring.

| Scenario | Asserts |
|-|-|
| `audit_log_duplicate_id_resolution_matches_documented_invariant` | Two clients push two `audit_log` rows with the same `id` but distinct `origin_device_id` and `created_at` values (1s apart). Read server-side state after both pushes settle. Assert ONE of two outcomes per the resolved build-spec invariant (reconcile before authoring): (a) `skipDuplicates` semantics -- exactly one row exists, the first by arrival; OR (b) "accepts both" semantics -- two rows exist (which requires composite PK, not single-`id`), pulled in `created_at ASC` order. Update phase-01 build spec §6 step 8 or §7.31 to pin one interpretation; the test asserts that interpretation. Per §6 step 8 vs §9.14 reconciliation. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 3 Phase-01 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P01-G40 through P01-G42). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P01-G40 -- X-Device-Id and X-App-Version headers on push (HIGH)

- **Source:** phase-01.md §4 SyncEngine push step 2.2 -- "POST /sync/push with batch and `X-Device-Id`, `X-App-Version`, `Authorization: Bearer <accessToken>` headers".
- **Target test section:** §2.1 / §2.3
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `sync_push_emits_x_device_id_and_x_app_version_headers` | Mount a mock HTTP server. Trigger a push. Assert the request carries `X-Device-Id: <value matching sync_state.device_id>` AND `X-App-Version: <value matching Cargo.toml package.version>`. Rename either to `X-Tauri-Device` or drop one -> test fails. Server-side mirror: in §2.3, assert `/sync/push` reads `originDeviceId` from `X-Device-Id` (NOT body). Per §4 step 2.2. |

### §12.2 P01-G41 -- /sync/lookup-op cross-tenant negative (MEDIUM)

- **Source:** phase-01.md §6.7 + §7.20 -- "a token for tenant A cannot look up op_ids belonging to tenant B. Asserted in §2.3."
- **Target test section:** §2.3
- **Category:** Missing Edge Coverage

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/lookup-op` | `lookup_op_does_not_leak_cross_tenant_op_ids_existence` | Seed `ProcessedOp { opId='B-1', entityIdTenant='B' }` server-side. Authenticate as tenant A; POST `{ opIds: ['B-1'] }`. Assert response `{ found: [] }` -- not just `{ found: ['B-1'] }` with stripped data, but the op MUST be entirely absent (no existence oracle). Per §6.7 + §7.20. |

### §12.3 P01-G42 -- SyncPill count badge tnum + Geist Mono (LOW)

- **Source:** phase-01.md §7.4 + design-system §5.4 -- count badge uses Geist Mono with `font-feature-settings: 'tnum'`.
- **Target test section:** §2.4
- **Category:** Missing Edge Coverage

| Hook / Component | Test | Asserts |
|-|-|-|
| `<SyncPill>` (`describe.each([['ltr'],['rtl']])`) | `sync_pill_badge_uses_tabular_numerals_and_geist_mono` | Render `<SyncPill>` with a `pendingCount={42}`. Locate the badge element. Assert `getComputedStyle(badge).fontFeatureSettings` includes `'tnum'` AND `getComputedStyle(badge).fontFamily` includes `'Geist Mono'`. Render with `pendingCount={888}` -- the digit width remains stable (no layout shift). Per design-system §5.4 + testing.md §14 anti-pattern. |
