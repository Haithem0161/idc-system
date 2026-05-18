# Phase 09: Pre-Ship Hardening -- Test Plan

**Proves:** v0.1.0 actually ships. The Prisma-backed sync + auth stores survive container restart (the BLOCKER fix per §9 audit provenance -- replacing the in-memory `MemorySyncStore` + `MemoryUserStore` with `PrismaEntityRepo` + `PrismaUserStore` against the existing 19-model schema), the JWT plugin refuses to boot in production without a real `JWT_PUBLIC_KEY` (no `'dev-only-secret'` fallback per §3 auth-jwt rewrite), `/healthz` reports real `db` / `redis` / `migrationsApplied` state instead of hard-coded `'ok'` (per §3 healthz wiring), `Dockerfile.dev` + `docker-compose.yaml` bring up a real Postgres + sync-server stack that survives `docker compose restart sync-server` with all rows preserved, `init-custom-sql.sql` applies all phase-03/05/06 raw-SQL pieces idempotently after `prisma db push` (paired partial unique indexes from phase-03 §7.20 + §7.21, `inventory_adjustments` BEFORE-UPDATE trigger from phase-05 §7.33, `inventory_adjustments` per-reason delta-sign CHECK from phase-06 §7.14, `visits` 7-name-snapshot CHECK from phase-05 §7.53), manual conflict resolution emits a `conflict_resolve` audit_log row in the same Prisma `$transaction` as the resolve commit (the phase-08 §1 gap closure -- audit-first invariant honored server-side too), `MemoryUserStore.rotate` raw `Error` throws become `DomainError` 401s for the global error handler, `console.log` + MVP `defaultValue` + brittle `unreachable!()` + stale phase-04 comments cleaned up per the 2026-05-12 pre-ship audit. The `.env.template` schema accurately enumerates all server runtime env vars (`JWT_PUBLIC_KEY`, `JWT_SECRET`, `BOOTSTRAP_SUPERADMIN_EMAIL`, etc.) and `@fastify/env` validates at boot.

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server). Heavy emphasis on the sync server -- this is where the BLOCKERs from the pre-ship audit live.
**Dependencies (other test plans):** Phase 01 test (sync plumbing -- the Prisma swap MUST preserve every phase-01 invariant: idempotency on op_id, audit-first ordering server-side, additive-only for audit_log, conflict envelope shape, ProcessedOp dedup), Phase 02 test (auth -- the user-store swap MUST preserve every phase-02 invariant: refresh-token rotation atomicity, password_hash never on the wire, role-gate enforcement), Phase 03/04/05/06/07/08 tests (every entity's `acceptPush` invariant MUST hold under Prisma; the LWW + manual + additive policies all must survive the swap; the raw-SQL pieces from each phase MUST apply via `init-custom-sql.sql`).

**Test Data:**
- Factories (Rust): inherited from earlier phases; the cleanup edits in §3 Tauri/Rust don't add any new factories.
- Factories (TS): inherited.
- Factories (Sync server): the existing factories swap from constructing `MemorySyncStore` to a `PrismaClient` against a `testcontainers` Postgres. The factory shapes don't change; only the persistence layer.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` loaded into Postgres via Prisma (mirror of the SQLite fixture); this verifies cross-surface fixture parity.
- Edge fixtures: `fixtures/edge/jwt-misconfigured/` (NEW for §6.7) -- env vars that should trigger boot refusal; `fixtures/edge/empty-database/` for verifying first-boot creates schema.

**Tool prerequisites:**
- Inherited from phase-01..08 execution.
- Docker: `docker` + `docker compose` (NEW -- first phase to require Docker; CI runners must have docker available). `testcontainers-postgres` (NEW Node dev-dep, `pnpm --filter sync-server add -D @testcontainers/postgresql`).
- `@fastify/env` (NEW server-side dep, `pnpm --filter sync-server add @fastify/env` per §3 env schema).
- `prom-client` already in phase-08.
- `psql` CLI for the `init-custom-sql.sql` smoke test (host-installed or via docker image).

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`.
- Audit conflict-resolution audit row content variants beyond what phase-08 already verified -- phase-09 verifies only that the row IS written server-side in the same Prisma tx, not the contents.
- Performance tuning -- per §8 phase-09 scope: "Performance tuning beyond what naturally happens when the in-memory store is replaced by Postgres with the existing indexes." Phase-09 verifies NO regression from in-memory baseline, not absolute targets (those are owned by `performance-soak.md` + each phase plan).
- Multi-tenant deployment topology -- per §8.
- Self-updater wiring -- Business OS owns child-app updates.
- BullMQ / Redis introduction -- §8 deferral. Phase-09's healthz probe reports `redis: 'ok'` when `REDIS_URL` unset.
- New schema / entities / IPC commands -- §8 + §1/§2 NO changes.

**Cross-phase commands:** none. Phase-09 ships zero new IPC commands. It MODIFIES behavior at 4 Rust files (per §3 Tauri/Rust) and rewrites the server's plugin wiring, but the public IPC surface is unchanged.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

Phase 09 makes 4 surgical edits in `src-tauri/src/`:
1. `domains/inventory/service/mod.rs:282` -- `unreachable!()` -> `Err(AppError::Internal(...))`.
2. `domains/catalog/service/operator_service.rs:222` -- delete the "phase-04 hardens..." forward-reference comment (cascade is the documented policy).
3. `domains/catalog/service/operator_service.rs:2` -- delete the stale module-level doc-comment.
4. `lib.rs:135-155` -- replace 5 `eprintln!` with `tracing::info!`, gated behind `IDC_EMBEDDED_MODE`.

| Module | Test | Asserts |
|-|-|-|
| `inventory::service::mod` -- create_adjustment with reason=ConsumeVisit constructor switch | `consume_visit_in_construct_switch_returns_internal_error_not_panic` | Per §3: bypass the early-return guard via a test feature flag; assert the constructor returns `Err(AppError::Internal("ConsumeVisit reached construction switch after early-return guard"))`. NO panic. The test confirms the foot-gun is closed without removing the dead-code path. |
| `inventory::service::mod` -- create_adjustment happy paths | `consume_visit_early_return_guard_still_works_unchanged` | The L224-L228 early-return is preserved; a normal `consume_visit` adjustment flows through `Visit::lock` -> `InventoryAdjustment::try_consume_visit` (per phase-05 / phase-06), never reaching the L282 switch. Regression test: phase-05 + phase-06 test suites still pass. |
| `catalog::service::operator_service::soft_delete` | `cascades_specialties_per_documented_policy_not_phase04_forward_reference` | Per §3 option (a): cascade is the documented behavior. The test asserts the operator + all its `operator_specialties` are soft-deleted in one tx, and that the doc-comment / inline comment NO LONGER references phase-04. (The comment cleanup is verified by a `grep` check in CI -- see §6.8.) |
| `lib::run::embedded_mode_log_messages` | `emit_via_tracing_info_when_idc_embedded_mode_is_1` | Set `IDC_EMBEDDED_MODE=1`; capture the `tracing` subscriber output; assert 5 INFO-level events with the embedded-mode banner content. NO `eprintln!` output (asserted by capturing stderr and verifying empty for the banner). |
| `lib::run::embedded_mode_log_messages` | `silent_when_idc_embedded_mode_unset_or_zero` | Unset / `IDC_EMBEDDED_MODE=0`; capture INFO events; assert ZERO events related to embedded-mode banner. Standalone mode stays quiet. |

### §1.2 TS pure functions / value objects

Phase 09 makes 4 frontend edits:
1. `src/providers/auth-provider.tsx:88` -- remove `console.log("[AuthProvider] /api/auth not reachable...")`.
2. `src/pages/admin/inventory/detail.tsx:77` -- replace inline English `defaultValue` with i18n key.
3. `src/components/setup/first-launch-setup.tsx:80` -- ensure `setup.subtitle` exists in both locales (no JSX change; verification only).
4. `src/components/shell/sidebar.tsx:152` -- confirm/cleanup the "Coming soon" disabled item.

| Module | Test | Asserts |
|-|-|-|
| `src/providers/auth-provider.tsx` | `does_not_emit_console_log_on_offline_phase_set` | Spy on `console.log`; trigger the `/api/auth not reachable` code path; assert `console.log` NOT called for that message. `console.error` on real failure paths is preserved (separate test). |
| `src/providers/auth-provider.tsx` | `still_emits_console_error_on_unexpected_errors` | Verify the cleanup didn't accidentally remove the legitimate error logging. |
| `src/pages/admin/inventory/detail.tsx` | `consumption_subtype_picker_uses_i18n_key_not_defaultValue` | Render component with locale `ar`; assert displayed string matches `t('admin.inventory.consumption_subtype_picker')`; NEVER the English `defaultValue`. |
| `src/pages/admin/inventory/detail.tsx` | `consumption_subtype_picker_present_in_en_and_ar_locales` | Verify the i18n key resolves in both locales; phase-08 §7.9 `pnpm lint:i18n` would catch a missing key but this is a sanity check at the unit level. |
| `src/components/setup/first-launch-setup.tsx` | `setup_subtitle_renders_from_i18n_in_both_locales` | Per §3 verification step 9: the modal subtitle is `t('setup.subtitle')`; both locales have the key. |
| `src/components/shell/sidebar.tsx` | `coming_soon_item_state_consistent_with_design_decision` | Either: (a) the disabled item exists with both i18n keys present and proper a11y attributes (`aria-disabled="true"`); OR (b) the item is removed entirely. The test pins whichever decision lands; phase-09 §3 leaves the choice open. |

### §1.3 Coverage targets

Phase 09 ships behavioral changes in existing files; coverage gates inherit from each owning phase. The deltas:

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/inventory/service/mod.rs` (the L282 + early-return path) | inherits phase-06 >= 90% | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::inventory::service` |
| `src-tauri/src/domains/catalog/service/operator_service.rs` | inherits phase-03 >= 90% | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::catalog::service` |
| `src-tauri/src/lib.rs::run` embedded-mode branch | >= 95% (covers both env-var-set and unset paths) | `cargo llvm-cov --lib --fail-under-lines 95 -- lib::run` |
| `src/providers/auth-provider.tsx` | >= 90% | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/providers/auth-provider.tsx"` |
| `src/pages/admin/inventory/detail.tsx`, `src/components/setup/first-launch-setup.tsx`, `src/components/shell/sidebar.tsx` | inherits phase-02 / phase-03 / phase-01 >= 60% (presentation) | -- |
| `sync-server/src/app/plugins/prisma.ts` (NEW per §3) | >= 95% (critical wiring) | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/plugins/env.ts` (NEW per §3) | >= 95% (boot-time validator) | -- |
| `sync-server/src/app/plugins/auth-jwt.ts` (rewrite per §3) | >= 95% | -- |
| `sync-server/src/app/plugins/sync-services.ts` (rewrite per §3) | >= 90% | -- |
| `sync-server/src/app/plugins/auth-services.ts` (rewrite per §3) | >= 90% | -- |
| `sync-server/src/app/routes/healthz.ts` (rewrite per §3) | >= 95% | -- |
| `sync-server/src/app/sync/infrastructure/prisma/audit-repo.ts`, `processed-op-repo.ts`, `sync-cursor-repo.ts`, `conflict-parked-repo.ts`, `entity-repo.ts` (NEW per §3) | >= 90% | -- |
| `sync-server/src/app/auth/infrastructure/prisma/user-store.ts` (NEW per §3) | >= 90% | -- |
| `sync-server/src/app/sync/service/conflict-service.ts` (audit-emission gap closure per §3) | >= 95% | -- |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/preship_phase09.rs` (NEW; tracks the 4 Rust cleanups + lib.rs embedded-mode logging).

| Scenario | Asserts |
|-|-|
| `inventory_create_adjustment_consume_visit_via_lock_workflow_unchanged_per_phase_05_and_06` | The phase-05 lock workflow's consume_visit path still works end-to-end; the L282 dead-code path is reachable only via a test-feature flag; the cleanup didn't change the happy path. Regression assertion against phase-05 + phase-06 scenarios. |
| `inventory_create_adjustment_internal_error_when_consume_visit_reaches_switch_under_test_feature_flag` | Per §1.1: feature flag `force-reach-construct-switch-with-consume-visit` -> `Err(AppError::Internal)`; NO panic; NO `unreachable_unchecked` UB. |
| `operator_soft_delete_cascades_specialties_per_documented_policy` | Per §1.1: cascade is the documented behavior; verifies phase-03 §7.22 still holds. Regression against phase-03 tests. |
| `lib_run_emits_embedded_mode_banner_via_tracing_info_when_env_var_is_1` | Set `IDC_EMBEDDED_MODE=1`; spawn `lib::run`; capture INFO events; assert 5 expected banner messages with `tracing::info!` provenance. |
| `lib_run_silent_when_env_var_unset_or_zero` | -- |
| `lib_run_does_not_emit_to_stderr_when_embedded_mode_enabled` | Capture stderr while `IDC_EMBEDDED_MODE=1`; assert empty for banner content (it goes to `tracing`, not `eprintln!`). |
| `grep_test_no_phase04_forward_reference_in_operator_service_rs` | A test that `grep -E "phase-04|phase 04|phase 4" src-tauri/src/domains/catalog/service/operator_service.rs` returns ZERO matches. The cleanup is permanent. |
| `grep_test_no_eprintln_in_lib_rs_for_embedded_mode_banner` | A test that `grep -E "eprintln!" src-tauri/src/lib.rs` -- if matches exist, they must be for non-banner content (e.g., legitimate early-boot before tracing is initialised). The banner-specific lines are removed. |

### §2.2 Tauri IPC handler tests

Phase 09 adds NO new IPC commands. The existing IPC surface is regression-tested by re-running the full phase-01..08 IPC matrix and confirming green. Phase-09 adds no new IPC integration tests.

| Command | Asserts |
|-|-|
| (regression) all 91 IPC commands across phases 01-08 | All return the same shapes and error envelopes after the Rust cleanups. `cargo test` baseline green; no behavior drift. |

### §2.3 Sync server route handlers

This is the heart of phase-09: the BLOCKER fixes. File: `sync-server/test/preship/persistence-phase09.test.ts` (NEW) + `sync-server/test/preship/jwt-enforcement-phase09.test.ts` (NEW) + `sync-server/test/preship/healthz-phase09.test.ts` (NEW) + `sync-server/test/preship/conflict-audit-phase09.test.ts` (NEW) + `sync-server/test/preship/env-schema-phase09.test.ts` (NEW).

DB: real Prisma test DB via `testcontainers` Postgres 16-alpine per §5 + §7 Postgres pinning. The Memory* stores stay in the codebase as test-only fixtures; they are NEVER instantiated in production paths.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `push_doctor_persists_to_postgres_via_prisma_entity_repo` | Push a doctor row through `/sync/push`; assert it lives in `prisma.doctor.findUnique`. Same shape as phase-03 §2.3; this verifies the swap preserves the contract. |
| `persistence-phase09.test.ts` | `push_doctor_survives_container_restart` | The real test: push -> `docker compose restart sync-server` (via a test-only docker management hook) -> reconnect -> pull -> doctor row still present. THIS IS THE BLOCKER FIX. |
| `persistence-phase09.test.ts` | `processed_op_cache_survives_container_restart_idempotent` | Push op X; restart; replay op X; assert the cached response returns (no double-write). Phase-01 idempotency invariant under Prisma. |
| `persistence-phase09.test.ts` | `sync_cursor_survives_container_restart_pull_resumes_correctly` | Pull batch 1; restart; pull batch 2 -- the cursor resumed from the persisted value. Per phase-01 §7.19 composite PK. |
| `persistence-phase09.test.ts` | `conflict_parked_survives_container_restart_resolver_can_load` | Force a parked conflict; restart; GET `/sync/conflicts` (phase-08 §7.11) returns the row. |
| `persistence-phase09.test.ts` | `refresh_token_survives_container_restart_can_still_refresh` | Login; restart; refresh -- new token issued (rotation atomic). Phase-02 §7.5 invariant. |
| `persistence-phase09.test.ts` | `lww_helper_centralised_in_prisma_entity_repo_per_3_4_lww_helper` | Push two updates to the same doctor with identical `updatedAt` + different `originDeviceId`; verify lex-smaller wins. Phase-03 §7.17 invariant. |
| `persistence-phase09.test.ts` | `additive_only_audit_log_push_persists_with_phase_01_invariants_preserved` | Audit log additive policy preserved under Prisma. |
| `persistence-phase09.test.ts` | `manual_conflict_policy_on_visits_parks_under_prisma_per_phase_05_7_19` | A manual-policy conflict on `visits` parks via the Prisma repo; the `ConflictParked` row matches the phase-05 §7 envelope. |
| `persistence-phase09.test.ts` | `init_custom_sql_applies_partial_unique_indexes_from_phase_03_7_20_and_7_21` | After `prisma db push` + `psql -f init-custom-sql.sql`, attempting two `(doctor, check_type, NULL)` pricing rows -> the second fails the partial unique index. Same for `inventory_consumption_map` per phase-03 §7.21. |
| `persistence-phase09.test.ts` | `init_custom_sql_applies_inventory_adjustments_before_update_trigger_from_phase_05_7_33` | After init, an UPDATE on `inventory_adjustments.delta` -> RAISE(ABORT). The trigger is loaded. |
| `persistence-phase09.test.ts` | `init_custom_sql_applies_inventory_adjustments_delta_sign_check_from_phase_06_7_14` | After init, an INSERT with `(reason='receive', delta=-5)` -> CHECK violation. |
| `persistence-phase09.test.ts` | `init_custom_sql_applies_visits_name_snapshot_check_from_phase_05_7_53` | After init, an INSERT of a locked visit with NULL name snapshot -> CHECK violation. |
| `persistence-phase09.test.ts` | `init_custom_sql_idempotent_on_replay` | Run `psql -f init-custom-sql.sql` twice in a row; second run succeeds (every CREATE uses IF NOT EXISTS where applicable, or wraps in `DO $$ ... EXCEPTION ... $$`). |
| `jwt-enforcement-phase09.test.ts` | `refuses_boot_in_production_without_jwt_public_key` | Set `NODE_ENV=production`; unset both `JWT_PUBLIC_KEY` and `JWT_SECRET`; spawn server; assert process exits with non-zero AND log contains "JWT plugin: production requires JWT_PUBLIC_KEY (RS256)". |
| `jwt-enforcement-phase09.test.ts` | `accepts_hs256_dev_fallback_with_warning_when_node_env_not_production_and_jwt_secret_above_32_chars` | Per §3 auth-jwt rewrite: dev mode with a 32+ char `JWT_SECRET` boots with a `warn` log line. |
| `jwt-enforcement-phase09.test.ts` | `refuses_boot_when_jwt_secret_below_32_chars_in_dev` | Per §3: `JWT_SECRET="short"` -> boot refuses. |
| `jwt-enforcement-phase09.test.ts` | `verifies_rs256_signed_jwt_against_public_key_in_production_mode` | Pre-seed RS256 key pair; sign a token with the private key; verify against the public key path; assert acceptance. |
| `jwt-enforcement-phase09.test.ts` | `rejects_token_signed_with_wrong_key_with_401` | -- |
| `jwt-enforcement-phase09.test.ts` | `no_silent_dev_only_secret_fallback_in_any_mode` | A boot with `JWT_PUBLIC_KEY` unset AND `JWT_SECRET` unset MUST refuse, regardless of `NODE_ENV`. No `'dev-only-secret'` constant in the codebase (CI grep). |
| `healthz-phase09.test.ts` | `returns_status_ok_when_db_reachable_per_3_healthz_wiring` | DB probe via `prisma.$queryRaw\`SELECT 1\`` returns ok. |
| `healthz-phase09.test.ts` | `returns_db_fail_when_db_unreachable` | Kill the DB container; `/healthz` body has `db: 'fail'`, `status: 'fail'`, HTTP 200. Per §3. |
| `healthz-phase09.test.ts` | `returns_redis_ok_when_redis_url_unset` | Per §3: when `REDIS_URL` is unset, `redis: 'ok'` (interpreted as "not configured"). |
| `healthz-phase09.test.ts` | `returns_redis_fail_when_redis_url_set_but_unreachable` | -- |
| `healthz-phase09.test.ts` | `migrations_applied_field_reflects_actual_migration_table_state` | Per §3: probe checks for the `_prisma_migrations` table or equivalent. |
| `healthz-phase09.test.ts` | `version_field_is_0_1_0` | Per §3: pinned. |
| `conflict-audit-phase09.test.ts` | `manual_conflict_resolve_writes_audit_log_row_in_same_prisma_transaction_per_3_audit_emission` | Force a manual conflict; resolve via POST `/sync/conflicts/:opId/resolve`; assert in ONE `prisma.$transaction`: (a) the `ConflictParked` row's `resolvedAt` is set AND (b) a new `AuditLog` row with `action='conflict_resolve'` exists. If either step fails the entire tx rolls back. |
| `conflict-audit-phase09.test.ts` | `audit_row_delta_contains_choice_op_id_resolve_op_id` | Per §3: `delta: { choice, opId, resolveOpId: input.resolveOpId ?? null }`. |
| `conflict-audit-phase09.test.ts` | `audit_row_actor_user_id_matches_resolving_superadmin_jwt_sub` | -- |
| `conflict-audit-phase09.test.ts` | `audit_row_entity_id_tenant_matches_resolved_parked_entity_id_tenant` | -- |
| `conflict-audit-phase09.test.ts` | `local_audit_log_pulls_the_new_row_after_resolve_per_3_server_canonical_until_next_pull` | Resolve on server; from a client, pull; verify the new audit_log row arrives locally. |
| `conflict-audit-phase09.test.ts` | `audit_row_rolled_back_when_conflicts_resolve_tx_fails` | Force `conflicts.resolveTx` to error; assert NO audit row written; the conflict stays parked. |
| `conflict-audit-phase09.test.ts` | `conflict_resolve_idempotent_on_resolve_op_id_per_phase_08_7_22` | Replay -> cached response; NO double audit row. |
| `env-schema-phase09.test.ts` | `at_fastify_env_fails_fast_on_missing_database_url` | Unset `DATABASE_URL`; assert boot fails with a clear error referencing the missing env var. |
| `env-schema-phase09.test.ts` | `at_fastify_env_fails_fast_on_missing_node_env_in_production_check_path` | -- |
| `env-schema-phase09.test.ts` | `env_template_listed_keys_exactly_match_runtime_reads` | Per §3: enumerate every `process.env.X` read in `sync-server/src/`; cross-reference with `.env.template`; assert sets match. |
| `env-schema-phase09.test.ts` | `memory_user_store_rotate_now_throws_domain_error_401_not_raw_error` | Per §3 error-handler reach: the legacy memory path (kept as test fixture) now throws `DomainError('AUTH_INVALID_REFRESH', 401)` not bare `Error`. Verifies the cleanup. |
| `env-schema-phase09.test.ts` | `sync_store_env_var_comment_removed_per_4_sync_store_env_var` | The mentioned-but-never-existed `SYNC_STORE=memory|prisma` env var is gone from the codebase. CI grep. |

### §2.4 React Query mutation / query flows

Phase 09's frontend edits are surgical and tested at unit level (§1.2). No new React Query hook flows.

| Component | Test | Asserts |
|-|-|-|
| `<AdminInventoryDetail>` | `consumption_subtype_picker_renders_i18n_resolved_string_in_both_locales` | -- |
| `<FirstLaunchSetupModal>` | `subtitle_renders_i18n_resolved_in_both_locales` | -- |
| `<AppShell>` `<Sidebar>` | `coming_soon_item_state_consistent` | Pinned by §1.2 test. |

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Phase 09 adds NO new server routes. The contract surface change:
- `/healthz` `HealthSchema.status` widens to `'ok' | 'fail'` per §3 healthz wiring.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /healthz` (response, after §3 widening) | `HealthSchema` extended | Captured both `'ok'` and `'fail'` responses; validates against the union. |
| (regression) all existing schemas | Inherited from phase-01..08 | The Prisma swap MUST preserve every contract. Phase-09 regression-runs the contract suite. |

### §3.2 IPC shape contract

Phase 09 adds NO new IPC commands and modifies NO existing return shapes. Regression-test only.

| IPC command | Status |
|-|-|
| (regression) all 91 commands from phases 01-08 | Inherited; contract unchanged. |

### §3.3 Sync envelope contract

Phase-09 §3.3 closes the canonical wire-shape snapshot set promoted from the v0.1.0 ship audit. Each snapshot is a paired `<name>.json` (or `.txt`) + `<name>.json.sha256` under `sync-server/test/expected/snapshots/`. The drift gate is bi-directional: `Value.Check(<Schema>, parsed)` catches server-side schema changes that lack a sample regen, and the SHA-256 hash catches stealth sample edits that lack a deliberate hash regen.

- **Test harness:** `sync-server/test/contract/canonical-snapshots.test.ts` (32 tests).
- **Snapshot files** -- 13 wire shapes per the §3.3 brief, listed by category:

  Healthz response (pre-existing, retained):
  - `expected/healthz/healthz-ok-canonical.json.sha256`
  - `expected/healthz/healthz-fail-canonical.json.sha256`

  Pre-ship env template (pre-existing, retained):
  - `expected/preship/env-template-canonical.txt.sha256` -- byte-hash of the canonical `.env.template` per §3 env schema; drift indicates the template fell out of sync with runtime reads.

  PushBody samples (`PushBodySchema`):
  - `expected/snapshots/patient-push.json` (+`.sha256`)
  - `expected/snapshots/visit-push-locked.json` (+`.sha256`)
  - `expected/snapshots/visit-push-voided.json` (+`.sha256`)
  - `expected/snapshots/inventory-adjustment-push.json` (+`.sha256`)
  - `expected/snapshots/operator-shift-push.json` (+`.sha256`)
  - `expected/snapshots/operator-shift-soft-delete.json` (+`.sha256`)

  PullResponse samples (`PullResponseSchema`):
  - `expected/snapshots/visit-pull-row.json` (+`.sha256`)
  - `expected/snapshots/operator-shift-pull.json` (+`.sha256`)

  Audit query response (`AuditQueryResponseSchema`):
  - `expected/snapshots/audit-query-response-mixed-50-row.json` (+`.sha256`) -- exercises all 14 action values + tri-state `ip` per the "exercises 14 distinct actions" invariant.

  Conflict resolver shapes:
  - `expected/snapshots/conflict-list-response-canonical.json` (+`.sha256`) -- two open conflicts (version-conflict + manual-policy), both `resolved_at=null` per phase-08 §7.11 open-only contract.
  - `expected/snapshots/conflict-resolve-applied-response.json` (+`.sha256`) -- canonical `{ok:true, status:"applied"}`.
  - `expected/snapshots/conflict-resolve-already-resolved-response.json` (+`.sha256`) -- 409 `ErrorResponseSchema` with `code="ALREADY_RESOLVED"` + `details.resolvedAt` (load-bearing for the 409 UI).

  Prometheus exposition (plaintext; hash-only -- no TypeBox schema):
  - `expected/snapshots/prometheus-exposition-sample.txt` (+`.sha256`) -- all 10 named metrics from `src/app/plugins/metrics.ts` MetricsRegistry + `outbox_depth_gauge` with tenant label.

  Regeneration is explicit (edit the JSON, recompute the hash, commit both); the README under `expected/snapshots/` documents the one-liner.

---

## §4 E2E Tests (Pyramid Layer 4)

Specs live under `e2e/specs/preship/`. The defining E2E for phase-09 is the **container-restart persistence smoke test**.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `prisma-persistence-survives-container-restart.e2e.ts` | Mariam | 1) `docker compose up -d`. 2) Bootstrap superadmin via env. 3) Login; create a doctor; lock a visit. 4) `docker compose restart sync-server`. 5) Login again; pull. | Doctor + visit + audit rows all present. THE BLOCKER FIX. |
| `healthz-reports-real-state.e2e.ts` | (no user; curl) | 1) `curl localhost:3161/healthz`. 2) Kill DB container. 3) Re-curl. | First returns `{ status: 'ok', db: 'ok' }`; second returns `{ status: 'fail', db: 'fail' }`. HTTP 200 in both cases. Per §3 healthz wiring. |
| `manual-conflict-resolve-writes-server-audit-row.e2e.ts` | Mariam | 1) Force manual conflict on visits. 2) Mariam resolves on Device A. 3) Wait for sync. 4) Device B pulls. | Device B's local audit log has the `conflict_resolve` row. Per §3 audit-emission. |
| `bootstrap-superadmin-via-prisma-seed.e2e.ts` | First boot | 1) `BOOTSTRAP_SUPERADMIN_EMAIL=admin@idc.iq BOOTSTRAP_SUPERADMIN_PASSWORD=... pnpm prisma db seed`. 2) Spawn server. 3) Login with bootstrap creds. | Login succeeds; the user is a superadmin. The seed is idempotent (running twice doesn't create a second admin). Per §3 §7 of phase-02 §7.21. |
| `i18n-key-resolves-not-defaultValue.e2e.ts` | Mariam | 1) Open `/admin/inventory/<id>`. 2) Try to add a consumption row against a `has_subtypes=true` parent. 3) Verify the error message in both locales. | The text comes from `t('admin.inventory.consumption_subtype_picker')`; NOT the inline English `defaultValue`. Per §3 §9 verification step 9. |
| `console-log-removed-from-auth-provider.e2e.ts` | (no user) | 1) Open Devtools. 2) Reload app. | The `console.log("[AuthProvider] /api/auth not reachable...")` is GONE. `console.error` on real failure paths is preserved. |
| `embedded-mode-banner-via-tracing-not-eprintln.e2e.ts` | (no user; CLI) | 1) Set `IDC_EMBEDDED_MODE=1`. 2) Run the app. 3) Capture stdout + stderr + tracing log. | INFO-level events from `tracing`; nothing on stderr for the banner. |

### §4.2 Failure-path flows

- **`jwt-secret-fallback-removed-production-refuses-boot.e2e.ts`** -- `NODE_ENV=production` with `JWT_PUBLIC_KEY` and `JWT_SECRET` both unset; assert container exits non-zero. The Docker compose `command` reports the literal error from `auth-jwt.ts`.
- **`jwt-secret-32-char-minimum-enforced-in-dev.e2e.ts`** -- Set `JWT_SECRET="short"`; boot refuses.
- **`init-custom-sql-failure-during-startup-blocks-readiness.e2e.ts`** -- Force a syntax error in `init-custom-sql.sql` (test fixture); container fails to become ready; `/healthz` never returns ok.
- **`env-validation-fails-fast-on-missing-database-url.e2e.ts`** -- Unset `DATABASE_URL`; @fastify/env errors at boot.
- **`conflict-resolve-tx-rollback-on-audit-failure.e2e.ts`** -- Force the audit insert to fail; assert the conflict resolve also rolls back; the row stays parked. Per §3 audit-emission tx semantics.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-survives-server-restart-mid-push.e2e.ts` | Device A starts a push of 50 ops; sync-server restart mid-push (test-only docker hook); Device A retries; ops eventually drain. | All 50 ops applied exactly once (idempotency on `op_id` via `ProcessedOp` table). |
| `two-device-conflict-resolve-audit-row-propagates-to-peer.e2e.ts` | Device A resolves a conflict; Device B pulls. | Device B's audit log has the `conflict_resolve` row. Per §3. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Persistence smoke test (manual).** Per §6 step 7 ("the real test"):
  1. `docker compose up -d`.
  2. Bootstrap superadmin.
  3. Push a doctor + lock a visit + adjust inventory.
  4. `docker compose restart sync-server`.
  5. Login again. Verify all data survived.
- **Healthz manual probe.** `curl http://localhost:3161/healthz` -- verify body + HTTP 200 in healthy and degraded states.
- **JWT production refusal manual.** Spin up the compose stack with `NODE_ENV=production` and no `JWT_PUBLIC_KEY`; verify the container's stderr shows the refusal message.
- **init-custom-sql idempotency manual.** Run `psql -f init-custom-sql.sql` twice; both succeed without errors.
- **`.env.template` audit.** Open the file; cross-reference against `process.env.X` reads in the codebase; both sides match.
- **Frontend visual: i18n keys resolve.** Mariam navigates `/admin/inventory/<id>` and `/setup/first-run` in both locales; verifies NO English `defaultValue` ever shows.

### §5.2 Cross-references to `personas.md`

- `personas.md` -> **P3 Mariam the Superadmin** -> the entire day-script must continue to pass after the Prisma swap; this is the regression gate. Required for §8 DoD.
- All other personas (P1, P2, P4, P5) must pass unchanged. Reinforcement.

**Canonical: P3 Mariam the Superadmin.** P3 MUST pass for §8 DoD to flip to `complete`. Phase-09's gate is "everything still works" after the swap + cleanups.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **N/A -- no new time-handling code.** Phase-09 preserves every phase-01..08 time invariant; regression-tested by re-running their suites.
- **JWT `iat` / `exp` UTC.** Per phase-02 §6.1: preserved after the JWT rewrite.
- **Refresh-token `expiresAt` Postgres `timestamptz`.** Per phase-02 §6.1: preserved after the user-store swap.

### §6.2 i18n & RTL

- **N/A -- owned by phase-08 (i18n-rtl cross-cutting + lint scripts) and each phase's surface tests.** Phase-09's 3 i18n key edits ensure no inline-English `defaultValue`s leak through. Verified in §1.2 + §4.1.
- **`pnpm lint:i18n` still passes after phase-09 edits.** Phase-08's lint script gates this; phase-09 verifies no regression.

### §6.3 Offline & Network

- **Container-restart resilience.** Per §4.1 + §4.3: server restart mid-push leaves no orphan state; client outbox resumes via `ProcessedOp` idempotency.
- **Server unreachable.** Existing phase-01 behaviors preserved (pill goes offline, outbox queues).
- **Real Postgres vs in-memory under network drops.** Postgres handles connection pooling via Prisma's built-in pool; assert no leaks under repeated drop / reconnect.

### §6.4 Concurrency & Conflicts

- **All 3 conflict policies preserved under Prisma.** `additive-only` (audit_log + inventory_adjustments), `last-write-wins` (every catalog entity + users), `manual` (visits + settings). Phase-09 verifies via regression suite + the manual-conflict + audit-emission integration test.
- **Conflict resolve idempotency under network drops.** Per phase-08 §7.22 + §1.1 helper carry-over.
- **LWW centralised in `PrismaEntityRepo.lwwShouldApply`.** Per §3 §4 LWW helper. Asserted in `persistence-phase09.test.ts`.

### §6.5 Crash & Recovery

- **Container kill mid-tx.** Postgres rolls back the active tx; on restart, no half-applied state. Asserted by killing the container during a `/sync/push` and verifying no partial rows.
- **Prisma connection pool recovery.** Per `prisma.ts` plugin's `onClose` hook: graceful disconnect on shutdown; clean re-connect on restart.
- **Conflict resolve rollback under audit-emit failure.** Per §3 + §2.3.
- **init-custom-sql failure blocks readiness.** Per §4.2.

### §6.6 Scale & Performance

- **No regression from in-memory baseline.** Per §8 phase-09 scope. Compare push throughput / pull latency before-vs-after swap; degradation > 20% is a regression. Asserted in `perf_no_regression_after_prisma_swap`.
- **Real Postgres index usage.** Verify the indexes declared in phases 01-08 are honored: `audit_log_tenant_at`, `inventory_items_low_stock`, etc. (`EXPLAIN ANALYZE` in Postgres mirroring the SQLite `EXPLAIN QUERY PLAN`).
- **Prisma N+1 audit.** Phase-09 verifies no accidental N+1 introduced by the swap. Test scenarios: push 50 audit ops in a batch -> Prisma should batch the INSERTs.

### §6.7 Security & Permissions

- **JWT production refusal.** Per §3 + §2.3 + §4.2: no `'dev-only-secret'` ever silently active. CI grep blocks reintroduction.
- **JWT pinning carry-over.** Per phase-02 §7.10: client-side pinning unaffected by the server JWT rewrite.
- **Postgres connection security.** `DATABASE_URL` uses `sslmode=prefer` or higher in production (documented in §3 env schema; phase-09 verifies the template).
- **Bootstrap secrets posture.** Per §7 §4: production must inject via host env, not compose file. Phase-09 documents but doesn't enforce at runtime; CI lint enforces (`grep` against `docker-compose.yaml` for hardcoded `BOOTSTRAP_SUPERADMIN_PASSWORD`).
- **`.env` not git-tracked.** Per §3 env schema: CI grep `git ls-files sync-server/.env` returns empty.
- **`/metrics` `X-Internal-Token` gate preserved.** Per phase-08 §7.17.
- **Tenant isolation under Prisma.** Every `prisma.<model>.findMany` query carries `where: { entityIdTenant: request.tenantId }` (or the model's equivalent field). Asserted by inspecting the `PrismaEntityRepo` source via a static analysis test.

### §6.8 Data Integrity

- **init-custom-sql applies all phase-03/05/06 raw-SQL pieces.** Per §2.3 + §4.1.
- **Paired partial unique indexes work under Postgres.** Per phase-03 §7.20 + §7.21: blocking duplicates is the integrity invariant.
- **Append-only trigger on `inventory_adjustments` works under Postgres.** Per phase-05 §7.33.
- **Per-reason delta-sign CHECK works under Postgres.** Per phase-06 §7.14.
- **Visits 7-name-snapshot CHECK works under Postgres.** Per phase-05 §7.53.
- **Migration ordering preserved.** Per phase-03 §7.31 + phase-05 §7.51: `prisma migrate deploy` applies files in lex order; phase-09 doesn't change the order.
- **Schema parity SQLite vs Postgres.** Every column declared in phases 01-08 exists on both surfaces. Asserted via a `pnpm prisma validate` + a Rust test that introspects `sqlite_master` and compares against Prisma schema.
- **Grep cleanup tests.** Per §2.1: `phase-04` forward-reference grep returns zero; `eprintln!` banner grep returns zero; `dev-only-secret` grep returns zero; `SYNC_STORE` env-var-comment grep returns zero.

---

## §7 Performance SLOs (this phase's surfaces)

Phase 09's perf gate is regression-only -- no new SLOs, just verification that the Prisma swap doesn't degrade existing thresholds.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Sync server (Postgres) | `/sync/push` 50-op batch (regression vs in-memory baseline) | within 20% of in-memory baseline | -- | `perf_no_regression_push_50_op_batch` | Allows Postgres overhead; flags any unexpected degradation. |
| Sync server (Postgres) | `/sync/pull` 100-row page (regression) | within 20% of in-memory baseline | -- | `perf_no_regression_pull_100_rows` | -- |
| Sync server (Postgres) | `/auth/login` round-trip (regression) | within 20% of in-memory baseline | -- | `perf_no_regression_login` | -- |
| Sync server (Postgres) | `/auth/refresh` round-trip (regression) | within 20% of in-memory baseline | -- | `perf_no_regression_refresh` | -- |
| Sync server (Postgres) | `/sync/conflicts/:opId/resolve` round-trip (now writes audit row in same tx) | < 500 ms p95 (phase-08 baseline) | -- | `perf_resolve_with_audit_under_500ms` | The extra audit insert adds a few ms; budget preserved. |
| Sync server (Postgres) | `/healthz` with real DB + Redis probes | < 50 ms p95 | -- | `perf_healthz_with_real_probes` | The probes add ~5-10 ms over the hardcoded version. |
| Sync server (Postgres) | Server boot time with `prisma.$connect` + `init-custom-sql.sql` | < 5 s p95 (cold container start) | -- | `perf_server_cold_boot_under_5s` | -- |
| Container | `docker compose restart sync-server` to readiness | < 10 s p95 | -- | `perf_container_restart_to_ready` | -- |

---

## §8 Definition of Done

- [ ] All §1 unit tests green.
- [ ] All §2 integration tests green:
  - `cargo test --test preship_phase09` + regression on every previous phase's integration tests.
  - `pnpm --filter sync-server test -- preship/` (persistence + jwt-enforcement + healthz + conflict-audit + env-schema).
  - `vitest run --project integration`.
- [ ] All §3 contract tests green (regression).
- [ ] All §4 E2E tests green on linux-x86_64; multi-device specs green.
- [ ] §5 persona script **P3 Mariam the Superadmin** passes; all other personas (P1, P2, P4, P5) pass as regression gates.
- [ ] §6 all eight edge categories addressed.
- [ ] §7 perf-regression budget honored across every row.
- [ ] Coverage gates met per §1.3.
- [ ] No open P0 or P1 defects in `defects.md`.
- [ ] All 6 BLOCKERs from §9 audit provenance addressed:
  - [ ] Sync routes wired to `PrismaSyncStore` (no longer `MemorySyncStore` in prod paths).
  - [ ] Auth routes wired to `PrismaUserStore`.
  - [ ] No `'dev-only-secret'` fallback in any code path.
  - [ ] `healthz` reports real `db` / `redis` / `migrationsApplied` state.
  - [ ] `Dockerfile.dev` + `docker-compose.yaml` ship and pass §4.1 smoke test.
  - [ ] Manual conflict resolution writes `conflict_resolve` audit row in same Prisma `$transaction`.
- [ ] All 5 SHIP-CONCERNs from §9 addressed:
  - [ ] `.env.template` matches runtime env reads exactly.
  - [ ] `memory-user-store.ts` throws `DomainError` 401s.
  - [ ] `console.log` removed from `auth-provider.tsx`.
  - [ ] MVP `defaultValue` replaced in `admin/inventory/detail.tsx`.
  - [ ] `setup.subtitle` i18n key present in both locales.
- [ ] All 3 NITs from §9 addressed:
  - [ ] `unreachable!()` in `inventory/service/mod.rs:282` swapped for `Err(AppError::Internal)`.
  - [ ] Stale phase-04 comment in `operator_service.rs` removed.
  - [ ] `eprintln!` in `lib.rs` swapped for `tracing::info!`.
- [x] Snapshot files committed:
  - `expected/healthz/healthz-ok-canonical.json.sha256`
  - `expected/healthz/healthz-fail-canonical.json.sha256`
  - `expected/preship/env-template-canonical.txt.sha256`
  - `expected/snapshots/patient-push.json.sha256`
  - `expected/snapshots/visit-push-locked.json.sha256`
  - `expected/snapshots/visit-push-voided.json.sha256`
  - `expected/snapshots/inventory-adjustment-push.json.sha256`
  - `expected/snapshots/operator-shift-push.json.sha256`
  - `expected/snapshots/operator-shift-soft-delete.json.sha256`
  - `expected/snapshots/visit-pull-row.json.sha256`
  - `expected/snapshots/operator-shift-pull.json.sha256`
  - `expected/snapshots/audit-query-response-mixed-50-row.json.sha256`
  - `expected/snapshots/conflict-list-response-canonical.json.sha256`
  - `expected/snapshots/conflict-resolve-applied-response.json.sha256`
  - `expected/snapshots/conflict-resolve-already-resolved-response.json.sha256`
  - `expected/snapshots/prometheus-exposition-sample.txt.sha256`
- [ ] CI guardrail in place: `test "$(git ls-files sync-server/.env)" = ""` per §5.
- [ ] CI grep checks pass: no `phase-04` forward-ref, no banner `eprintln!`, no `'dev-only-secret'`, no `SYNC_STORE` env-var comment.
- [ ] `pnpm prisma validate` clean.
- [ ] `pnpm prisma migrate status` clean (or `prisma db push --accept-data-loss` documented as the dev path per §7.2).
- [ ] Persistence smoke test green: `docker compose restart sync-server` preserves all rows.
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green:
  - `pnpm lint && pnpm build`
  - `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
  - `cd sync-server && pnpm lint && pnpm typecheck && pnpm test`

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P3 Mariam the Superadmin** | -- | -- | -- | Phase-09 must not break anything. P3 is the broadest gate. |
| P1 Asma the Accountant (reinforcement) | -- | -- | -- | Regression gate. |
| P2 Mehdi the Receptionist (reinforcement) | -- | -- | -- | Regression gate. |
| P4 Two-Device Conflict (reinforcement) | -- | -- | -- | Verifies the conflict-resolve audit row propagates server-canonical -> client. |
| P5 Year-End Audit (reinforcement) | -- | -- | -- | Regression gate. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P09-G01 -- Autoload dependency ordering (CRITICAL)

- **Source:** phase-09.md §3 sync-services rewrite (autoload deps)
- **Target test section:** §2.3
- **Category:** Missing Integration Test

The Prisma swap depends on `prisma.ts` being decorated onto the Fastify instance BEFORE `auth-services` and `sync-services` resolve `fastify.prisma`. Without `fp(..., { dependencies: ['prisma'] })` on the dependent plugins, autoload order is alphabetical and the dependents boot against `undefined`. The build spec mandates the dependency array; the test plan currently asserts only the end-to-end persistence outcome.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `autoload_resolves_prisma_plugin_before_auth_services_and_sync_services` | Boot Fastify with autoload; intercept plugin-registration order via `fastify.printPlugins()` (or a spy on `fp` registration timestamps); assert `prisma` appears strictly before `auth-services` and `sync-services`. |
| `persistence-phase09.test.ts` | `auth_services_declares_prisma_in_fastify_plugin_dependencies_array` | Static check on the source: `fp(plugin, { name: 'auth-services', dependencies: ['prisma'] })`. Symmetrical assertion for `sync-services`. Fails if either dependency array is missing or omits `'prisma'`. |
| `persistence-phase09.test.ts` | `boot_fails_fast_when_prisma_plugin_missing_for_dependent` | Remove `prisma.ts` from the autoload directory in a test fixture; assert Fastify boot rejects with the missing-dependency error (per `@fastify/autoload` + `fastify-plugin` contract), NOT with a runtime `undefined.prisma` access. |

### §9.2 P09-G02 -- Prisma onClose graceful disconnect (CRITICAL)

- **Source:** phase-09.md §3 prisma.ts onClose hook
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

The `prisma.ts` plugin must register `fastify.addHook('onClose', async () => prisma.$disconnect())` so graceful shutdowns close the connection pool. Container-restart E2E tests SIGKILL the process and never exercise this path; without a dedicated test, the hook can rot silently and connection leaks emerge only under sustained restart cycles in production.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `prisma_disconnect_runs_on_fastify_on_close_hook` | Spy on `prisma.$disconnect`; call `await fastify.close()`; assert `$disconnect` was invoked exactly once before `close` resolves. |
| `persistence-phase09.test.ts` | `on_close_hook_registered_via_prisma_plugin_not_app_level` | Inspect the plugin's registered hooks; assert the `onClose` listener is owned by the `prisma` plugin (encapsulated), not bolted onto the root instance. |
| `persistence-phase09.test.ts` | `graceful_shutdown_completes_within_5s_under_active_query` | Start a long-running `prisma.$queryRaw` SELECT; trigger `fastify.close()`; assert `$disconnect` resolves within 5s and the in-flight query is either drained or rejected cleanly (no hung handles). |

### §9.3 P09-G03 -- Refresh token survives restart (CRITICAL)

- **Source:** phase-09.md §6 step 7 persistence round-trip
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

The defining persistence E2E (`prisma-persistence-survives-container-restart.e2e.ts`) verifies doctor + visit + audit rows survive `docker compose restart sync-server`, but never asserts the auth path. If `refresh_tokens` lives only in `MemoryUserStore` post-swap, every session dies on restart -- a regression that the current spec would miss because the persona re-logs-in fresh.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `prisma-persistence-survives-container-restart.e2e.ts` (extended) | Mariam | 1) Login; capture `refreshToken` cookie. 2) `docker compose restart sync-server`. 3) Without re-logging-in, POST `/auth/refresh` with the captured token. | New access token issued; the prior refresh token is rotated (revoked); subsequent reuse of the OLD refresh token returns 401. The whole flow proves the open-session row survived the Postgres round trip. |

### §9.4 P09-G04 -- resolve vs resolveTx API split (CRITICAL)

- **Source:** phase-09.md §3 ConflictParkedRepository resolve / resolveTx split
- **Target test section:** §2.3 conflict-audit-phase09
- **Category:** Missing Integration Test

`ConflictParkedRepository` exposes two methods: `resolve(opId, ...)` for standalone use and `resolveTx(tx, opId, ...)` for use inside an existing `prisma.$transaction`. The `ConflictResolveService` MUST use `resolveTx` so the parked-row update and the `conflict_resolve` audit insert commit atomically. Calling `resolve` from inside the service bypasses the transaction and silently breaks the audit-emission invariant.

| Test File | Test | Asserts |
|-|-|-|
| `conflict-audit-phase09.test.ts` | `conflict_resolve_service_invokes_resolve_tx_not_standalone_resolve` | Spy on both `resolve` and `resolveTx`; run a manual-conflict resolve; assert `resolveTx` was called with the active transaction client and `resolve` was NOT called. |
| `conflict-audit-phase09.test.ts` | `resolve_tx_runs_inside_prisma_transaction_with_audit_insert` | Capture the Prisma tx client passed to `resolveTx`; assert the same client is used for the subsequent `auditLog.create`; assert both rows commit at the same `xmin` snapshot (single tx). |
| `conflict-audit-phase09.test.ts` | `standalone_resolve_method_kept_for_test_fixtures_only` | Static-analysis check: production source under `sync-server/src/app/sync/service/` never imports `resolve` (only `resolveTx`); `resolve` is referenced solely from `sync-server/test/` fixtures. |

### §9.5 P09-G05 -- HealthSchema TypeBox widened (HIGH)

- **Source:** phase-09.md §3 HealthSchema widen
- **Target test section:** §3.1
- **Category:** Missing Integration Test

§3.1 lists rows for both `ok` and `fail` responses against `HealthSchema`, but neither asserts the schema itself was edited. The schema must change from `Type.Literal('ok')` on `status` to `Type.Union([Type.Literal('ok'), Type.Literal('fail')])`. A response-only test passes whether the schema declares the union or stays narrow (because a narrow schema rejecting `'fail'` would fail validation at a different layer).

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /healthz` (schema-level assertion) | `HealthSchema` widened | Compile-time + runtime: introspect the TypeBox schema for the `status` property; assert it is a `Union` of two `Literal` schemas with values `'ok'` and `'fail'`. Reject any narrower shape (e.g., a leftover `Type.Literal('ok')`). |
| `GET /healthz` (Ajv validation) | `HealthSchema` widened | Construct a `{ status: 'fail', ... }` payload by hand; run `ajv.compile(HealthSchema)(payload)`; assert it returns `true`. Same for `'ok'`. A pre-widening schema fails the `'fail'` case. |

### §9.6 P09-G06 -- Memory* fixtures never instantiated in prod (HIGH)

- **Source:** phase-09.md §3 Memory* test-only fixtures
- **Target test section:** §2.3
- **Category:** Missing Integration Test

The swap leaves `MemorySyncStore` and `MemoryUserStore` in the codebase as test fixtures, but production paths must never construct them. A static-analysis test scanning `sync-server/src/` (excluding `sync-server/test/`) catches any accidental reintroduction during refactors.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `production_source_never_imports_memory_sync_store` | `grep -rE "new MemorySyncStore|from .*memory-sync-store" sync-server/src/ --exclude-dir=test` returns ZERO matches. Symmetrical assertion for `MemoryUserStore`. |
| `persistence-phase09.test.ts` | `memory_stores_only_referenced_from_test_fixtures` | Positive control: the same grep against `sync-server/test/` returns at least one match per store (proves the fixtures are still wired into tests). |
| `persistence-phase09.test.ts` | `plugin_factories_resolve_prisma_repos_not_memory_stores` | Boot the app; introspect the registered services on `fastify.syncStore` and `fastify.userStore`; assert they are `PrismaSyncStore` / `PrismaUserStore` instances (constructor name check) in every non-test boot path. |

### §9.7 P09-G07 -- Refresh-token rotation atomicity (HIGH)

- **Source:** phase-09.md §3 refresh-token rotation atomicity
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

Rotation revokes the old refresh token and issues a new one. Both writes must commit inside a single `prisma.$transaction`; otherwise a crash between the revoke and the insert leaves the user either with two valid tokens (security risk) or with neither (lockout). Phase-02 §7.5 declares the invariant locally; phase-09 must verify it survives the swap to Prisma.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `refresh_rotation_wrapped_in_prisma_transaction` | Spy on `prisma.$transaction`; call `/auth/refresh`; assert the spy was invoked exactly once and that BOTH the `revoke(old)` and `create(new)` operations resolve inside its callback. |
| `persistence-phase09.test.ts` | `mid_rotation_failure_rolls_back_both_writes` | Force `prisma.refreshToken.create` to throw inside the tx; assert the old token's `revokedAt` is still NULL after the failure (revoke rolled back) AND no new token row exists. The user can still refresh with the original token. |
| `persistence-phase09.test.ts` | `mid_rotation_failure_returns_500_not_partial_success` | Same forced-failure scenario; the HTTP response is 500 with the global error envelope; the client retries with the unchanged refresh token and succeeds on a clean second attempt. |

### §9.8 P09-G08 -- Named volume persists across down/up (HIGH)

- **Source:** phase-09.md §5 docker-compose `sync_db_data` volume
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

`docker compose restart` keeps containers and volumes attached; `docker compose down` removes containers and only `down -v` removes volumes. The `sync_db_data` named volume must persist the Postgres data across a full `down` + `up` cycle. The existing E2E only restarts; no spec proves the volume binding works.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `prisma-persistence-survives-full-compose-down-up.e2e.ts` | Mariam | 1) `docker compose up -d`. 2) Login; create a doctor + lock a visit. 3) `docker compose down` (NOT `down -v`). 4) `docker compose up -d`. 5) Login; pull. | Doctor + visit + audit rows all present. `docker volume inspect sync_db_data` shows the volume survived. A control run with `down -v` MUST clear the data (verifies the binding works in both directions). |

### §9.9 P09-G09 -- .env CI guardrail actually fails on dirty fixture (MEDIUM)

- **Source:** phase-09.md §5 CI guardrail (no `sync-server/.env` committed)
- **Target test section:** §6.7
- **Category:** Missing Integration Test

The §8 DoD includes `test "$(git ls-files sync-server/.env)" = ""` as a guardrail, but no test proves the guardrail itself works. A fixture commit that adds `sync-server/.env` to the index must cause the check to exit non-zero; otherwise the guardrail is theatre.

| Test File | Test | Asserts |
|-|-|-|
| `env-schema-phase09.test.ts` | `ci_guardrail_fails_when_sync_server_env_committed_in_fixture` | In a temp git repo: stage a `sync-server/.env` file; run the guardrail one-liner; assert exit code is non-zero AND stderr/stdout includes the offending path. |
| `env-schema-phase09.test.ts` | `ci_guardrail_passes_on_clean_tree` | Same temp repo without `.env`; the guardrail exits 0 and prints nothing. Positive control. |
| `env-schema-phase09.test.ts` | `ci_guardrail_passes_when_env_template_exists_but_env_does_not` | The template `.env.template` is checked in; the actual `.env` is gitignored. The guardrail must not flag the template. |

### §9.10 P09-G10 -- operator_service comment states cascade rule (MEDIUM)

- **Source:** phase-09.md §3 operator_service.rs:222 doc-comment rewrite
- **Target test section:** §1.1 / §2.1
- **Category:** Missing Integration Test

The current grep tests assert ABSENCE of the stale `phase-04` forward-reference. Absence alone doesn't prove the new comment documents the cascade rule -- the comment could be empty. The rewrite must affirmatively state that soft-deleting an operator cascades to its specialties as a permanent design decision.

| Module | Test | Asserts |
|-|-|-|
| `catalog::service::operator_service` | `doc_comment_states_cascade_rule_explicitly` | `grep -E "cascade|operator_specialties" src-tauri/src/domains/catalog/service/operator_service.rs` returns at least one match in the L222 region (lines 200-240). The match text describes the cascade as the documented behavior, not a TODO or forward reference. |
| `catalog::service::operator_service` | `doc_comment_does_not_reference_future_phases` | Combined check: ZERO `phase-0\d`, `TODO`, `FIXME`, `will be hardened`, or `forward reference` strings in the same region. Affirmative presence + negative absence together pin the rewrite. |

### §9.11 P09-G11 -- Sidebar "Coming soon" decision recorded (MEDIUM)

- **Source:** phase-09.md §3 sidebar.tsx:152 "Coming soon" decision
- **Target test section:** §1.2 / §2.4
- **Category:** Missing Edge Coverage

The current §1.2 row pins "whichever decision lands" without forcing a decision. Phase-09 cannot flip to `complete` while the build spec leaves the choice open. The gap-closing decision (keep with `aria-disabled` + i18n keys, or remove entirely) must be recorded in the phase plan and tested deterministically.

| Module | Test | Asserts |
|-|-|-|
| `src/components/shell/sidebar.tsx` | `coming_soon_decision_recorded_in_phase_09_plan` | A documentation check: `docs/idc-system/phase-09.md` §7 (Open Decisions) records the resolved choice (a or b) with a date. The test reads the markdown and asserts the decision row is non-empty. |
| `src/components/shell/sidebar.tsx` | `coming_soon_item_matches_recorded_decision_option_a` (if option a) | Renders the sidebar; asserts the disabled item exists, both i18n keys resolve, `aria-disabled="true"` is set, and the item is not focusable via Tab. |
| `src/components/shell/sidebar.tsx` | `coming_soon_item_matches_recorded_decision_option_b` (if option b) | Renders the sidebar; asserts the item is ABSENT from the DOM entirely; no orphan i18n keys remain in `en.json` or `ar.json` for the removed entry. |

### §9.12 P09-G12 -- env-template byte-hash verified in CI (LOW)

- **Source:** phase-09.md §3 env schema byte-hash
- **Target test section:** §3.3
- **Category:** Missing Snapshot

The snapshot file `expected/preship/env-template-canonical.txt.sha256` is listed in §3.3 but no test row asserts a CI step actually compares the current `.env.template` against the hash. File existence is not equivalent to hash verification.

- **Snapshot files**:
  - `expected/preship/env-template-canonical.txt.sha256` -- verified via `sha256sum sync-server/.env.template | diff - expected/preship/env-template-canonical.txt.sha256` in CI; drift fails the build. The test plan must declare the comparison step, not just the artifact path.

### §9.13 P09-G13 -- .dockerignore excludes build noise (LOW)

- **Source:** phase-09.md §5 sync-server/.dockerignore
- **Target test section:** §1.3 / §2.3
- **Category:** Missing Coverage Gate

`Dockerfile.dev` ships in phase-09 but no test asserts `sync-server/.dockerignore` excludes `node_modules`, `dist`, `.env`, and `coverage` from the build context. Without the ignore file, build context size balloons and `.env` can leak into the image.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `sync-server/.dockerignore` | File exists; contains entries `node_modules`, `dist`, `.env`, `coverage` (each on its own line, no negation) | `test/preship/dockerignore-phase09.test.ts` reads the file and asserts each required pattern is present and not preceded by `!`. |

### §9.14 P09-G14 -- Operator cascade decision recorded in manual scripts (LOW)

- **Source:** phase-09.md §7 Open Decision #5 (operator cascade rule)
- **Target test section:** §5
- **Category:** Missing Persona / Manual Step

The manual / persona scripts must record which option (a: cascade is documented behavior, or b: cascade is a phase-04 hardening point) was selected for the operator cascade rule. Without the recorded selection, future regression of the cascade behavior cannot be audited against intent.

- **Operator cascade decision audit (manual).** Added to §5.1:
  1. Open `docs/idc-system/phase-09.md` §7 Open Decisions.
  2. Confirm Decision #5 has a recorded option (a or b) with a date and an owner.
  3. Cross-reference the recorded option against `src-tauri/src/domains/catalog/service/operator_service.rs` behavior (cascade enabled vs deferred).
  4. If absent or contradicted, file a P1 defect in `defects.md` and block phase-09 completion.

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). These are Pass 2 additions layered on top of the §9 Pass 1 rows; the same merge discipline applies -- entries live here during the Pass 2 verification cycle, then fold into their target §X.Y subsections during test authoring. When Pass 3 re-runs, every gap below must show as covered.

### §10.1 P09-G15 -- PrismaSyncCursorRepo upsert against composite PK (CRITICAL)

- **Source:** phase-09.md §4 cursor semantics under Prisma
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§4 mandates that `PrismaSyncCursorRepo.bumpCursor` uses `upsert` against the composite PK `(entityId, deviceId)` per phase-01 §7.19. A naive `findUnique` + `create-or-update` two-step would lose the race when two devices bump the same `(entityId, deviceId)` cursor concurrently, silently breaking monotonicity invariants. Existing §2.3 rows assert the end-to-end pull cursor outcome but never the underlying Prisma API call shape, so a refactor that replaced `upsert` with the read-modify-write pair would pass.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `bump_cursor_uses_prisma_upsert_with_composite_pk` | Spy on `prisma.syncCursor.upsert`; call `cursorRepo.bumpCursor(entityId, deviceId, newCursor)`; assert `upsert` was invoked exactly once with `where: { entityId_deviceId: { entityId, deviceId } }` and that `findUnique` / `update` / `create` were NOT called as separate operations. |
| `persistence-phase09.test.ts` | `concurrent_bump_cursor_calls_resolve_monotonically` | Fire two `bumpCursor` calls for the same `(entityId, deviceId)` with cursors `T0` and `T1 > T0` in parallel; assert the persisted row reads back as `T1` (highest); neither call raises a unique-constraint error -- `upsert` absorbs the race. |

### §10.2 P09-G16 -- init-custom-sql.sql runs after prisma db push (CRITICAL)

- **Source:** phase-09.md §2 / §5 Dockerfile.dev CMD ordering
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§5 specifies the `Dockerfile.dev` CMD as `pnpm prisma db push --accept-data-loss && psql "$DATABASE_URL" -f prisma/init-custom-sql.sql && node dist/main.js`. The ordering is load-bearing: `init-custom-sql.sql` carries the raw-SQL CHECK constraints, triggers, and paired partial unique indexes from §2 that depend on Prisma-generated tables existing. A reversed order leaves the constraints unapplied and the container boots cleanly with no error -- only a later phase-06 push of an invalid `(reason, delta)` combo would surface the missing CHECK.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `dockerfile_dev_cmd_runs_db_push_before_init_custom_sql` | Parse the `Dockerfile.dev` CMD shell string; tokenize on `&&`; assert the index of `prisma db push` is strictly less than the index of `psql ... -f prisma/init-custom-sql.sql`, which is strictly less than `node dist/main.js`. A reordered CMD fails the test. |
| `persistence-phase09.test.ts` | `init_custom_sql_constraints_present_after_full_boot` | Boot a fresh `sync-db` + `sync-server` via compose; query `information_schema.check_constraints` and `information_schema.triggers`; assert every phase-03/05/06 invariant (DoctorCheckPricing paired index, inventory_adjustments delta-sign CHECK, visits name-snapshot CHECK, BEFORE UPDATE abort trigger) is present exactly once. |

### §10.3 P09-G17 -- PrismaEntityRepo single $transaction per batch (CRITICAL)

- **Source:** phase-09.md §3 PrismaEntityRepo `prisma.$transaction([...])` per-batch
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§3 specifies that `PrismaEntityRepo` uses `prisma.$transaction([...])` per-batch for atomicity. The `dispatchEntity` path of `SyncPushService` may apply N ops to N rows in one push; without the surrounding transaction, a failure on op K leaves ops 0..K-1 committed and the client believes the whole batch failed (because it retries the whole batch). Existing §2.3 rows assert per-entity outcomes but never the batch-level atomicity contract.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `dispatch_entity_wraps_batch_in_single_prisma_transaction` | Spy on `prisma.$transaction`; push a batch of 3 doctor ops via `/sync/push`; assert `$transaction` was invoked exactly once with an array of 3 promises (or one callback containing all 3 writes), NOT three separate `$transaction` calls. |
| `persistence-phase09.test.ts` | `partial_batch_failure_rolls_back_all_ops` | Push a batch of 3 doctor ops where op 2 violates a unique constraint; assert the response is 500 (or per the error envelope, the documented batch-fail code); assert NONE of the 3 doctor rows exist in Postgres post-failure -- the transaction rolled all of them back. |

### §10.4 P09-G18 -- healthz response shape adds migrationsApplied and version (HIGH)

- **Source:** phase-09.md §3 healthz wiring (response body)
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§3 commits the new `/healthz` response shape to include `migrationsApplied: boolean` and `version: '0.1.0'` alongside the widened `status` union. §9.5 (P09-G05) covered the `status` widening alone; neither §9.5 nor any existing §3.1 row validates the two new fields. A regression that dropped `migrationsApplied` or hardcoded `version: '0.0.0'` would slip through because Ajv would accept the narrower body.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /healthz` (schema-level) | `HealthSchema` extended | Introspect the TypeBox schema: assert `migrationsApplied` is `Type.Boolean()` and `version` is `Type.Literal('0.1.0')` (or `Type.String()` with format pinned). Reject any schema missing either field. |
| `GET /healthz` (Ajv response validation) | `HealthSchema` extended | Hit `/healthz` against a booted server; assert the response body matches `{ status: 'ok' \| 'fail', db: 'ok' \| 'fail', redis: 'ok' \| 'fail', migrationsApplied: true, version: '0.1.0' }`. `migrationsApplied` is `true` on a freshly-migrated DB; the `version` field equals `'0.1.0'` byte-for-byte. |

### §10.5 P09-G19 -- PrismaUserStore preserves sha256 hashing of refresh tokens (HIGH)

- **Source:** phase-09.md §3 PrismaUserStore (phase-02 §7.21 invariant)
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§3 explicitly states "refresh tokens still sha256 before persisting (phase-02 §7.21)". The swap from `MemoryUserStore` to `PrismaUserStore` MUST preserve this invariant -- the `RefreshToken.tokenHash` column stores `sha256(presentedToken)`, never the plaintext token. A regression that wrote the raw refresh token would compromise every persisted session if the database were ever leaked. Existing §2.3 rows verify rotation atomicity (P09-G07) but not the hash-at-rest invariant.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `prisma_user_store_persists_sha256_of_refresh_token_not_plaintext` | Login as the bootstrap superadmin; capture the issued `refreshToken`. Query the `RefreshToken` table directly via Prisma; assert the stored `tokenHash` equals `sha256(refreshToken)` (hex-encoded, 64 chars); assert NO column contains the plaintext refresh token substring. |
| `persistence-phase09.test.ts` | `prisma_user_store_lookup_uses_sha256_of_presented_token` | Spy on `crypto.createHash('sha256')`; call `/auth/refresh` with a valid token; assert the hash function was invoked on the presented token before the `prisma.refreshToken.findUnique` call; the `findUnique` `where.tokenHash` argument equals the computed hash, not the raw token. |

### §10.6 P09-G20 -- PrismaUserStore preserves Argon2id password hashes (HIGH)

- **Source:** phase-09.md §3 PrismaUserStore (phase-02 §7.21 invariant)
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§3 affirms "password hashes still Argon2id". The bootstrap path persists the superadmin password via the new Prisma-backed store; without an explicit test, a swap could silently downgrade to bcrypt, scrypt, or worst-case plaintext while still satisfying the round-trip login flow (any hash-or-plaintext scheme that round-trips would pass the existing login test). The hash format must be locked to Argon2id at rest.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `bootstrap_superadmin_password_persisted_as_argon2id` | Boot the server with bootstrap env vars set; query the `User` table; assert the `passwordHash` column matches the Argon2id PHC prefix `$argon2id$v=19$m=...$t=...$p=...$...` (regex check). Reject `$2a$` / `$2b$` (bcrypt), `$scrypt$`, or any non-PHC plaintext shape. |
| `persistence-phase09.test.ts` | `password_verify_path_calls_argon2_verify_not_bcrypt` | Spy on `@node-rs/argon2` (or the project's chosen Argon2 binding) `verify`; login with the bootstrap superadmin credentials; assert `argon2.verify` was invoked with the stored hash and the presented password. No bcrypt module is loaded in the production code path. |

### §10.7 P09-G21 -- Pull ordering: updatedAt asc then id asc (HIGH)

- **Source:** phase-09.md §4 cursor semantics under Prisma
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§4 specifies pull queries use `orderBy: [{ updatedAt: 'asc' }, { id: 'asc' }]` for stable pagination. The secondary `id asc` sort is load-bearing: two rows written in the same millisecond on the server (common under concurrent device pushes) tie on `updatedAt`, and without a stable secondary key the pagination cursor can skip rows or return duplicates across page boundaries. Existing §2.3 rows assert the cursor moves forward but never the orderBy clause shape.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `prisma_pull_query_orders_by_updated_at_asc_then_id_asc` | Spy on `prisma.<model>.findMany` for every syncable model; trigger `/sync/pull?entity=<m>&since=<cursor>`; assert the `orderBy` argument equals `[{ updatedAt: 'asc' }, { id: 'asc' }]` exactly -- not `{ updatedAt: 'asc' }` alone, not `[{ id: 'asc' }, { updatedAt: 'asc' }]`. |
| `persistence-phase09.test.ts` | `pull_pagination_stable_on_identical_updated_at_ties` | Seed 5 doctor rows with byte-identical `updatedAt` timestamps and ascending `id`s; pull with `pageSize=2`; assert page 1 returns ids `[id_1, id_2]`, page 2 returns `[id_3, id_4]`, page 3 returns `[id_5]`; no row is skipped or duplicated across pages. |

### §10.8 P09-G22 -- auth-services bootstrap persists superadmin to Postgres idempotently (HIGH)

- **Source:** phase-09.md §3 auth-services bootstrap path (lines 29-36 stays, now persisting to Postgres)
- **Target test section:** §2.3 persistence-phase09
- **Category:** Missing Integration Test

§3 specifies the auth-services bootstrap path "stays, now persisting to Postgres". The bootstrap inserts the superadmin user on first boot when `BOOTSTRAP_SUPERADMIN_EMAIL` env is set; previously the in-memory store evaporated on restart, so re-running was harmless. With Postgres, a second boot must NOT create a duplicate superadmin row -- the upsert / find-then-insert pattern must be idempotent. A naive `prisma.user.create` would throw a unique-constraint error on the second boot, breaking the container restart loop.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `bootstrap_superadmin_inserts_exactly_one_row_on_first_boot` | Boot with bootstrap env vars set against an empty `User` table; assert exactly one row exists with `email = $BOOTSTRAP_SUPERADMIN_EMAIL` and `role = 'superadmin'`. |
| `persistence-phase09.test.ts` | `bootstrap_superadmin_idempotent_across_restarts` | Boot the server with bootstrap env vars; observe one superadmin row. Stop and re-boot the server with the SAME env vars; assert the `User` table still has exactly one row matching the bootstrap email, the row's `id` is unchanged, and no unique-constraint error was raised during boot. |
| `persistence-phase09.test.ts` | `bootstrap_path_skipped_when_env_unset` | Boot without `BOOTSTRAP_SUPERADMIN_EMAIL`; assert the `User` table is empty and no bootstrap log line fired. |

### §10.9 P09-G23 -- Tenant isolation enumerated across 15 syncable models (HIGH)

- **Source:** phase-09.md §3 PrismaEntityRepo (15 syncable-entity repositories)
- **Target test section:** §6.7 / §2.3
- **Category:** Missing Integration Test

§3 names "All 15 syncable-entity repositories" routed through `PrismaEntityRepo`. §6.7 mentions a static-analysis check on tenant scoping but does not enumerate the 15 models, so a regression in a single repo's `where` clause (forgetting `entityId: tenantId`) could leak rows across tenants. The Pass 1 row owns the static-analysis hook; this Pass 2 row pins the explicit per-model assertion list so every model is covered concretely.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `entity_repo_dispatch_includes_tenant_id_filter_for_every_syncable_model` | For each of the 15 syncable entities (enumerate by reading the TENANT_MODELS list from `prisma-extension-config.ts`), spy on the corresponding `prisma.<model>.findMany` and `prisma.<model>.update`; trigger a pull and a push for that entity; assert every captured `where` argument contains `entityId: <tenantId>` (or the documented tenant column name). A model whose dispatch path omits the filter fails the row. |
| `persistence-phase09.test.ts` | `cross_tenant_pull_returns_zero_rows_for_every_model` | Seed two tenants (`tenant_A`, `tenant_B`) with one row per syncable model in each; authenticate as `tenant_A`; pull every entity; assert ZERO `tenant_B` rows appear in any pull response. Repeat the symmetric check from `tenant_B`. |

### §10.10 P09-G24 -- Postgres image pinned to 16.4-alpine (MEDIUM)

- **Source:** phase-09.md §7.1 (Postgres image pinning decision)
- **Target test section:** §6.6 / §2.3
- **Category:** Missing Coverage Gate

§7.1 records the decision to pin `postgres:16.4-alpine` rather than the floating `postgres:16-alpine`. The current `docker-compose.yaml` skeleton in §5 still shows `postgres:16-alpine`; the pinned tag must land in the committed file. A floating tag drifts when Postgres ships a new minor (16.5, 16.6 ...) and the team loses repro fidelity across machines -- a regression CI cannot detect.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `sync-server/docker-compose.yaml` | The `image:` field under `services.sync-db` MUST equal `postgres:16.4-alpine` exactly (string match, not glob). Reject `postgres:16-alpine`, `postgres:16.4`, or any other tag. | `test/preship/compose-image-pin-phase09.test.ts` parses the YAML with `js-yaml`, walks `services.sync-db.image`, and asserts string equality with `postgres:16.4-alpine`. |

### §10.11 P09-G25 -- metrics.ts hide:true rationale documented in source (MEDIUM)

- **Source:** phase-09.md §3 routes/metrics.ts rationale documentation
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§3 says `routes/metrics.ts` keeps `hide: true` and explicitly requires: "Document this rationale in `metrics.ts` so future audits don't re-flag it." Without a test, the comment can rot or be deleted in a future cleanup pass and the same audit finding will resurface. The comment must be present and reference the gating mechanism (`X-Internal-Token`) and the audience (Prometheus, not human consumers).

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `metrics_route_carries_hide_true_rationale_comment` | Read `sync-server/src/app/routes/metrics.ts`; assert the source contains a comment block within 10 lines of the `hide: true` schema option that references both "X-Internal-Token" (the gating header) and "Prometheus" (the intended consumer). A bare `hide: true` without the rationale comment fails the test. |

### §10.12 P09-G26 -- docker-compose volume mounts for hot-reload (MEDIUM)

- **Source:** phase-09.md §5 docker-compose volumes
- **Target test section:** §2.3 / §4.1
- **Category:** Missing Integration Test

§5 declares the `sync-server` service mounts `./src:/app/src` and `./prisma:/app/prisma` so dev edits propagate into the running container without a rebuild. Without the mounts, every schema change or source edit requires `docker compose build` and the documented dev loop in §6 breaks down. The mounts must be present in the committed compose file; a stealth removal during a "cleanup" pass would silently degrade the developer experience.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `compose_sync_server_mounts_src_and_prisma_for_hot_reload` | Parse `sync-server/docker-compose.yaml`; assert `services.sync-server.volumes` is an array containing exactly the entries `./src:/app/src` and `./prisma:/app/prisma` (or their long-form equivalents with `type: bind`). Reject a missing or empty volumes list. |
| `persistence-phase09.test.ts` | `compose_sync_db_mounts_named_volume_for_persistence` | Same parse; assert `services.sync-db.volumes` includes `sync_db_data:/var/lib/postgresql/data` and the top-level `volumes:` section declares `sync_db_data`. This pairs with P09-G08 (named volume survives down/up) but pins the compose file shape rather than the runtime behavior. |

### §10.13 P09-G27 -- Refresh-token retention vacuum prunes revoked rows (MEDIUM)

- **Source:** phase-09.md §4 refresh-token persistence semantics step 4 (retention vacuum)
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§4 step 4 says "Both new and old rows live until the retention vacuum prunes revoked rows older than `JWT_REFRESH_TTL_SECONDS`." Without a vacuum job and a test for it, the `RefreshToken` table grows unbounded as every rotation appends a row that is never reclaimed. Over the lifetime of a clinic deployment this is an unbounded leak that ultimately bloats backups and slows lookups. The vacuum must run on a schedule (or per request) and the cutoff must be `JWT_REFRESH_TTL_SECONDS`.

| Test File | Test | Asserts |
|-|-|-|
| `persistence-phase09.test.ts` | `refresh_token_vacuum_deletes_revoked_rows_older_than_ttl` | Seed three `RefreshToken` rows: (A) revoked 1 hour ago, (B) revoked `JWT_REFRESH_TTL_SECONDS + 1s` ago, (C) active never revoked. Invoke the vacuum routine; assert (B) is deleted, (A) and (C) remain. |
| `persistence-phase09.test.ts` | `refresh_token_vacuum_leaves_active_tokens_untouched` | Seed an active token with `revokedAt = null` whose `expiresAt` is in the future; run the vacuum repeatedly; assert the row persists across every invocation regardless of its age. |

### §10.14 P09-G28 -- Manual script lists sidebar decision verification step (MEDIUM)

- **Source:** phase-09.md §3 sidebar.tsx:152 + §7 Open Decisions
- **Target test section:** §5 / §1.2
- **Category:** Manual Step

§9.11 (P09-G11) already pins a `coming_soon_decision_recorded_in_phase_09_plan` doc-check test that reads phase-09.md §7 for the recorded decision. What is missing is the corresponding manual script step that walks a reviewer through opening `docs/idc-system/phase-09.md` §7 and visually confirming the decision row is populated with option (a or b), a date, and an owner -- mirroring the existing P09-G14 operator-cascade manual step. Without the manual step, the doc-check test alone could pass on a row that was added programmatically without any human review.

- **Sidebar "Coming soon" decision audit (manual).** Added to §5.1:
  1. Open `docs/idc-system/phase-09.md` §7 Open Decisions.
  2. Locate the row for the `sidebar.tsx:152` "Coming soon" decision (Decision #5 in the current draft, or the row whose subject matches the sidebar entry).
  3. Confirm the row is populated with one of the two recorded options (a: keep with `aria-disabled` + i18n keys, or b: remove entirely), a decision date, and a named owner.
  4. Cross-reference the recorded option against `src/components/shell/sidebar.tsx` at line 152 -- the rendered behavior must match the recorded choice.
  5. If the row is empty or contradicted by the code, file a P2 defect in `defects.md` and block phase-09 completion.

### §10.15 P09-G29 -- docker-compose canonical snapshot (LOW)

- **Source:** phase-09.md §3 / §5 docker-compose.yaml
- **Target test section:** §3.3
- **Category:** Missing Snapshot

§5 commits the exact shape of `sync-server/docker-compose.yaml` (services, env vars, ports, volumes). A stealth edit that swapped `JWT_SECRET: ${JWT_SECRET}` for a hardcoded literal, or added an unauthenticated `postgres` port binding (`0.0.0.0:5432`), would not be caught by the per-field tests in P09-G24 / P09-G26 because those only assert their specific keys. A canonical byte-hash snapshot locks the whole file shape and forces any change through PR review.

- **Snapshot files**:
  - `expected/preship/docker-compose-canonical.yaml.sha256` -- verified via `sha256sum sync-server/docker-compose.yaml | diff - expected/preship/docker-compose-canonical.yaml.sha256` in CI. Drift fails the build; legitimate compose changes regenerate the hash with explicit reviewer sign-off in the PR body (mirrors the §3.3 env-template snapshot policy from §9.12). The test plan must declare the comparison step alongside the artifact path.

### §10.16 P09-G30 -- PrismaEntityRepo lwwShouldApply branch coverage (LOW)

- **Source:** phase-09.md §4 LWW helper centralisation
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§4 centralises the `(version, updatedAt, originDeviceId)` LWW tiebreak inside `PrismaEntityRepo.lwwShouldApply(serverRow, incoming) -> boolean`. The helper is pure logic with several branches (version-strictly-greater, version-equal-tiebreak-on-updatedAt, equal-updatedAt-tiebreak-on-originDeviceId). The §1.3 coverage row pins Prisma repo files at 90% lines overall, but lines coverage on a small helper can hit 90% without exercising every branch. Branch coverage on this single function is the load-bearing safeguard against a regression that flipped a `>=` to `>`.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `sync-server/src/app/sync/infrastructure/prisma/entity-repo.ts` (`lwwShouldApply` function only) | >= 100% branches | `c8 --reporter=text --branch-coverage --include 'src/app/sync/infrastructure/prisma/entity-repo.ts' pnpm test test/persistence-phase09.test.ts` -- the per-function branch coverage must hit 100% on `lwwShouldApply`; gate the CI step on the c8 JSON summary. Drop below 100% and the build fails. |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 4 Phase-09 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P09-G31 through P09-G34). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P09-G31 -- 009_pre_ship.sql no-op migration file (HIGH)

- **Source:** phase-09.md §1 / §2 line 21 -- "`src-tauri/migrations/009_pre_ship.sql` no-op header so SQLite migration runner records version 9".
- **Target test section:** §2.1 / §6.8
- **Category:** Missing Integration Test

A missing file would leave the migrations table un-bumped and break a later phase's idempotent re-apply check.

| Scenario | Asserts |
|-|-|
| `migration_009_pre_ship_file_exists_and_bumps_version_to_9` | Assert `src-tauri/migrations/009_pre_ship.sql` exists on disk. Read its contents; assert the file contains either no DDL statements (whitespace + SQL comments only) or a `-- no-op` marker. Apply migrations 001..009 to a fresh in-memory SQLite; query `PRAGMA user_version` (or the project's migrations-tracking table); assert the recorded version is 9. Re-apply migration 009 against the same DB; assert no error (idempotent header). Per §1 + §2 line 21. |

### §11.2 P09-G32 -- Prepared migration slot lex-order (MEDIUM)

- **Source:** phase-09.md §2 line 37 -- rename / verify lex-order of `20260512000000_inventory_adjustments_delta_sign/` prepared migration slot.
- **Target test section:** §2.3 / §6.8
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `inventory_adjustments_delta_sign_migration_slot_status_and_lex_order` | Query Prisma's `_prisma_migrations` table on the test Postgres; locate the `inventory_adjustments_delta_sign` row. If `finished_at IS NOT NULL` (applied), assert no rename is needed and pass. If `finished_at IS NULL` (prepared but not applied), assert the directory name under `sync-server/prisma/migrations/` has been renamed to match the planner's decision. List all migration dirs sorted lexicographically; assert the slot's name comes BEFORE any later phase-10+ slot AND AFTER all phase-09 slots. A reversed order could shadow a later migration. Per §2 line 37. |

### §11.3 P09-G33 -- ProcessedOpRepository composite PK shape (MEDIUM)

- **Source:** phase-09.md §3 line 102 -- `ProcessedOpRepository` composite PK `(op_id, entityId)`.
- **Target test section:** §3.3 / §2.3
- **Category:** Missing Contract Test

Mirrors P01-G19's lesson: contract-test the compound PK shape directly. §2.3 `processed_op_cache_survives_container_restart_idempotent` covers behaviour, not shape.

| Scenario | Asserts |
|-|-|
| `processed_op_composite_pk_shape_pinned_via_prisma_introspection` | Inspect the generated Prisma client TypeScript types for the `ProcessedOp` model. Assert the `@@id` declaration in the Prisma schema is `@@id([op_id, entityId])` -- in that EXACT order (not `[entityId, op_id]`, which Prisma names differently and would silently change the compound-id field name). Run an introspection query against the test Postgres: `SELECT a.attname FROM pg_constraint c JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY(c.conkey) WHERE c.conrelid = '"ProcessedOp"'::regclass AND c.contype = 'p' ORDER BY array_position(c.conkey, a.attnum);`. Assert the column order is `op_id`, `entity_id`. Per §3 line 102 + P01-G19 lesson. |

### §11.4 P09-G34 -- DATABASE_URL sslmode=prefer in production template (LOW)

- **Source:** phase-09.md §6.7 line 308 -- "Postgres connection security -- DATABASE_URL uses sslmode=prefer or higher in production".
- **Target test section:** §6.7
- **Category:** Missing Integration Test

The bullet claims coverage that does not exist.

| Scenario | Asserts |
|-|-|
| `env_template_database_url_carries_sslmode_prefer_for_production` | Read `sync-server/.env.template`. Assert the commented production-example line for `DATABASE_URL` includes `sslmode=prefer` (or `sslmode=require` / `verify-full` -- any value at least as strong as `prefer`). Boot the server with `NODE_ENV=production` and a `DATABASE_URL` that omits `sslmode=` (or explicitly sets `sslmode=disable`); assert the env-schema validation throws (per @fastify/env strict-mode contract). Per §6.7 line 308. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 3 Phase-09 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P09-G35 through P09-G37). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P09-G35 -- AUTH_EXPIRED_REFRESH 401 mapping distinct from AUTH_INVALID_REFRESH (MEDIUM)

- **Source:** phase-09.md §3 Sync Server "Error-handler reach" -- both `AUTH_INVALID_REFRESH` AND `AUTH_EXPIRED_REFRESH` 401 mappings.
- **Target test section:** §2.3
- **Category:** Missing Integration Test

| Route | Test | Asserts |
|-|-|-|
| `POST /auth/refresh` | `refresh_expired_token_returns_401_with_auth_expired_refresh_distinct_code` | Seed a `RefreshToken` row with `expiresAt < now` and `revokedAt IS NULL`. POST `/auth/refresh { refreshToken: <expired> }`. Assert: (a) HTTP 401; (b) response body `error.code === 'AUTH_EXPIRED_REFRESH'` (NOT `'AUTH_INVALID_REFRESH'`); (c) the two codes are distinct in the error-handler mapping (a follow-up POST with a totally-unknown token returns `AUTH_INVALID_REFRESH`). A regression collapsing both into `AUTH_INVALID_REFRESH` (or 500) is detected. Per §3 error-handler reach. |

### §12.2 P09-G36 -- fjwt verify algorithms allowlist (MEDIUM)

- **Source:** phase-09.md §3 Sync Server auth-jwt rewrite -- `verify: { algorithms: ['RS256'] }` passed to `fjwt` registration.
- **Target test section:** §3.1 / §2.3
- **Category:** Missing Contract Test

| Scenario | Asserts |
|-|-|
| `fjwt_registration_passes_algorithms_RS256_allowlist_option` | Spy on `fastify.register` calls during the auth-jwt plugin's boot. Capture the options object passed to `fjwt`. Assert: `options.verify.algorithms` is the EXACT array `['RS256']` (not `undefined`, not `['HS256','RS256']`, not the default which permits `none`). Verify this in BOTH `NODE_ENV=production` and `NODE_ENV=development` branches -- the algorithms allowlist MUST apply in both. A regression that omitted the option would still pass RS256 token-acceptance tests because the RS256 path still verifies; only this option-shape assertion catches the regression. Per §3 auth-jwt rewrite. |

### §12.3 P09-G37 -- prisma.ts log config by NODE_ENV (LOW)

- **Source:** phase-09.md §3 Sync Server `prisma.ts` -- `log: process.env.NODE_ENV === 'development' ? ['warn','error'] : ['error']`.
- **Target test section:** §1.1
- **Category:** Missing Unit Test

| Scenario | Asserts |
|-|-|
| `prisma_plugin_log_config_branches_on_NODE_ENV` | Mount the `prisma.ts` plugin twice in isolated Fastify instances: once with `NODE_ENV=development`, once with `NODE_ENV=production`. Spy on `new PrismaClient(options)` construction. Assert: (a) dev instance passes `log: ['warn', 'error']`; (b) prod instance passes `log: ['error']`. A regression that flipped production to `['query','info','warn','error']` would silently leak SQL into prod logs (privacy + log-volume regression) and the lines-coverage gate alone would not catch it. Per §3 prisma.ts log-config rule. |
