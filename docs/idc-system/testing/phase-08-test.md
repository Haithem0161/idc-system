# Phase 08: Audit, Conflict Resolver & Polish -- Test Plan

**Proves:** A superadmin can search the audit log via `/audit` with 6 filter inputs (actor + action + entity + entity_id prefix per §7.24 + date range + free-text) and click into entity-detail pages (§7.7), the cross-boundary merge-paginator (§7.4) seamlessly stitches local + server pages around the 90-day cliff with the `<ServerBackedBadge>` (§7.25) marking server-sourced rows. The conflict resolver UI at `/sync/conflicts` (§7.11 + §7.23 role-gated to superadmin) lists parked conflicts from a server GET endpoint (durable across app restart per §7.11), renders side-by-side local-vs-server payloads via `<DeltaViewer>`, accepts Keep Local / Keep Server / Merge resolutions, and the resolve flow is server-idempotent on `resolve_op_id` (§7.22) so mid-flight network drops don't double-apply. The `<SyncPill>` onClick wires to `/sync/conflicts` (§7.14) when state is error or outboxCount > 0. The daily audit vacuum (§7.1 + §7.2 missed-run handling + §7.21 metrics_events extension) soft-deletes audit rows older than 90 days WITH `dirty=0` ONLY (never flips dirty), deletes metrics_events older than 30 days, writes one self-audit row with `action='vacuum'`, and idempotently catches up after app sleep. A `vacuum_unsynced_safe` repo path bypasses the standard sync-row update closure so the vacuum doesn't re-push pruned rows (per phase-01 §7.31 + §7.16 carve-out). The 8-hour soak harness asserts §7.16 quantitative criteria (>=50 ops/sec sustained, <=800 outbox steady-state, p95 lock < 30s, < 50MB memory growth, audit-vacuum <10s for 90-day rowset, zero sync_conflict auto_resolved). The `pnpm lint:i18n` script (§7.9) AST-walks every `.tsx`/`.ts` outside `src/i18n/locales/` and fails on any Arabic / English literal. The `pnpm lint:rtl` script (§7.18) AST-scans for unwrapped chevron icons. `pnpm a11y` (§7.13) walks every page; axe-core reports zero serious or critical violations; all interactive components have visible focus rings (phase-01 §7.25); WCAG 2.1 AA color contrast holds. Telemetry: `GET /metrics` (Prometheus, gated by `X-Internal-Token`), enriched `/healthz`, and `diagnostics::summary` IPC surface PRD §1.3 success metrics (§7.17).

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (audit_log + outbox + ProcessedOp + ConflictParked + ConflictResolveService stub + the `vacuum` audit action + metrics_events table + sync_state.last_audit_vacuum_at column from §7.19), Phase 02 test (auth + `<RequireRole>`), Phase 03 test (catalog -- audit drill-down navigates to admin entity detail), Phase 04 test (shifts entity references), Phase 05 test (`manual` conflict policy on `visits`, `<DirtyDot>` from §7.29, `<VisitDetail mode='readonly'>` for audit-table drill-down), Phase 06 test (additive-only policy on `inventory_adjustments`), Phase 07 test (read-only accounting surfaces -- `<UserMenu>` Diagnostics modal from §7.17 hides/shows links by role).

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_audit_log_entry_old, make_audit_log_entry_recent, make_conflict_parked, make_metrics_event}` (extended).
- Factories (TS): `src/test-utils/factories.ts::{makeAuditFilter, makeAuditRow, makeConflictListItem, makeDiagnosticsSummary}`.
- Factories (Sync server): `sync-server/test/support/factories.ts::{makeAuditQueryParams, makeConflictResolveRequest}`.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- contains 5 days of audit_log rows (varied actions, actors, entities). Phase-08 plan consumes the fixture.
- Synthetic fixture: `fixtures/soak/8h-offline.sql` (NEW for §6.6 + soak harness; ownership: this plan) -- the seed used by `src-tauri/tests/soak/eight_hour_offline.rs` to simulate 8h of offline operation per PRD §1.3 / §5 soak harness.
- Vacuum fixture: `fixtures/edge/vacuum-edge.sql` (NEW for §6.5 + §6.8) -- mixed pre-90-day and post-90-day audit rows with `dirty=0` AND `dirty=1`; verifies vacuum scope (only `dirty=0` pruned).

**Tool prerequisites:**
- Inherited from phase-01..07 execution.
- A11y: `@axe-core/cli` (NEW: `pnpm add -D @axe-core/cli` -- phase-01 §7.11 declared the script; phase-08 finalises with the full-page walk).
- i18n lint: `@babel/parser` + `@babel/traverse` + `@babel/types` (NEW: `pnpm add -D @babel/parser @babel/traverse @babel/types` -- for the AST walker in §7.9).
- RTL icon lint: same babel toolchain reused for §7.18.
- Soak harness: `tracing-flame` for memory profiling during the 8h run (NEW Rust dev-dep, `cargo add --dev tracing-flame`). `procfs` (Linux only) for RSS sampling.
- Prometheus metrics: `prom-client` already in the server's `package.json` from §7.17 (NEW server-side dep, `pnpm --filter sync-server add prom-client`).

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`. Phase-08 enforces JWT-role assertions on `/audit/query` and `/metrics` endpoints; the replay matrix lives elsewhere.
- 3xN conflict matrix exhaustively -- the resolver round-trip for the 3 cells (`additive-only`, `last-write-wins`, `manual`) lands in this plan; the cross-product across every entity lives in `sync-conflicts.md`.
- Full visual page-by-page i18n / RTL sweep -- phase-08 OWNS the `i18n-rtl.md` cross-cutting plan; this test plan ships the lint scripts and the end-to-end gating.
- Performance soak under hardware-realistic conditions (real disk, real network) -- the simulated harness is owned here; `performance-soak.md` aggregates results.
- Receipt print success >99% per PRD §1.3 -- the underlying metric is emitted by phase-05's lock workflow; phase-08 only reads from `metrics_events` and surfaces the rate.

**Cross-phase commands:** none unique. Phase-08 wires the resolver UI on top of `sync::list_conflicts` and `sync::resolve_conflict` from phase-01 (cross-referenced; phase-01 test verified the IPC mechanism; phase-08 verifies the end-to-end UI round-trip). Phase-08 adds 5 new commands: `audit::query`, `audit::vacuum_now`, `diagnostics::summary`, `sync::list_conflicts` (updated return type per §7.11), `sync::resolve_conflict` (unchanged signature; new audit-emission behavior per §3 + §4).

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`AuditFilter` value object (`src-tauri/src/domains/audit/domain/value_objects/audit_filter.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `AuditFilter::try_new` | `accepts_all_14_phase_01_to_07_action_enums` | Per phase-01 §7.36 final enum: 14 values including `daily_close_run` (phase-07 §7.18). |
| `AuditFilter::try_new` | `accepts_all_15_entity_table_names_per_8_8` | Per §7.8: `users, settings, check_types, check_subtypes, doctors, doctor_check_pricing, operators, operator_specialties, operator_shifts, patients, visits, inventory_items, inventory_consumption_map, inventory_adjustments, audit_log`. |
| `AuditFilter::try_new` | `rejects_entity_id_prefix_below_4_chars` | Per §7.24: min 4. |
| `AuditFilter::try_new` | `rejects_entity_id_prefix_above_36_chars` | Per §7.24: max 36 (full UUID length). |
| `AuditFilter::try_new` | `rejects_text_below_2_chars` | Per §7.6: min 2 -- mirrors the search-input convention. |
| `AuditFilter::try_new` | `rejects_from_after_to` | -- |
| `AuditFilter::crosses_90_day_boundary` | `returns_true_when_from_before_now_minus_90d_and_to_after_minus_90d` | Per §7.4 cross-boundary paginator gate. |
| `AuditFilter::crosses_90_day_boundary` | `returns_false_for_strictly_local_or_strictly_server` | -- |

**`AuditQueryService` pure helpers (`src-tauri/src/domains/audit/service/audit_query_service.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `AuditQueryService::route_decision` | `routes_strictly_local_when_from_after_minus_90d` | -- |
| `AuditQueryService::route_decision` | `routes_strictly_server_when_to_before_minus_90d` | -- |
| `AuditQueryService::route_decision` | `routes_merged_when_crosses_boundary` | Per §7.4. |
| `AuditQueryService::merge_paginate` | `merges_local_and_server_streams_by_at_desc_id_desc_stable` | Two sorted streams merged by `(at DESC, id DESC)`; resulting stream is stable. Per §7.4 step 3. |
| `AuditQueryService::merge_paginate` | `boundary_divider_inserted_when_source_switches` | Per §7.4: when a page contains rows from BOTH local and server, the rendering layer gets a "Crossed local retention boundary" divider record. |
| `AuditQueryService::merge_paginate` | `cursor_carries_source_field_for_resume` | Per §7.4: cursor format `{ at, id, source: 'local' | 'server' }`. |
| `AuditQueryService::build_local_sql` | `applies_actor_action_entity_id_prefix_text_filters` | Per §7.24: SQL has `AND entity_id LIKE :prefix || '%'`. |
| `AuditQueryService::build_local_sql` | `falls_back_to_INSTR_for_free_text_match_on_delta` | Per §4 local-step 3: `INSTR(delta, :free_text)`. |

**`AuditVacuumJob` (`src-tauri/src/domains/audit/service/audit_vacuum_job.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `AuditVacuumJob::cutoff_for_action_type` | `audit_log_cutoff_now_minus_90_days` | Per §7.21 step 1. |
| `AuditVacuumJob::cutoff_for_action_type` | `metrics_events_cutoff_now_minus_30_days` | Per §7.21 step 3. |
| `AuditVacuumJob::compute_run_decision_at_boot` | `runs_immediately_when_last_audit_vacuum_at_above_24h_ago` | Per §7.2. |
| `AuditVacuumJob::compute_run_decision_at_boot` | `skips_when_last_audit_vacuum_at_within_24h` | -- |
| `AuditVacuumJob::compute_run_decision_at_boot` | `runs_immediately_when_last_audit_vacuum_at_is_null_fresh_install` | -- |
| `AuditVacuumJob::compute_next_wakeup_time` | `targets_local_03_00_next_day` | -- |
| `AuditVacuumJob::on_error_retry_after_1h` | `single_retry_then_waits_for_24h_tick` | Per §7.2. |
| `AuditVacuumJob::vacuum_self_audit_row` | `uses_zero_uuid_sentinel_for_entity_id` | Per §7.3: `entity_id = '00000000-0000-0000-0000-000000000000'`. |
| `AuditVacuumJob::vacuum_self_audit_row` | `delta_carries_audit_purged_metrics_purged_cutoffs` | Per §7.21 step 5. |

**`vacuum_unsynced_safe` repo helper** (the bypass path that does NOT flip `dirty=1`)

| Module | Test | Asserts |
|-|-|-|
| `AuditRepo::vacuum_unsynced_safe` | `does_not_flip_dirty_on_pruned_rows` | Per phase-01 §7.31: after vacuum, the pruned rows have `dirty=0` (sets `deleted_at` only, NEVER touches `dirty`). |
| `AuditRepo::vacuum_unsynced_safe` | `predicate_includes_dirty_eq_0_and_deleted_at_is_null` | Type-level proof via the function signature: callers can't pass a dirty-row filter. Per §7.1 trait-signature refactor. |
| `AuditRepo::vacuum_unsynced_safe` | `outbox_remains_empty_after_vacuum` | The vacuum NEVER enqueues outbox rows. Asserted by `SELECT COUNT(*) FROM outbox` before/after vacuum. |
| `MetricsRepo::vacuum_older_than` | `hard_deletes_with_no_audit_or_outbox_writes` | Per §7.21: `metrics_events` is local-only; non-syncable; deletion is hard, leaves no trace. |

**`ConflictResolveService` (`src-tauri/src/domains/sync/service/conflict_resolve_service.rs`)** -- expanded from phase-01's stub.

| Module | Test | Asserts |
|-|-|-|
| `ConflictResolveService::compute_resolve_op_id` | `sha256_stable_across_calls_with_same_input` | Per §7.22: `sha256(opId|choice|merged_canonical_json)` deterministic. |
| `ConflictResolveService::compute_resolve_op_id` | `differs_when_choice_or_merged_differs` | -- |
| `ConflictResolveService::detect_already_resolved` | `returns_already_resolved_response_when_processed_op_cached` | Per §7.22 server step 1: ProcessedOp cache hit returns the cached response body. |
| `ConflictResolveService::detect_already_resolved` | `returns_409_when_different_resolution_attempted_for_resolved_conflict` | Per §7.22 server step 2. |

**`DiagnosticsSummary` (`src-tauri/src/domains/diagnostics/service/diagnostics_summary.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `DiagnosticsSummary::compute` | `lock_latency_p95_ms_from_metrics_events` | Per §7.17: reads `metrics_events` table for `kind in ('lock_start','lock_end')`; computes p95 of `duration_ms`. |
| `DiagnosticsSummary::compute` | `outbox_depth_reads_count_where_parked_zero` | -- |
| `DiagnosticsSummary::compute` | `last_sync_at_returns_max_of_last_pushed_at_and_last_pulled_at` | -- |
| `DiagnosticsSummary::compute` | `conflict_count_7d_filters_metrics_events_kind_sync_conflict` | -- |
| `DiagnosticsSummary::compute` | `receipt_print_success_rate_30d_computes_from_metrics_events` | `success / (success + fail)` over 30-day window. |

**`SyncPill` event-driven helpers (`src/components/shell/sync-pill.tsx`)** (TS unit, but documented here for completeness)

| Module | Test | Asserts |
|-|-|-|
| `SyncPill::shouldNavigateToConflicts` | `returns_true_when_status_error_or_outbox_above_zero_or_conflicts_pending` | Per §7.14. |
| `SyncPill::shouldNavigateToConflicts` | `returns_false_when_idle_and_outbox_empty_and_no_conflicts` | -- |

### §1.2 TS pure functions / value objects

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/audit.ts::AuditFilterSchema` | `requires_from_le_to` | -- |
| `src/lib/schemas/audit.ts::AuditFilterSchema` | `entity_id_prefix_min_4_max_36` | Per §7.24. |
| `src/lib/schemas/audit.ts::AuditRowSchema` | `includes_dirty_boolean_per_7_15` | -- |
| `src/lib/schemas/sync.ts::ConflictResolutionSchema` | `requires_choice_and_optional_merged_object` | Per phase-01 §3.2 contract. |
| `src/lib/audit/entity-routes.ts::routeForEntity` | `maps_each_of_15_entities_to_detail_route_or_omitted` | Per §7.7: `users -> /admin/users/:id`, `visits -> /reception/visits/:id`, etc. `settings` and `audit_log` return `null` (no detail route). |
| `src/lib/i18n/lint-i18n.ts` | (this is the standalone script; tested via separate harness) | -- |
| `src/lib/rtl/icons.ts::DirectionalChevron` | `renders_forward_arrow_with_rtl_rotate_180_class` | Per §7.18: the helper wraps chevrons so the lint script passes. |
| `src/stores/sync-status-store.ts` (extends phase-01) | `conflicts_count_polls_at_2s_via_diagnostics_summary` | After phase-08, the badge count comes from `diagnostics::summary` instead of the in-memory cache. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/audit/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::audit::domain` |
| `src-tauri/src/domains/audit/service/**` (AuditQueryService, AuditVacuumJob, merge_paginate) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::audit::service` |
| `src-tauri/src/domains/audit/infrastructure/**` (vacuum_unsynced_safe path, server query client) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::audit::infrastructure` |
| `src-tauri/src/domains/diagnostics/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::diagnostics` |
| `src-tauri/src/domains/metrics/repositories/**` (vacuum_older_than) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::metrics` |
| `src/features/audit/**`, `src/features/sync/conflicts/**`, `src/lib/schemas/audit.ts`, `src/lib/audit/entity-routes.ts`, `src/lib/rtl/icons.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/{audit,sync/conflicts}/**,src/lib/schemas/audit.ts,src/lib/audit/entity-routes.ts,src/lib/rtl/icons.ts"` |
| `src/pages/audit/**`, `src/pages/sync/**`, `src/components/audit/**`, `src/components/sync/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/audit/**,src/pages/sync/**,src/components/audit/**,src/components/sync/**"` |
| `sync-server/src/app/domains/audit/service/**` | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/audit/presentation/**` (`/audit/query`) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |
| `sync-server/src/app/sync/conflicts/**` (resolve service + idempotency from §7.22) | >= 95% lines (conflict-resolution code is critical) | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/routes/metrics.ts`, `healthz.ts` | >= 85% lines | `pnpm --filter sync-server test:coverage` |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/audit_phase08.rs` (already exists at HEAD).
- Auxiliary file: `src-tauri/tests/soak/eight_hour_offline.rs` (NEW; soak harness owned by §5).

**New scenarios in `audit_phase08.rs`:**

| Scenario | Asserts |
|-|-|
| `audit_query_filters_local_actor_action_entity_id_prefix_text_in_one_select` | All 5 filters combine with `AND`; the SQL plan uses `audit_log_tenant_at` + applicable secondary index. |
| `audit_query_routes_strictly_local_for_recent_30_day_range` | Per §7.4 route decision: no server call. |
| `audit_query_routes_strictly_server_for_above_90_day_range` | Per §7.4: server endpoint hit; local SELECT skipped. |
| `audit_query_routes_merged_when_range_crosses_90_day_boundary` | Per §7.4: both local and server paths executed; merge by `(at DESC, id DESC)`. |
| `audit_query_merge_paginator_inserts_boundary_divider_record` | Per §7.4 step 3 + §7.25: the rendered output contains a divider when source switches. |
| `audit_query_cursor_carries_source_field_and_resumes_correctly` | Page 1 ends mid-local; cursor preserves source; page 2 resumes correctly. |
| `audit_query_response_includes_dirty_boolean_per_7_15` | -- |
| `audit_vacuum_run_soft_deletes_audit_log_older_than_90_days_with_dirty_zero` | Per §7.1 + §7.31: pre-seed audit rows >90d ago with `dirty=0`; vacuum; rows have `deleted_at != null`; `dirty` STILL 0 (never flipped to 1). |
| `audit_vacuum_run_skips_audit_log_with_dirty_eq_1` | Pre-seed >90d ago with `dirty=1`; vacuum; row preserved (not pruned). |
| `audit_vacuum_run_does_not_enqueue_outbox_for_pruned_rows` | After vacuum, `outbox` count unchanged. Per phase-01 §7.31. |
| `audit_vacuum_writes_one_self_audit_row_with_action_vacuum` | Per §7.3: `entity='audit_log'`, `entity_id='00000000-...'`, `action='vacuum'`, `delta` contains `{ audit_purged, metrics_purged, cutoffs }`. |
| `audit_vacuum_run_extends_to_metrics_events_older_than_30_days_per_7_21` | metrics_events rows >30d are HARD-deleted (no audit, no soft-delete). |
| `audit_vacuum_run_updates_last_audit_vacuum_at_per_7_2` | Per §7.19: `sync_state.last_audit_vacuum_at` updated to the run's start time. |
| `audit_vacuum_runs_at_app_start_when_last_run_above_24h_ago` | Per §7.2 + §1.1 helper. |
| `audit_vacuum_skips_at_app_start_when_last_run_within_24h` | -- |
| `audit_vacuum_retries_after_1h_on_error_then_waits_for_24h_tick` | Per §7.2: mock the SQL to fail once; assert one retry; subsequent tick is the regular 24h ahead. |
| `audit_vacuum_scheduled_wakeup_targets_local_03_00` | Tokio task target time check. |
| `migration_008_creates_polish_DDL_per_7_19` | `sync_state.last_audit_vacuum_at` column exists. Idempotent on populated DB. |
| `conflict_resolver_round_trip_keep_local_replays_outbox_row` | (a) parked conflict exists locally + server; (b) call `sync::resolve_conflict({choice:'local'})`; (c) outbox.parked flips to 0; (d) re-push succeeds; (e) audit row `action='conflict_resolve'` written; (f) `<ConflictResolverPanel>` removes the resolved row. |
| `conflict_resolver_round_trip_keep_server_drops_local_op` | Mirror for `choice:'server'`. |
| `conflict_resolver_round_trip_merge_validates_against_entity_schema` | A merged payload that fails the entity's TypeBox schema returns 400; the conflict stays parked; the UI surfaces the field-level error. |
| `conflict_resolver_emits_audit_log_row_via_sync_pull_after_server_commit` | Per §3 Server gap closure (forward-ref phase-09 §3 implementation): the resolver writes `audit_log` server-side; the next pull brings it down. |
| `conflict_resolver_idempotent_on_resolve_op_id_per_7_22` | Replay the same resolution -> identical response; no double-write. Per §7.22 + §1.1 `compute_resolve_op_id` test. |
| `conflict_resolver_returns_409_already_resolved_when_different_resolution_for_resolved_conflict` | Per §7.22: second attempt with different choice -> 409. |
| `conflict_list_endpoint_durable_across_app_restart_per_7_11` | Pre-seed a parked conflict server-side. Quit the app. Relaunch. Navigate to `/sync/conflicts` -- the list renders the row (loaded from server, not in-memory). |
| `sync_pill_onclick_navigates_to_sync_conflicts_when_status_error` | Per §7.14: force `<SyncPill>` to error state; click; assert navigation. |
| `sync_pill_onclick_navigates_when_outbox_count_above_zero` | -- |
| `sync_pill_onclick_does_not_navigate_when_idle_and_outbox_empty` | -- |
| `diagnostics_summary_returns_lock_latency_p95_outbox_depth_last_sync_conflict_count_receipt_print_rate` | Per §7.17. |
| `diagnostics_summary_reads_metrics_events_table_only_for_lock_and_print_metrics` | Tied to phase-01 §7.28's table. |
| `metrics_events_vacuum_runs_inside_audit_vacuum_job_per_7_21` | Single composite vacuum call prunes both tables in the same job. |

### §2.2 Tauri IPC handler tests

| Command | Happy-path test | Error-path test |
|-|-|-|
| `audit_query` | `returns_filtered_paged_results_in_local_or_remote_or_merged_mode` | `non_superadmin_returns_forbidden_per_7_23` |
| `audit_vacuum_now` | `runs_vacuum_immediately_returns_vacuumresult` | `non_superadmin_returns_forbidden` |
| `sync_list_conflicts` | `returns_unresolved_conflicts_paged_per_7_11_signature` | -- |
| `sync_resolve_conflict` | `keeps_local_or_server_or_merged_and_writes_audit_emits_outbox_unpark` | `unknown_op_id_returns_not_found` |
| `diagnostics_summary` | `returns_summary_struct_per_7_17` | `non_superadmin_returns_forbidden_if_route_gated` (cross-ref phase-07 §7.17 mirror; phase-08 keeps it open to all logged-in users -- the modal hides itself per role in `<UserMenu>`) |

### §2.3 Sync server route handlers

File: `sync-server/test/audit/audit-phase08.test.ts` + `sync-server/test/sync/conflicts-phase08.test.ts` + `sync-server/test/metrics/metrics-phase08.test.ts`.

| Route | Test | Asserts |
|-|-|-|
| `GET /audit/query` | `returns_paged_audit_rows_sorted_at_desc_id_desc_per_7_5` | Cursor base64 `{ at, id }`; stable across ties. |
| `GET /audit/query` | `requires_superadmin_jwt_role` | Per §3 Server. |
| `GET /audit/query` | `applies_all_filters_per_7_6_typebox_schema` | actor + action + entity + entity_id_prefix + date range + text. |
| `GET /audit/query` | `text_filter_substring_against_json_delta_v1_only` | Per §4: full-text deferred; v1 uses `LIKE %text%` against the canonicalized JSON. |
| `GET /audit/query` | `tenant_scoped_via_jwt_entity_id` | Cross-tenant audit invisible. |
| `GET /sync/conflicts` | `returns_only_unresolved_conflicts_paged_by_parked_at_desc_per_7_11` | -- |
| `GET /sync/conflicts` | `caps_at_100_per_request_per_7_11` | -- |
| `GET /sync/conflicts` | `requires_superadmin_jwt_role_per_7_23` | -- |
| `POST /sync/conflicts/:opId/resolve` | `keep_local_returns_applied_writes_audit_log_row` | Per §3 Server gap closure: `conflict_resolve` audit row written in the SAME prisma.$transaction as the conflict resolve. |
| `POST /sync/conflicts/:opId/resolve` | `keep_server_returns_canonical_row_writes_audit` | -- |
| `POST /sync/conflicts/:opId/resolve` | `merged_validates_against_entity_schema_or_400` | -- |
| `POST /sync/conflicts/:opId/resolve` | `idempotent_on_resolve_op_id_returns_cached_response` | Per §7.22 server step 1. |
| `POST /sync/conflicts/:opId/resolve` | `returns_409_already_resolved_when_different_resolution` | Per §7.22 server step 2. |
| `POST /sync/conflicts/:opId/resolve` | `transaction_rolls_back_audit_when_resolve_fails` | If `conflicts.resolveTx` errors after `audit.appendTx`, both rolled back. |
| `GET /metrics` | `returns_prometheus_exposition_format_when_internal_token_present` | Per §7.17: `Content-Type: text/plain`; histogram + counter + gauge metrics. |
| `GET /metrics` | `returns_404_when_internal_token_absent_or_invalid` | The endpoint is gated; no JWT auth -- only the env-controlled token. |
| `GET /metrics` | `does_not_leak_tenant_specific_data` | Aggregate metrics only; no per-tenant identifiers in labels. |
| `GET /healthz` | `returns_db_ok_redis_ok_migrations_applied_per_7_17_enrichment` | Per §7.17 + phase-09 §3 healthz wiring (forward-ref). |
| `GET /healthz` | `returns_db_fail_when_db_unreachable` | Per phase-09 §3 healthz wiring. |
| `GET /healthz` | `does_not_require_jwt` | Liveness endpoint. |

### §2.4 React Query mutation / query flows

| Hook | Test | Asserts |
|-|-|-|
| `useAuditQuery` | `routes_local_or_remote_or_merged_via_filter_range` | -- |
| `useAuditQuery` | `passes_entity_id_prefix_through_ipc_per_7_24` | -- |
| `useAuditQuery` | `boundary_divider_record_renders_separator_in_table` | Per §7.25. |
| `useConflictsList` | `loads_from_server_endpoint_on_mount_per_7_11` | -- |
| `useConflictsList` | `refetches_on_sync_conflict_event` | -- |
| `useConflictResolve` (mutation) | `invalidates_conflicts_list_after_resolve` | -- |
| `useConflictResolve` | `surfaces_409_already_resolved_with_toast_and_refetch` | Per §7.22 frontend handler. |
| `useDiagnosticsSummary` | `returns_summary_for_user_menu_modal_per_7_17` | -- |

Components covered (each `describe.each([['ltr'],['rtl']])`):
- `<AuditFilters>` 6 inputs per §7.24: actor combobox + action chips (12+ values per phase-01 §7.36) + entity dropdown (15 names per §7.8) + `<EntityIdSubstringInput>` + date range + free-text.
- `<AuditTable>` row Entity cell links to entity-detail route per §7.7.
- `<AuditTable>` Pending-sync column renders `<DirtyDot>` per §7.15.
- `<AuditTable>` server-backed badge in header when mode is `server` or `merged` per §7.25.
- `<AuditTable>` per-row boundary divider when source switches per §7.4.
- `<DeltaViewer>` two-column diff; identical fields collapsed; password / token fields show `[REDACTED]` per phase-01 §7.14 carry-over.
- `<ConflictList>` one row per parked conflict; sorted by `createdAt DESC`.
- `<ConflictResolverPanel>` side-by-side local-vs-server payloads via `<DeltaViewer>`.
- `<ConflictResolverPanel>` Keep Local / Keep Server / Merge actions; Merge opens `<MergeEditor>`.
- `<MergeEditor>` per-field local / server / manual radio picker.
- `<ServerBackedBadge>` "querying server" pill style per `.claude/rules/design-system.md` §5.2.
- `<SyncPill>` onClick navigation per §7.14; keyboard `Enter`/`Space` activates the same.
- `<UserMenu>` "Diagnostics" entry opens `<DiagnosticsModal>` rendering the §7.17 summary.
- `<AuditPage>` (`/audit`) and `<SyncConflictsPage>` (`/sync/conflicts`) wrapped in `<RequireRole roles={['superadmin']}>` per §7.23.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /audit/query` (request) | `AuditQuerySchema` (per §7.6 + §7.24 entity_id_prefix) | All required fields; entity_id_prefix optional, min 4 max 36. |
| `GET /audit/query` (response) | `AuditQueryResponseSchema` | `{ rows: AuditRow[], next_cursor: string | null }`. |
| `GET /sync/conflicts` (response) | `ConflictListResponseSchema` (per §7.11) | Array of `ConflictParkedSchema`. |
| `POST /sync/conflicts/:opId/resolve` (request) | `ConflictResolveBodySchema` (extended from phase-01 with `resolve_op_id` per §7.22) | -- |
| `POST /sync/conflicts/:opId/resolve` (response) | `ConflictResolveResponseSchema` | `{ status: 'applied' | 'already_resolved', resolvedAt }`. |
| `GET /metrics` (response) | (no JSON schema; Prometheus exposition text format) | Verify `Content-Type` + sample histogram/counter/gauge presence via regex assertion. |
| `GET /healthz` (response) | `HealthSchema` extended per §7.17 (widened to `'ok' | 'fail'`) | Captured response. |

### §3.2 IPC shape contract

| IPC command | Rust struct | TS schema |
|-|-|-|
| `audit_query` | `AuditPage { rows: Vec<AuditEntry>, next_cursor: Option<String>, mode: AuditQueryMode }` | (NEW) `AuditPageSchema = z.object({ rows: z.array(AuditRowSchema), next_cursor: z.string().nullable(), mode: z.enum(['local', 'server', 'merged']) })` -- per §7.25 mode field surfaces the `<ServerBackedBadge>` rendering decision. |
| `audit_vacuum_now` | `VacuumResult { audit_purged: u32, metrics_purged: u32, cutoffs: VacuumCutoffs }` | `VacuumResultSchema` |
| `sync_list_conflicts` | `Vec<ConflictParked>` (return type promoted per §7.11) | `z.array(ConflictParkedSchema)` |
| `sync_resolve_conflict` | `()` | `z.void()` |
| `diagnostics_summary` | `DiagnosticsSummary { lock_latency_p95_ms, outbox_depth, last_sync_at, conflict_count_7d, receipt_print_success_rate_30d }` | `DiagnosticsSummarySchema` |
| (Error envelope -- fixed) | `AppError` (new variants `AuditError::Forbidden`, `ConflictError::AlreadyResolved`, `VacuumError::*`) | `AppErrorSchema` -- shared schema. |

### §3.3 Sync envelope contract

- **`conflict_resolve` audit row push.** Per §3 Server gap closure: the server writes the audit row in the same Prisma tx; the next client pull brings it down using the existing `audit_log` additive-only push contract. Phase-08 verifies this round-trip end-to-end.
- **Snapshot files**:
  - `expected/audit/audit-query-response-canonical.json.sha256`
  - `expected/sync/conflict-list-response-canonical.json.sha256`
  - `expected/sync/conflict-resolve-applied-response.json.sha256`
  - `expected/sync/conflict-resolve-already-resolved-response.json.sha256`
  - `expected/metrics/prometheus-exposition-sample.txt.sha256`

---

## §4 E2E Tests (Pyramid Layer 4)

Specs live under `e2e/specs/audit/` and `e2e/specs/sync/`.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `audit-search-filter-and-delta-expand.e2e.ts` | Mariam (superadmin) | 1) Navigate `/audit`. 2) Apply filters: action=`lock`, entity=`visits`, date=last 7 days. 3) Click a row to expand. 4) Verify `<DeltaViewer>` renders two-column diff with collapsed identical fields. | -- |
| `audit-entity-drilldown-to-detail-page.e2e.ts` | Mariam | 1) `/audit` row Entity cell click. | Navigates to `/admin/users/:id` (or `/reception/visits/:id`, etc.) per §7.7 routing map. |
| `audit-cross-boundary-merged-pagination.e2e.ts` | Mariam | 1) Set date range from 100 days ago to today (crosses 90-day cliff). 2) Scroll through pages. | `<ServerBackedBadge>` renders; boundary divider appears at the cliff. Per §7.4 + §7.25. |
| `audit-entity-id-prefix-filter.e2e.ts` | Mariam | Per §7.24: enter 8-char prefix; verify filter works. | -- |
| `conflict-resolver-keep-local-round-trip.e2e.ts` | Mariam | 1) Force a `manual` conflict on `settings.dye_cost_iqd` (two devices). 2) Navigate `/sync/conflicts`. 3) Click conflict. 4) Click Keep Local. 5) Verify resolved; outbox row unparks; re-push succeeds. | Audit row `action='conflict_resolve'` written; pull surfaces it on both devices. |
| `conflict-resolver-keep-server-replaces-local.e2e.ts` | Mariam | 1) Resolve with Keep Server. | Local row matches server canonical; conflict disappears. |
| `conflict-resolver-merge-flow.e2e.ts` | Mariam | 1) Open `<MergeEditor>`; pick local for field A, server for field B, manual override for field C. 2) Submit. | Merged payload pushed; conflict resolved. |
| `conflict-resolver-409-already-resolved-refresh.e2e.ts` | Mariam | Per §7.22: simulate network-drop-after-commit-before-ack; click resolve twice. | Second click surfaces `errors:sync.already_resolved` toast; refreshes list. |
| `conflict-list-durable-across-restart.e2e.ts` | Mariam | Per §7.11: pre-seed parked; quit app; relaunch; navigate `/sync/conflicts`. | List renders the conflict (from server, not in-memory). |
| `sync-pill-onclick-navigates-on-error.e2e.ts` | Any | Per §7.14: force pill error state; click. | Navigates `/sync/conflicts`. |
| `sync-pill-onclick-keyboard-enter-and-space.e2e.ts` | Any | Per §7.14: focus pill; press Enter. | Same navigation. |
| `diagnostics-modal-shows-summary.e2e.ts` | Any | Per §7.17: open `<UserMenu>`; click Diagnostics. | Modal renders 5 metrics. |
| `audit-route-role-guard-for-non-superadmin.e2e.ts` | Asma (accountant) | Attempt `/audit`. | Redirected to `/no-access` per §7.23. |
| `sync-conflicts-route-role-guard.e2e.ts` | Asma | Attempt `/sync/conflicts`. | Redirected per §7.23. |
| `audit-vacuum-now-trigger-from-test-only-IPC.e2e.ts` | Mariam | Pre-seed audit rows >90 days old; trigger vacuum. | Rows soft-deleted; self-audit row written. |

### §4.2 Failure-path flows

- **`offline-audit-query-local-only-when-range-within-90d.e2e.ts`** -- Set offline; navigate `/audit`; assert query works against local DB; no network call.
- **`server-5xx-during-audit-query-merged-graceful-degradation.e2e.ts`** -- Force server 5xx for the remote half; UI surfaces a partial-result banner; local rows still render.
- **`conflict-resolver-during-mid-flight-network-drop.e2e.ts`** -- Per §7.22 end-to-end: drop network after server commits but before 200 returns; client retries; idempotency cache returns same response; no double-write.
- **`audit-vacuum-error-retries-after-1h.e2e.ts`** -- Per §7.2: mock SQL fail; assert one retry 1h later; subsequent tick is regular 24h.
- **`vacuum-does-not-flip-dirty-on-pruned-rows.e2e.ts`** -- Verify the `dirty=0` invariant after vacuum (forensic check via test-only IPC).
- **`audit-query-receptionist-blocked-at-three-layers.e2e.ts`** -- Per §7.23: UI hides link; IPC returns Forbidden; server `/audit/query` returns 403.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-conflict-resolver-keep-local.e2e.ts` | Device A + B both edit `settings.dye_cost_iqd` offline; both reconnect; B's push receives 409 + ConflictParked. Mariam on B opens `/sync/conflicts`; resolves Keep Local. | A's setting overwritten; B's value wins; audit row pulled to both devices. |
| `two-device-conflict-resolver-on-visits-manual-policy.e2e.ts` | Cross-references phase-05's `visits` manual policy. Device A edits draft; Device B locks same draft; A's push receives 409. Mariam resolves Merge with field-level pick. | Resolved visit reflects the merge; audit row pulled. |
| `vacuum-on-both-devices-converges-via-pull.e2e.ts` | Vacuum runs on Device A; metric_events rows pruned locally; the self-audit row pushes via outbox; Device B pulls and sees the vacuum audit row in its `audit_log` table. Per phase-01 §7.31 carve-out. | The pruned rows on Device A do NOT propagate as deletes (per phase-01 §7.31 -- vacuum doesn't flip dirty); Device B still has those local rows until its own vacuum runs. |
| `soak-eight-hour-offline.e2e.ts` | Per §5 soak harness: simulated 8h offline operation; reconnect; assert §7.16 quantitative criteria. | All criteria met. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: `<AuditPage>` in both directions.** Eyebrow rule mirrored; filter chips wrap correctly in RTL; table numeric columns aligned to page edge in RTL.
- **Visual: `<ConflictResolverPanel>` side-by-side layout.** Verify local-vs-server columns mirror in RTL (local on the right in RTL); `<MergeEditor>` field-pickers render correctly.
- **Visual: `<DiagnosticsModal>`.** 5 KPI tiles + sync conflict counter + receipt print rate. Visual verification per design-system §5.5.
- **Soak run end-to-end.** Per §5 + §6.6: kick off `src-tauri/tests/soak/eight_hour_offline.rs`; wait 8 hours; verify the Markdown report at `target/soak-report.md` shows all criteria met.
- **a11y full sweep.** Per §7.13: open every page in the app; run `axe-core` browser extension; verify zero serious/critical violations.
- **Color contrast manual check.** Per §6.7 + design-system: random sample 20 text/background pairs; verify >=4.5:1 normal / >=3:1 large.
- **Screen reader announcements.** With NVDA / VoiceOver: navigate `/audit` -> filters -> table -> row expand. Verify `<DeltaViewer>` JSON diff announces correctly.

### §5.2 Cross-references to `personas.md`

- `personas.md` -> **P3 Mariam the Superadmin** -> audit + conflict resolver flows. Required for §8 DoD.
- `personas.md` -> **P4 Two-Device Conflict** -> end-to-end (P4 is THE persona for phase-08 conflicts). Reinforcement.
- `personas.md` -> **P5 Year-End Audit** -> step 6 (mariam resolves a synthetic conflict). Reinforcement.

**Canonical: P3 Mariam the Superadmin.**

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **90-day boundary uses Asia/Baghdad local.** Per phase-07 §7.8 + audit query semantics: the 90-day cliff is computed at local midnight. Asserted in `audit_query_routes_strictly_local_for_recent_30_day_range`.
- **Daily vacuum target 03:00 local.** Asserted in `audit_vacuum_scheduled_wakeup_targets_local_03_00`.
- **Missed-run handling.** Per §7.2: laptop closed past 03:00; on next boot, vacuum runs immediately if >24h since last run.
- **Clock skew vs server.** Per phase-01: server-authoritative `at` on pulled audit rows.
- **DST defensive.** CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `domains/audit/`.

### §6.2 i18n & RTL

- **en/ar swap on every phase-08 route.** `/audit` + `/sync/conflicts`. Strings from `audit.*` + `sync.*` + `a11y.*` namespaces.
- **Arabic-Indic numerals on audit timestamps + entity_id prefix.** Per phase-02 §7.12.
- **RTL layout invariants.** `<DeltaViewer>` columns mirror; chevrons rotate via `rtl:rotate-180` per §7.18.
- **Mixed-direction in `<DeltaViewer>`.** Audit row with Arabic patient name + ASCII UUID renders correctly.
- **`pnpm lint:i18n` runs and reports zero violations.** Per §7.9. Sets the convention for every later commit.
- **`pnpm lint:rtl` runs and reports zero violations.** Per §7.18.

### §6.3 Offline & Network

- **Audit query offline for local-range.** Per §7.4: ranges within 90 days work fully offline.
- **Audit query above-90d returns clear error offline.** UI surfaces "Server unreachable for archived rows" message.
- **Conflict-resolve offline rejected.** The resolve flow requires server commit; offline returns `AppError::Sync(NetworkUnavailable)`.
- **Vacuum offline.** Vacuum is local-only; runs regardless of network state.
- **Soak: 8-hour offline.** Per §5 + §7.16: all criteria asserted by the harness.

### §6.4 Concurrency & Conflicts

- **3xN conflict matrix entry points.** Phase-08 owns the resolver round-trip for `manual` (settings + visits), `last-write-wins` (everything else), `additive-only` (audit_log + inventory_adjustments). The matrix is owned by `sync-conflicts.md`; phase-08 ships the resolver UI that exercises it.
- **Conflict resolver round-trip end-to-end.** Per §4.3: parked -> resolve -> audit row + outbox unpark + re-push.
- **Idempotency under network drops.** Per §7.22.
- **Already-resolved 409.** Per §7.22.
- **`sync_conflict` metric auto_resolved=false invariant.** Soak harness asserts ZERO rows with `auto_resolved=true`. Per §7.16.

### §6.5 Crash & Recovery

- **SIGKILL during vacuum tx.** Vacuum rolls back atomically; `last_audit_vacuum_at` not updated; next boot retries.
- **SIGKILL during conflict resolve tx.** Server tx rolls back; client outbox still parked; user can retry.
- **SQLite WAL after crash.** Per phase-01 baseline.
- **Disk full during vacuum.** `AppError::Db` returned; vacuum reschedules for 1h later.
- **Vacuum partial run.** If vacuum errors mid-job (e.g., metrics_events delete fails after audit_log soft-delete succeeded), the tx rolls back both; `last_audit_vacuum_at` not updated.
- **8h soak crash recovery.** Per §7.16: memory growth < 50MB asserts no leak that would crash a real session.

### §6.6 Scale & Performance

- **Audit query at 100k rows local.** < 500ms p99 per §5 perf-verification + §9 default (90-day window).
- **Audit query merged across 100k local + 100k server.** < 2s p95 (server query dominates).
- **Vacuum at 90-day rowset (~25k typical).** < 10s per §7.16.
- **Conflict list at 100 parked.** Server endpoint < 200ms p95.
- **8h soak quantitative criteria.** Per §7.16:
  - Sync push throughput >= 50 ops/sec sustained.
  - Outbox steady-state depth <= 800 rows.
  - p95 lock latency < 30s.
  - Memory growth < 50MB.
  - Audit-vacuum < 10s for 90-day rowset.
  - Zero `sync_conflict` rows with `auto_resolved=true`.
- **Prometheus `/metrics` scrape latency.** < 100ms p95.
- **`diagnostics::summary` IPC latency.** < 50ms p99 (reads local `metrics_events`).

### §6.7 Security & Permissions

- **`/audit` route + IPC role gate.** Per §7.23: superadmin only at three layers (route + IPC + server).
- **`/sync/conflicts` route role gate.** Same. Per §7.23.
- **`/metrics` gated by `X-Internal-Token`.** Per §7.17: no JWT auth; env-controlled token. Wrong / missing token -> 404 (intentional, not 401, to avoid revealing the endpoint exists).
- **`/audit/query` server JWT-role check.** Per §3 server: 403 for non-superadmin.
- **JWT tampering.** Cross-cutting in `security.md`.
- **Audit row immutability.** Per phase-01 §7.21: server rejects any push with `deleted_at != null` on audit_log. Pre-vacuum: dirty=0; post-vacuum: dirty=0 (never flipped). The pruned rows NEVER propagate as deletes to peers.
- **Resolver authorization.** Resolving a conflict requires superadmin role (UI + IPC + server).
- **Sensitive field redaction in audit deltas.** Per phase-01 §7.14: password / token / hash / email never appear in raw form.

### §6.8 Data Integrity

- **Migration 008 idempotent.** Per §7.19: `sync_state.last_audit_vacuum_at` column added; replay-safe.
- **Migration 008 against populated DB.** Pre-seeded `sync_state` row gets the new column without breaking the singleton CHECK.
- **Vacuum predicate type-level proof.** Per §7.1: `vacuum_unsynced_safe` signature prevents pruning dirty rows.
- **Vacuum NEVER flips dirty.** Per phase-01 §7.31 carve-out + §2.1 test.
- **Vacuum self-audit row uses zero-UUID sentinel.** Per §7.3.
- **`sync_version` monotonicity on conflict resolve.** The resolved entity's `version` increments by 1 per the resolve operation (Keep Local replays as an upsert; Keep Server is a no-op locally; Merge is an upsert with the merged payload).
- **`audit_log.action` enum on `vacuum` + `conflict_resolve` + `daily_close_run`.** Per phase-01 §7.36 final enum (14 values): all assertable via Rust `AuditAction::from_str`.

---

## §7 Performance SLOs (this phase's surfaces)

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `audit::query` 90-day local window with all 6 filters | < 500 ms p99 | yes | `perf_audit_query_local_90d` | §9 default + §5 perf bench. |
| Tauri (SQLite) | `audit::vacuum_now` against 90-day rowset (~25k) | < 10 s | no (from §7.16 soak criteria) | `perf_audit_vacuum_90d_rowset_under_10s` | -- |
| Tauri (SQLite) | `diagnostics::summary` | < 50 ms p99 | no (tighter than §9's default; reads local metrics_events) | `perf_diagnostics_summary_under_50ms` | -- |
| Sync engine | Outbox drain steady-state during 8h soak | >= 50 ops/sec sustained | yes | `perf_outbox_drain_8h_steady_state` | Per §7.16 + §9. |
| Sync engine | Outbox steady-state depth during 8h soak | <= 800 rows | yes | `perf_outbox_depth_8h_soak` | Per §7.16 + §9. |
| Sync engine | Lock p95 latency during sustained load | < 30 s | yes | `perf_lock_p95_during_soak` | Per PRD §1.3 + §7.16. |
| Sync engine | Memory growth over 8h soak | < 50 MB | no (from §7.16; leak budget) | `perf_memory_growth_8h_soak` | -- |
| Sync server (Postgres) | `/audit/query` 100-row page | < 200 ms p95 | yes | `perf_server_audit_query` | §9 default. |
| Sync server (Postgres) | `/sync/conflicts` list at 100 parked | < 200 ms p95 | yes | `perf_server_conflict_list_100` | §9 default. |
| Sync server (Postgres) | `/sync/conflicts/:opId/resolve` round-trip | < 500 ms p95 | -- | `perf_server_resolve_round_trip` | One Prisma tx with 2 writes. |
| Sync server (Postgres) | `/metrics` Prometheus scrape | < 100 ms p95 | -- | `perf_server_metrics_scrape` | -- |
| Sync server (Postgres) | `/healthz` with db + redis probes | < 50 ms p95 | -- | `perf_server_healthz_probes` | -- |
| Frontend | `<AuditPage>` cold paint with 50 rows | < 200 ms | -- | `perf_audit_page_cold_paint` | -- |
| Frontend | `<DeltaViewer>` render for typical 5-field delta | < 50 ms | -- | `perf_delta_viewer_5_field` | -- |
| Frontend | `<ConflictResolverPanel>` cold paint with 10 conflicts | < 250 ms | -- | `perf_conflict_resolver_cold_paint` | -- |
| Frontend | `<DiagnosticsModal>` open | < 100 ms | -- | `perf_diagnostics_modal_open` | -- |

---

## §8 Definition of Done

- [ ] All §1 unit tests green.
- [ ] All §2 integration tests green.
- [ ] All §3 contract tests green.
- [ ] All §4 E2E tests green; multi-device specs green; soak spec green (8h simulated).
- [ ] §5 persona script **P3 Mariam the Superadmin** passes; P4 Two-Device Conflict passes as reinforcement; soak harness Markdown report shows all §7.16 criteria met.
- [ ] §6 all eight edge categories addressed.
- [ ] §7 SLOs met; §7.16 quantitative soak criteria all green.
- [ ] Coverage gates met per §1.3.
- [ ] No open P0 or P1 defects in `defects.md`.
- [ ] Snapshot files committed:
  - `expected/audit/audit-query-response-canonical.json.sha256`
  - `expected/sync/conflict-list-response-canonical.json.sha256`
  - `expected/sync/conflict-resolve-applied-response.json.sha256`
  - `expected/sync/conflict-resolve-already-resolved-response.json.sha256`
  - `expected/metrics/prometheus-exposition-sample.txt.sha256`
- [ ] `pnpm lint:i18n` returns zero violations on the entire codebase. Per §7.9.
- [ ] `pnpm lint:rtl` returns zero violations. Per §7.18.
- [ ] `pnpm a11y` returns zero serious or critical axe-core violations on every page. Per §7.13.
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green.

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P3 Mariam the Superadmin** | -- | -- | -- | -- |
| P4 Two-Device Conflict (reinforcement) | -- | -- | -- | Phase-08's bread-and-butter persona. |
| P5 Year-End Audit (reinforcement) | -- | -- | -- | Optional, exercises audit query + drill-down. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P08-G01 — Server resolve idempotency short-circuit (CRITICAL)

- **Source:** phase-08.md §7.22 + §3 server resolve idempotency
- **Target test section:** §2.3
- **Category:** Missing Integration Test

The server-side resolve idempotency rule mandates that a duplicate `resolve_op_id` short-circuits via the ProcessedOp cache and returns the prior cached body byte-for-byte. Without this assertion, a mid-flight network drop that triggers client retry can silently double-apply a resolution or return a divergent response. Coverage of the ProcessedOp cache hit path is the load-bearing safety invariant for §7.22.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/conflicts/:opId/resolve` | `idempotent_resolve_short_circuits_via_processed_op_cache_returns_byte_identical_body` | Seed ProcessedOp with prior response body for `resolve_op_id=X`; second POST with same `resolve_op_id` returns 200 with body byte-identical to the cached entry; no new audit row written; no second `conflicts.resolveTx` invocation. Per §7.22 server step 1. |

### §9.2 P08-G02 — Soak harness writes `target/soak-report.md` with 6 criteria (CRITICAL)

- **Source:** phase-08.md §7.16 soak report
- **Target test section:** §6.6
- **Category:** Missing Edge Coverage

The 8-hour soak harness is the §7.16 quantitative gate; without a verified Markdown report, the criteria cannot be audited post-run. The report must capture all six numbers in a single artifact so reviewers and CI can diff against the SLO table. This gap is the difference between "the harness ran" and "the harness proved the criteria."

- **Soak report artifact assertion** (extends §6.6 "8h soak quantitative criteria"). After `src-tauri/tests/soak/eight_hour_offline.rs` completes, assert `target/soak-report.md` exists and contains rows for: (1) sync push throughput ops/sec, (2) outbox steady-state depth rows, (3) p95 lock latency ms, (4) memory growth MB, (5) audit-vacuum duration on 90-day rowset ms, (6) `sync_conflict auto_resolved=true` count. Each row records the measured value and the §7.16 threshold side-by-side; absent rows fail the harness.

### §9.3 P08-G03 — Resolve audit row captures before/after JSON (HIGH)

- **Source:** phase-08.md §3 server resolve audit
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§2.3 already asserts the `conflict_resolve` audit row is written in the same `prisma.$transaction`, but never asserts the row's `delta` payload encodes the pre-resolution and post-resolution entity snapshots. Without payload-shape assertion the audit row is structurally present but functionally useless for the forensic trail §6.7 demands. The before/after JSON capture is the durable record of which side won and what the merged payload looked like.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/conflicts/:opId/resolve` | `audit_row_delta_captures_before_json_pre_resolve_and_after_json_post_resolve_entity_state` | Resolve a parked conflict via Keep Server; assert the `conflict_resolve` audit row's `delta` payload has shape `{ before: <local_pre_resolve>, after: <server_canonical>, choice: 'server', resolve_op_id }`; identical assertion for Keep Local (local payload as `after`) and Merge (merged payload as `after`). |

### §9.4 P08-G04 — `/metrics` named labels enumerated (HIGH)

- **Source:** phase-08.md §7.17 /metrics named labels
- **Target test section:** §2.3
- **Category:** Missing Integration Test

Current `/metrics` coverage asserts Prometheus exposition format via a generic regex but never names the specific metrics. Without enumerating `sync_push_duration_seconds`, `sync_conflict_total`, `outbox_depth_gauge`, and `audit_query_duration_seconds`, a regression that drops a metric series passes CI. Each named metric in §7.17 is consumed by a downstream dashboard; missing one silently breaks observability.

| Route | Test | Asserts |
|-|-|-|
| `GET /metrics` | `exposition_contains_4_named_metrics_per_7_17` | Response body contains a `# TYPE sync_push_duration_seconds histogram` directive, a `# TYPE sync_conflict_total counter` directive, a `# TYPE outbox_depth_gauge gauge` directive, and a `# TYPE audit_query_duration_seconds histogram` directive; each metric has at least one sample line. |

### §9.5 P08-G05 — `HealthSchema` enumerates 5 required keys (HIGH)

- **Source:** phase-08.md §7.17 /healthz fields
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.17 enrichment widens `HealthSchema` to a `'ok' | 'fail'` union but does not enumerate the five required top-level keys. A schema that merely permits the union without requiring `status, db, redis, migrationsApplied, version` will accept responses missing one of those keys, leaving the consumer (`<DiagnosticsModal>`) to fail at runtime. The contract test must lock the key set, not just the status value.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /healthz` (response) | `HealthSchema` (per §7.17 enrichment) | Required keys enforced: `status: 'ok' | 'fail'`, `db: 'ok' | 'fail'`, `redis: 'ok' | 'fail'`, `migrationsApplied: boolean`, `version: string`. Ajv `additionalProperties: false`. Negative case: response missing `migrationsApplied` rejected by schema. |

### §9.6 P08-G06 — ConflictResolverPanel header counters (HIGH)

- **Source:** phase-08.md §7.17 ConflictResolverPanel header counters
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

The `<ConflictResolverPanel>` header surfaces three rolling-7d counters (conflicts opened, resolved, oldest unresolved age) that drive the superadmin's triage decision. No E2E asserts these counters render with the correct values from `diagnostics::summary` after a known sequence of parks and resolves. Without coverage the header silently becomes stale and the resolver UI loses its at-a-glance signal.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `conflict-resolver-header-counters-rolling-7d.e2e.ts` | Mariam (superadmin) | 1) Pre-seed 5 parked conflicts (3 within 7d, 2 older); 2 resolved within 7d; oldest unresolved 4d ago. 2) Navigate `/sync/conflicts`. 3) Read header counter values. | Header shows `Opened 7d: 3`, `Resolved 7d: 2`, `Oldest unresolved: 4d`; values match `diagnostics::summary` payload. |

### §9.7 P08-G07 — `.husky/pre-commit` runs `lint:i18n` + `lint:rtl` on staged files (HIGH)

- **Source:** phase-08.md §7.10 / §7.18 husky pre-commit
- **Target test section:** §5 / §6.2
- **Category:** Missing E2E Scenario

Husky's pre-commit wiring is the enforcement edge that prevents Arabic-string regressions from landing. §7.10 + §7.18 declare the hook MUST run `lint:i18n` and `lint:rtl` against staged files; without explicit verification the hook can be silently absent or misconfigured and the lints become advisory rather than gating. A repository fixture commit is the deterministic way to prove the hook fires.

- **`.husky/pre-commit` integration check** (extends §6.2 "`pnpm lint:i18n` runs and reports zero violations"). Add a CI step that (a) confirms `.husky/pre-commit` exists and is executable, (b) `grep`s the script for `lint:i18n` and `lint:rtl` invocations against `lint-staged` glob, (c) runs the hook against a synthetic staged commit containing one `.tsx` file with a raw Arabic literal and asserts the hook exits non-zero. Mirror entry in §5.1 manual script list for occasional sanity check.

### §9.8 P08-G08 — Icon-only buttons use `aria-label={t('a11y.icons.<name>')}` with 13 keys (MEDIUM)

- **Source:** phase-08.md §7.12 ARIA icon labels
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.12 enumerates 13 `a11y.icons.*` i18n keys covering every icon-only button in the phase-08 surface area. No test scans the rendered DOM for icon-only buttons and asserts each carries the expected `aria-label`. Screen-reader users depend on this contract; a missing key falls back to an empty accessible name and the button becomes unannounced.

| Hook | Test | Asserts |
|-|-|-|
| `<ConflictResolverPanel>` / `<AuditFilters>` / `<DeltaViewer>` / `<DiagnosticsModal>` / `<SyncPill>` / `<UserMenu>` | `every_icon_only_button_carries_aria_label_from_a11y_icons_namespace_with_13_keys` | Render each component in `dir=ltr` and `dir=rtl`; enumerate every `<button>` whose only child is an icon; assert `aria-label` equals `t('a11y.icons.<name>')` for the expected key from the §7.12 list of 13; missing key fails the test. |

### §9.9 P08-G09 — `/audit` + `/sync/conflicts` breadcrumbs verified (MEDIUM)

- **Source:** phase-08.md §7.20 breadcrumbs
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.20 declares static breadcrumbs for `/audit` and `/sync/conflicts` routes driven by `breadcrumbs.*` i18n keys. No E2E verifies the breadcrumb renders the right localized text in both directions. A missing or misnamed key produces an empty breadcrumb segment that breaks the chrome's consistency.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `breadcrumbs-audit-and-sync-conflicts-en-ar.e2e.ts` | Mariam | 1) Navigate `/audit`; 2) read header breadcrumb; 3) toggle locale to ar; 4) repeat; 5) navigate `/sync/conflicts`; 6) repeat. | Crumb under en reads `Audit` then `Sync · Conflicts`; under ar reads the matching `breadcrumbs.audit` and `breadcrumbs.sync_conflicts` translations; both routes carry crumbs in both locales. |

### §9.10 P08-G10 — Consolidated persona spec for verify-step-12 superadmin journey (MEDIUM)

- **Source:** phase-08.md §6.12 end-to-end story
- **Target test section:** §5 / §4.1
- **Category:** Missing Edge Coverage

phase-08.md §6.12 lists a 12-step superadmin journey (search audit, drill down, identify conflict, resolve, verify vacuum ran, inspect diagnostics) that the test plan addresses only via fragmented specs across §4.1. The DoD's canonical persona must walk the entire story end-to-end without context loss. A consolidated spec is the integration check that the fragmented pieces compose.

- **Consolidated persona spec** (extends §5.1 + §4.1). Author `e2e/specs/personas/p3-mariam-superadmin-day.e2e.ts` walking the §6.12 12-step journey in order: login -> `/audit` search -> apply 6 filters -> click row -> drill to entity-detail -> spot conflict in `<SyncPill>` -> navigate `/sync/conflicts` -> resolve via Merge -> open `<DiagnosticsModal>` -> verify counters updated -> trigger `audit_vacuum_now` via test IPC -> verify self-audit row appears in `/audit` results. Reference from §5.2 as canonical P3 script.

### §9.11 P08-G11 — Final TENANT_MODELS 15-entry list asserted at v0.1.0 (MEDIUM)

- **Source:** phase-08.md §7.26 final TENANT_MODELS list
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.26 freezes the TENANT_MODELS membership at the v0.1.0 cut: 15 entries spanning catalog, operators, visits, inventory, audit. Earlier phases (P03-G09, P04-G04) noted partial-membership gaps; phase-08 owns the consolidating assertion. Without a single test that asserts the full 15-entry list, the server-side tenant scoping can silently drift via additions to TENANT_MODELS that bypass review.

| Route | Test | Asserts |
|-|-|-|
| `TENANT_MODELS` (server constant) | `final_15_entry_list_at_v0_1_0_matches_phase_08_7_26` | Imports `TENANT_MODELS` from `sync-server/src/app/sync/tenant-models.ts`; asserts the array contains exactly: `users, settings, check_types, check_subtypes, doctors, doctor_check_pricing, operators, operator_specialties, operator_shifts, patients, visits, visit_items, inventory_items, inventory_adjustments, audit_log`. Order-insensitive; length-strict; excludes local-only (`metrics_events`, `outbox`, `sync_state`, `inventory_consumption_map`) and server-only (`refresh_tokens`, `processed_ops`, `conflicts_parked`, `sync_cursor`). |

### §9.12 P08-G12 — Rolling-7d conflict counter query SLO (MEDIUM)

- **Source:** phase-08.md §7.17 rolling-7d counter query
- **Target test section:** §7
- **Category:** Missing Performance SLO

`<ConflictResolverPanel>`'s header counters fire on every panel mount; the rolling-7d query against `metrics_events` (filtered by `kind='sync_conflict'`) must clear an SLO so the panel doesn't stall on busy tenants. No row in §7 typed-SLO table covers this query path; the implicit performance budget is invisible.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `diagnostics::summary` rolling-7d `sync_conflict` counter sub-query | < 30 ms p99 | no (tighter than §9 default; reads local `metrics_events` with `kind='sync_conflict'` and `at >= now - 7d`) | `perf_rolling_7d_conflict_counter_under_30ms` | Used in `<ConflictResolverPanel>` header on every mount; must not stall panel paint. |

### §9.13 P08-G13 — RTL visual diff in snapshot artifact list (LOW)

- **Source:** phase-08.md §6.9 RTL visual diff
- **Target test section:** §8 / §10
- **Category:** Missing Snapshot

§6.9 declares "screenshots of every page" as an RTL visual invariant but the snapshot artifact list in §8 DoD omits the RTL diff bundle. Without a snapshot path the visual diff is captured ad-hoc and lost between runs. Adding the path makes the artifact a first-class DoD item.

- **Snapshot file addition** (extends §8 DoD "Snapshot files committed" list). Append `expected/i18n-rtl/phase-08-pages-rtl-visual-diff/*.png.sha256` to the §8 snapshot listing; include matching entry in the §10 reference table covering `/audit`, `/sync/conflicts`, and `<DiagnosticsModal>` rendered in `dir=rtl` at 1440x900.

### §9.14 P08-G14 — `sync-pill.tsx` + `diagnostics-modal.tsx` in coverage glob (LOW)

- **Source:** phase-08.md §7.14 + §7.17 sync-pill / diagnostics-modal
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§1.3's frontend coverage globs cover `src/features/sync/conflicts/**` and `src/pages/audit/**` but never name `src/components/shell/sync-pill.tsx` or `src/components/diagnostics-modal.tsx`. Both are phase-08-owned components with unit and integration tests already declared; without a coverage row they can drop below threshold silently.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src/components/shell/sync-pill.tsx`, `src/components/diagnostics/diagnostics-modal.tsx` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/components/shell/sync-pill.tsx,src/components/diagnostics/diagnostics-modal.tsx"` |

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). The Pass 2 sweep re-read phase-08.md against the §9 additions and surfaced 16 new exposures (P08-G15 through P08-G30); merge these into the named target sections during the next test-authoring round.

### §10.1 P08-G15 -- Vacuum step 5 single-row atomicity (CRITICAL)

- **Source:** phase-08.md §7.21 step 5 composite vacuum job
- **Target test section:** §6.5 / §2.1
- **Category:** Missing Edge Coverage

§7.21 expands `AuditVacuumJob::run` into a six-step pipeline whose step 5 writes EXACTLY ONE `vacuum` audit row with delta `{ audit_purged, metrics_purged, cutoffs }` covering BOTH the `audit_log` soft-delete pass and the `metrics_events` hard-delete pass in the same transaction. Existing §2.1 vacuum tests assert the row exists and its delta shape but never pin the row count, so a refactor that emits two rows (one per purge) or a torn write that emits zero rows would silently pass current coverage. The audit-of-audit invariant -- one vacuum event = one row -- is the load-bearing contract; without it, forensic reconstruction of pruning history is impossible.

| Scenario | Asserts |
|-|-|
| `composite_vacuum_writes_exactly_one_audit_row_covering_both_purges_in_same_tx` | Seed 50 `audit_log` rows >90d old with `dirty=0`, plus 30 `metrics_events` rows >30d old. Trigger `audit::vacuum_now`. Count `audit_log WHERE action='vacuum' AND at >= job_start` returns EXACTLY 1 (not 0, not 2). The row's `delta` deserializes to `{ audit_purged: 50, metrics_purged: 30, cutoffs: { audit: <iso>, metrics: <iso> } }`. Inject a SIGKILL between step 4 and step 5 (use a test seam) and assert NO partial vacuum row exists post-restart (full transaction rollback). Per §7.21 step 5 + §6.5 crash atomicity. |

### §10.2 P08-G16 -- Client compute_resolve_op_id canonicalization (CRITICAL)

- **Source:** phase-08.md §7.22 client step 2 / §3.3 envelope contract
- **Target test section:** §3.2 / §3.3
- **Category:** Missing Contract Test

§7.22 mandates the Tauri client computes a stable `resolve_op_id = sha256(opId|choice|merged_canonical_json)` so two retries against the same logical resolution collide on the server's `ProcessedOp` cache. The MUST condition is canonical JSON: sorted keys, normalized whitespace, stable number formatting. Existing §3.2 IPC tests assert the hash is non-empty but never lock canonicalization, so a client that serializes `{b:1,a:2}` differently from `{a:2,b:1}` would emit divergent hashes for the same merge and defeat the idempotency cache. This is a CRITICAL contract gap because it directly enables the §7.22 double-resolve scenario Pass 1 P08-G01 was meant to prevent.

| Scenario | Asserts |
|-|-|
| `compute_resolve_op_id_uses_canonical_json_sorted_keys_normalized` | Construct two semantically identical merged payloads with different key orderings: `{name:"foo",amount:5}` vs `{amount:5,name:"foo"}`. Call `compute_resolve_op_id(opId="X", choice="merge", payload)` on each. Both return byte-identical SHA-256 hashes. Repeat with nested objects (`{a:{c:1,b:2}}` vs `{a:{b:2,c:1}}`) and arrays of objects -- arrays preserve order, objects canonicalize. Whitespace and trailing zeros normalized (`5.0` == `5`). Cross-check the Rust implementation against a TS reference canonicalizer to lock both sides. Per §7.22 client step 2 + §3.3 envelope canonicalization. |

### §10.3 P08-G17 -- Vacuum step 6 cursor update ordering (HIGH)

- **Source:** phase-08.md §7.21 step 6 cursor update ordering
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.21 step 6 specifies `sync_state.last_audit_vacuum_at` is updated as the FINAL step, AFTER step 5's self-audit row write. The ordering is load-bearing: if step 5 fails or the transaction rolls back, the cursor must remain untouched so the next scheduled run reattempts the pruning. Existing §2.1 tests assert the cursor lands on a successful run but never assert the cursor is UNTOUCHED on a step-5 failure; a regression that updated the cursor before the audit row would silently skip the next vacuum attempt and let stale rows accumulate.

| Scenario | Asserts |
|-|-|
| `cursor_update_is_final_step_untouched_on_audit_row_write_failure` | Pre-seed `sync_state.last_audit_vacuum_at = '2026-04-01T00:00:00Z'`. Inject a test fault that makes the step-5 INSERT INTO audit_log fail (e.g., constraint violation via a mocked repo). Run `audit::vacuum_now`. The job returns `Err(AppError::*)`; `sync_state.last_audit_vacuum_at` STILL reads `'2026-04-01T00:00:00Z'` (untouched); no purged rows remain deleted (full rollback). On the happy path repeat, cursor updates to the current timestamp ONLY after the audit row commits. Per §7.21 step 6 + §6.5 atomicity. |

### §10.4 P08-G18 -- ConflictResolverPanel live sync:conflict updates (HIGH)

- **Source:** phase-08.md §4 frontend `<ConflictResolverPanel>` + §3 `SyncEngine::handle_conflict_response`
- **Target test section:** §4.1 / §2.4
- **Category:** Missing E2E Scenario

§4 says the resolver UI consumes `sync:conflict` events emitted by `SyncEngine::handle_conflict_response`; §7.11 added the durable list endpoint but the in-session live-update path remains: when a new conflict is parked mid-session, the open `<ConflictResolverPanel>` must rerender to include the new row without requiring a manual refetch. Existing §4.1 specs cover the initial-load and post-resolve removal paths but never assert the mid-session insert. A regression that dropped the event subscription would leave the operator staring at a stale list during a live multi-device flow.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `conflict-resolver-panel-live-update-on-sync-conflict-event.e2e.ts` | Mariam (superadmin) | 1) Navigate `/sync/conflicts` with 1 pre-seeded parked conflict; assert list shows 1 row. 2) From device B (via `MULTI_DEVICE=true`), trigger a push that yields a new parked conflict. 3) Wait for the `sync:conflict` event to propagate to device A. 4) Without clicking refresh, assert the list now shows 2 rows; the new row appears at the top (parked_at DESC). 5) Assert header counters (Opened 7d) increment by 1 in the same tick. Per §4 + §7.17 header counters + §2.4 React Query invalidation on event. |

### §10.5 P08-G19 -- Cursor encoding symmetry server vs merged-paginator (HIGH)

- **Source:** phase-08.md §7.4 merged paginator cursor + §7.5 server cursor
- **Target test section:** §3.1 / §2.3
- **Category:** Missing Integration Test

§7.5 specifies server `/audit/query` next_cursor as base64url-encoded `{ at, id }`; §7.4 adds a merged-paginator cursor with an additional `source: 'local' | 'server'` field. The two cursor schemas must be symmetric and round-trippable: a server-only cursor decodes to a 2-key object; a merged cursor decodes to a 3-key object including `source`. Existing §3.1 contract tests cover response shape but never decode cursors. A regression that changed the encoding (raw base64, JSON-stringified array, etc.) would silently break pagination across the 90-day boundary.

| Route | Test | Asserts |
|-|-|-|
| `GET /audit/query` | `server_next_cursor_decodes_to_base64url_at_id_two_key_object` | Issue a paged query; take the returned `next_cursor`. Decode via `Buffer.from(c, 'base64url').toString('utf8')` then `JSON.parse`. Result has exactly keys `{ at, id }`; `at` is an ISO-8601 string; `id` is a UUID. Re-encode and assert byte-equality with the returned cursor (canonical round-trip). |
| `audit::query` (cross-boundary) | `merged_paginator_cursor_decodes_to_three_key_object_with_source` | Trigger a query where `from < (now - 90d)` AND `to > (now - 90d)` so §7.4's merger runs. Take the returned cursor; decode; assert keys `{ at, id, source }`; `source IN { 'local', 'server' }`. Cursor with `source='local'` resumes the local SELECT; cursor with `source='server'` resumes the remote leg. Per §7.4 + §7.5. |

### §10.6 P08-G20 -- EntityIdSubstringInput manual visual review (HIGH)

- **Source:** phase-08.md §7.24 `<EntityIdSubstringInput>`
- **Target test section:** §5.1
- **Category:** Manual Step

§7.24 added `<EntityIdSubstringInput>` inside `<AuditFilters>` with i18n placeholder `audit.filters.entity_id_prefix.placeholder`. Existing §5.1 manual script list covers receipt visual review and RTL page screenshots but never names this specific input. A misaligned placeholder, broken RTL caret position, or wrong text-direction inside the input would slip through automated tests because the input renders bilingually with a Latin-character expectation (UUID prefix). Manual eyes on `<AuditFilters>` in RTL specifically are required to confirm the input renders LTR-numerals despite the surrounding RTL layout.

- **Manual script row addition** (extends §5.1 owned scripts). Add a row to the §5.1 table: `<EntityIdSubstringInput> placeholder + RTL caret review` -- open `/audit` under `locale=ar` and `dir=rtl`; focus the entity-id-prefix input; confirm placeholder text reads the Arabic translation of "first 8 chars of entity_id"; confirm the input itself remains LTR-direction (UUID characters render left-to-right even inside RTL chrome); confirm the caret blinks at the LTR-leading edge; confirm clearing the input restores the placeholder. Reference from §5.2 as part of the canonical P3 Mariam persona's audit-filtering segment.

### §10.7 P08-G21 -- Receipt print success >99% threshold visual surfacing (HIGH)

- **Source:** phase-08.md §6 verify step 11 receipt_print_success >99% + §7.17 diagnostics::summary
- **Target test section:** §6.6 / §6.7
- **Category:** Missing Edge Coverage

§6 verify step 11 requires `> 99%` of lock events emit `receipt_print_success`; §7.17 surfaces the 30-day rate via `diagnostics::summary.receipt_print_success_rate_30d` in the `<DiagnosticsModal>`. The current §4.1 E2E asserts the modal renders the field but not the threshold-cross visual semantic: below 99% the value must render in `--crimson` (alert), at or above 99% in `--success` (target met). A regression that dropped the threshold-conditional class would let degraded print health hide behind a green-looking number.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `diagnostics-modal-receipt-print-rate-threshold-visual-crimson-vs-success.e2e.ts` | Mariam (superadmin) | 1) Seed `metrics_events` such that 100 lock events in the last 30d emitted 98 `receipt_print_success` + 2 `receipt_print_fail` (rate=98%). 2) Open `<UserMenu>` -> Diagnostics. 3) Inspect the receipt-print-rate row's computed color. 4) Reseed to 100/100 (rate=100%). 5) Reopen modal; inspect again. | At 98% the row's value renders with `color: var(--crimson)` (red) per design-system §1.4 alerting; at 100% it renders with `color: var(--success)` (green). Cross-check both renderings under `dir=ltr` and `dir=rtl`. Per §6 verify step 11 + §7.17 + design-system §1.4. |

### §10.8 P08-G22 -- UserMenu Diagnostics entry role visibility (MEDIUM)

- **Source:** phase-08.md §7.17 `<UserMenu>` Diagnostics entry + §7.23 role-link hide pattern
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.17 declares `<UserMenu>` adds a "Diagnostics" entry that opens the summary modal; §7.23 establishes the cross-cutting pattern that role-gated routes also hide their nav links from `<UserMenu>` for non-matching roles. The Diagnostics modal exposes telemetry that maps to superadmin-only audit territory; existing §2.4 tests cover the modal's behavior when invoked but never assert the menu entry is HIDDEN for receptionist and accountant roles. A regression that surfaced the entry to all roles would leak operational metrics to non-admin operators.

| Hook | Test | Asserts |
|-|-|-|
| `<UserMenu>` (component test, `describe.each([['ltr'],['rtl']])`) | `diagnostics_entry_visible_only_for_superadmin_hidden_for_receptionist_and_accountant` | Render `<UserMenu>` under each of the three roles via the auth context. As `receptionist`: `screen.queryByRole('menuitem', { name: /diagnostics/i })` returns `null`. As `accountant`: same null result. As `superadmin`: the menuitem exists and is keyboard-focusable. Same matrix under `dir=rtl`. Per §7.17 + §7.23 role-link hide pattern. |

### §10.9 P08-G23 -- AuditQuerySchema action enum 12 vs 14 reconciliation (MEDIUM)

- **Source:** phase-08.md §7.6 AuditQuerySchema 12 values vs phase-01 §7.36 final enum
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.6 enumerates 12 audit `action` literals in the server `AuditQuerySchema` TypeBox union; the §1.1 unit test in this plan asserts the Rust `AuditAction::from_str` enum has 14 values (incl. `daily_close_run` per phase-07). Pass 1 §9.x never reconciled the two -- the server schema says 12, the client enum says 14, and phase-01 §7.36 is the canonical source. A contract test must lock the server TypeBox union against the phase-01 §7.36 final enum so a future addition (e.g. phase-07's `daily_close_run` audit) cannot be accepted by Rust but rejected by the server schema.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /audit/query` (request) | `AuditQuerySchema.action` (per §7.6) | Reconciliation test: enumerate phase-01 §7.36's final `AuditAction` enum values (presently 14: `create, update, soft_delete, lock, void, clock_in, clock_out, password_change, login, logout, conflict_resolve, vacuum, daily_close_run, <14th from phase-01 final>`). For each value, submit `?action=<value>` to `/audit/query`; assert 200 (not 400 schema rejection). Conversely, submit `?action=bogus_action`; assert 400. The §7.6 schema must accept ALL phase-01 §7.36 values, not just the 12 originally enumerated. Test fails until §7.6 widens the union. |

### §10.10 P08-G24 -- DirectionalChevron icon mirror correctness (MEDIUM)

- **Source:** phase-08.md §7.18 `DirectionalChevron` from `@/lib/rtl/icons.ts`
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.18 introduces `<DirectionalChevron direction="forward" />` and `direction="backward"` as the canonical wrapper around `ChevronLeft` / `ChevronRight` so RTL flips happen at the component layer instead of via ad-hoc Tailwind classes. Existing §2.4 tests use `<DirectionalChevron>` but never assert WHICH lucide icon it renders in each direction-locale pair. The correctness matrix is non-trivial: `direction="forward"` under `dir=ltr` -> ChevronRight; under `dir=rtl` -> ChevronLeft. A regression that mirrored the mapping or dropped the rotation class would render arrows pointing the wrong way without breaking any other test.

| Hook | Test | Asserts |
|-|-|-|
| `<DirectionalChevron>` (`describe.each([['ltr'],['rtl']])`) | `directional_chevron_renders_correct_lucide_icon_per_direction_and_locale` | Render `<DirectionalChevron direction="forward" />` under `dir=ltr`; assert the rendered `<svg>` has lucide class `lucide-chevron-right` (NOT `chevron-left`). Same component under `dir=rtl`: rendered `<svg>` has class `lucide-chevron-left`. Repeat for `direction="backward"`: ltr -> chevron-left, rtl -> chevron-right. Assert no `rotate-180` class is present (the swap happens at icon-choice time, not via CSS rotation). Per §7.18. |

### §10.11 P08-G25 -- /metrics body size / label cardinality SLO (MEDIUM)

- **Source:** phase-08.md §7.17 `/metrics` Prometheus exposition
- **Target test section:** §7
- **Category:** Missing Performance SLO

§7.17 adds `GET /metrics` exposing four metric series (`sync_push_duration_seconds`, `sync_conflict_total`, `outbox_depth_gauge`, `audit_query_duration_seconds`), with `outbox_depth_gauge` per-tenant. No row in §7 caps the exposition body size or per-label cardinality. On a multi-tenant production server with hundreds of tenants, the per-tenant gauge can balloon the scrape body into the MB range and cause Prometheus scrape timeouts. A bounded SLO is required so the scrape stays under a defensible threshold and so a regression that introduced a high-cardinality label (e.g. per-user) is caught at build time.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Sync server | `GET /metrics` exposition body size at 100-tenant fixture | < 256 KB; total label-set count <= 5000 across all series | no (not in §9 default table; phase-08-owned operational SLO) | `metrics_exposition_size_and_cardinality_bound_at_100_tenant_fixture` | Prevents Prometheus scrape-timeout regressions; bounds operator blast-radius of new metric labels. Per §7.17. |

### §10.12 P08-G26 -- metrics_events FK / audit_log sync_version invariants on vacuum (MEDIUM)

- **Source:** phase-08.md §7.21 step 3-4 metrics hard-delete + soft-delete sync_version
- **Target test section:** §6.8
- **Category:** Missing Edge Coverage

§7.21 step 4 hard-deletes `metrics_events` rows older than 30 days; step 2 soft-deletes `audit_log` rows older than 90 days with `dirty=0`. Two integrity invariants live here: (a) `metrics_events` has no inbound FKs from any other table (local-only, non-syncable per phase-01 §7.28), so the hard delete must NOT trip a FK violation; (b) audit_log soft-deletes must continue to increment `sync_version` normally (the row is still locally present, just marked `deleted_at`). Existing §6.8 tests cover sync_version monotonicity on normal updates but never on soft-delete-via-vacuum. A regression that touched the FK graph or skipped `sync_version` on the soft-delete branch would corrupt the sync envelope.

| Scenario | Asserts |
|-|-|
| `metrics_events_hard_delete_no_fk_violation_and_audit_log_soft_delete_increments_sync_version` | Seed 30 `metrics_events` rows >30d old plus an `audit_log` row >90d old with `dirty=0` and `sync_version=5`. Trigger `audit::vacuum_now`. Verify (a) all 30 `metrics_events` rows are physically gone from the table (SELECT COUNT returns 0); no SQLite FK error in `tracing` output; pragma `foreign_keys = ON` was active during the run. (b) The `audit_log` row now reads `deleted_at IS NOT NULL` AND `sync_version = 6` (incremented through the soft-delete). Per §7.21 step 3-4 + §6.8 sync_version monotonicity. |

### §10.13 P08-G27 -- Receipt print rate kind-name derivation (MEDIUM)

- **Source:** phase-08.md §6 verify step 11 + §7.17 diagnostics::summary
- **Target test section:** §6.6
- **Category:** Missing Edge Coverage

§7.17 lists `receipt_print_success_rate_30d` as a field on the `diagnostics::summary` payload; §6 verify step 11 asserts the >99% threshold. The derivation rule is implicit: the rate must be `count(metrics_events.kind='receipt_print_success') / (count('receipt_print_success') + count('receipt_print_fail'))` over the last 30 days. Existing tests assert the field is present but never pin the kind-name spelling -- a regression that typoed `receipt_print_succeeded` or used a different denominator (e.g., total lock events instead of total print attempts) would yield a plausible-looking but wrong rate.

| Scenario | Asserts |
|-|-|
| `diagnostics_summary_receipt_print_rate_derived_from_exact_kind_names` | Seed `metrics_events` for the last 30 days: 95 rows with `kind='receipt_print_success'`, 5 rows with `kind='receipt_print_fail'`, plus 200 unrelated `kind='lock_event'` rows. Call `diagnostics::summary`. Assert `receipt_print_success_rate_30d == 0.95` (95/(95+5) -- denominator is the print attempts, NOT lock events). Repeat with 0 of either kind: assert the field returns `null` (or `1.0` per the agreed convention, asserted explicitly) so the UI threshold check doesn't divide by zero. Per §6 verify step 11 + §7.17. |

### §10.14 P08-G28 -- MergeEditor settings entity end-to-end (MEDIUM)

- **Source:** phase-08.md §3 Frontend `<MergeEditor>` for `visits` and `settings`
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§3 Frontend declares `<MergeEditor>` supports per-field merge for BOTH `visits` and `settings`. Existing §4.1 specs cover the visits merge (clinical-day persona) but the settings merge -- a superadmin scenario where two devices both edited `clinic.address` and `pricing.tax_rate` while offline -- is exercised only at the component level via §2.4. Without an end-to-end run, integration bugs at the `useConflictResolve` -> server `/sync/conflicts/:opId/resolve` boundary for the `settings` entity won't surface. settings is structurally different from visits (single-row, no line-items), so reusing the visits-only assertion is not sufficient.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `merge-editor-settings-entity-end-to-end-multi-device.e2e.ts` | Mariam (superadmin) under `MULTI_DEVICE=true` | 1) Device A and Device B both online; settings has `clinic.address='Old St'`, `pricing.tax_rate=15`. 2) Both go offline. 3) Device A edits to `clinic.address='New St', pricing.tax_rate=15`. Device B edits to `clinic.address='Old St', pricing.tax_rate=18`. 4) Both reconnect. 5) Device A pushes first; Device B's push parks a conflict. 6) On device B, navigate `/sync/conflicts`; click the settings conflict; choose Merge. 7) In `<MergeEditor>`, pick `clinic.address` from server (Device A's value) and `pricing.tax_rate` from local. 8) Submit. | The merged settings row reflects `{ clinic.address: 'New St', pricing.tax_rate: 18 }` on both devices after the next sync round-trip. Audit log records a single `conflict_resolve` row with `delta.choice='merge'` and the merged payload. Per §3 + §4 merge flow. |

### §10.15 P08-G29 -- AuditQueryResponseSchema golden snapshot (LOW)

- **Source:** phase-08.md §3 server `AuditQueryResponseSchema`
- **Target test section:** §10 / §3.3
- **Category:** Missing Snapshot

§3 server schemas declare `AuditQueryResponseSchema` for the `/audit/query` row list + nextCursor, and §10 of the testing rules requires golden snapshots for sync envelope samples. There is currently no committed snapshot for a typical mixed-actions, mixed-entities 50-row `/audit/query` response, so a renderer-side or serializer-side change that subtly altered the canonical row shape (for example, adding `entity_id_tenant` or reordering fields) would not trip the contract harness. One snapshot fixture locks the public-facing row shape for forensic and downstream-consumer stability.

| Snapshot file | Asserts |
|-|-|
| `expected/audit/audit-query-response-mixed-50-row.json.sha256` | Hash of the canonicalized JSON for a `/audit/query` response carrying 50 rows that mix all 12 (or 14, post P08-G23) action values, all 15 entity values, at least one row per actor in the clinical-day fixture, both `dirty=0` and `dirty=1` rows, and a non-null `next_cursor`. Committed alongside phase-01's existing sync-envelope snapshots. Per §10 envelope golden-file rule + §3.3 contract layer. DoD §8 grows one row: `[ ] expected/audit/audit-query-response-mixed-50-row.json.sha256 (NEW for this phase -- canonical audit-query response row shape)`. |

### §10.16 P08-G30 -- src/lib/rtl/icons.ts explicit coverage row (LOW)

- **Source:** phase-08.md §7.18 `@/lib/rtl/icons.ts` module
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§7.18 introduces `@/lib/rtl/icons.ts` as a new module exporting `<DirectionalChevron>` and related directional-icon helpers. Existing §1.3 frontend coverage globs treat `src/lib/**` as a single >=90% block, but `src/lib/rtl/icons.ts` is a tiny module (a handful of LOC) whose 100% coverage is trivially achievable and whose correctness is load-bearing for every RTL screen. Lumped into the broad `src/lib/**` glob, it could drop to 0% coverage while the aggregate stays green because the rest of `src/lib/**` is large. An explicit row pins the module's coverage independently.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src/lib/rtl/icons.ts` | >= 95% lines | `vitest --coverage --coverage.thresholds.lines=95 --coverage.include="src/lib/rtl/icons.ts"` |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 8 Phase-08 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P08-G31 through P08-G38). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P08-G31 -- handle_conflict_response outbox lifecycle by choice (HIGH)

- **Source:** phase-08.md §4 Tauri step 3 + §7.22 client -- after `sync::resolve_conflict` returns 200, the originating outbox row's fate depends on `choice`.
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1 covers only the `choice='local'` replay branch. `'server'` (DROP outbox row) and `'merge'` (REPLACE outbox row with merged-payload op) are not asserted.

| Scenario | Asserts |
|-|-|
| `resolve_conflict_server_choice_drops_outbox_row` | Seed a parked conflict with an outbox row (original push). POST `/sync/conflicts/<opId>/resolve { choice: 'server' }` succeeds. Assert: the original outbox row is REMOVED (count drops by 1, not replayed); no new outbox row is enqueued for the entity; local state mirrors the server's value (the server "won"). |
| `resolve_conflict_merge_choice_replaces_outbox_row_with_merged_payload` | Same setup. POST resolve with `{ choice: 'merge', merged: <payload> }`. Assert: the original outbox row is REMOVED; a new outbox row is enqueued with the merged payload's `op_id` and entity body equal to `merged`. The new row's `version` exceeds the local row's pre-resolve version. |
| `resolve_conflict_local_choice_replays_existing_outbox_row_with_resolver_op_id` | (Existing §2.1 coverage referenced here for completeness.) Assert the row is retained and the resolver replays it under a fresh op envelope. |

### §11.2 P08-G32 -- Merge paginator 1ms boundary non-overlap (HIGH)

- **Source:** phase-08.md §7.4 step 2 -- `[from, min(to, now-90d - 1ms)]` for the server slice.
- **Target test section:** §2.1
- **Category:** Missing Integration Test

The 1ms subtraction ensures local and server slices do not overlap. A regression using inclusive bounds on both legs would double-count rows at the boundary.

| Scenario | Asserts |
|-|-|
| `audit_query_merge_paginator_produces_zero_duplicate_rows_at_90d_boundary` | Seed 5 audit rows clustered around the 90-day-ago timestamp (3 within the last second of the local window, 2 within the first second of the server window). Run `audit::query` spanning the boundary; collect the merged result; assert ZERO duplicate `(at, id)` pairs across the union. Assert the local slice's max `at` < server slice's min `at` (strict inequality, proving the 1ms subtraction lands). Per §7.4 step 2. |

### §11.3 P08-G33 -- Criterion benchmark harness with 3 named benches (HIGH)

- **Source:** phase-08.md §5 Performance Verification -- benchmark harness `src-tauri/benches/` with `Lock end-to-end p95 < 30s`, `Sync replication after reconnect p95 < 5s`, `Audit query 90-day window p95 < 500ms local`.
- **Target test section:** §7 / §11
- **Category:** Missing Performance SLO + Missing Setup

§7 has soak-derived rows but no Criterion-bench harness row and no `Sync replication after reconnect` SLO. The benches directory is unreferenced.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (Criterion) | Lock end-to-end | p95 < 30s | no (PRD-ceiling, looser than §9 typical) | `bench_lock_end_to_end` in `src-tauri/benches/lock.rs` | Build spec PRD ceiling; existing tests measure tighter local-only paths, this bench covers the full lock + receipt + outbox enqueue flow. |
| Sync engine (Criterion) | Sync replication after reconnect | p95 < 5s | no (new SLO) | `bench_sync_replication_after_reconnect` in `src-tauri/benches/sync.rs` | Pre-PRD ceiling for offline -> reconnect catch-up; absent from §9 default table; added per phase-08 §5. |
| Tauri (Criterion) | Audit query 90-day window local | p95 < 500ms | no | `bench_audit_query_90d_window` in `src-tauri/benches/audit.rs` | Audit page p95 ceiling for local slice only; server-merge p95 is governed by §10.x and §9 default. |

Add corresponding §8 DoD checkbox: `[ ] src-tauri/benches/{lock,sync,audit}.rs exist and pass their declared p95 ceilings under `cargo bench`; the three named benches are referenced by their exact name in the `criterion` annotations.`

### §11.4 P08-G34 -- /audit/query page-size cap reconciliation (MEDIUM)

- **Source:** phase-08.md §3 Server Prisma "LIMIT 200 per page" vs §7.5 "Max page size: 100; default 50".
- **Target test section:** §3.1
- **Category:** Missing Contract Test

The two phase-08 sources disagree on the page cap. §10.9 reconciled the action enum; nothing reconciles the page-size constant.

| Route | Test | Asserts |
|-|-|-|
| `POST /audit/query` | `audit_query_enforces_max_100_default_50_per_7_5_resolution` | Send `{ limit: 150 }`. Expect 400 with error body `{ kind: 'Validation', message: '...max 100...' }`. Send `{ limit: 100 }` -> 200 with up to 100 rows. Send `{}` (no limit) -> 200 with up to 50 rows. The §3 Server Prisma "LIMIT 200" prose should be updated to "LIMIT 100" so source and runtime align; this test pins the resolved invariant at 100/50. Per §7.5. |

### §11.5 P08-G35 -- include_resolved=true stubbed in v1 (MEDIUM)

- **Source:** phase-08.md §7.11 -- `sync::list_conflicts(include_resolved=true)` "stubbed in v1 to return empty".
- **Target test section:** §2.2
- **Category:** Missing Integration Test

| Command | Test | Asserts |
|-|-|-|
| `sync::list_conflicts` | `list_conflicts_with_include_resolved_returns_empty_vec_in_v1` | Seed both an OPEN and a RESOLVED `ConflictParked` row. Call `sync::list_conflicts { include_resolved: true }`. Assert returned `Vec<ConflictParked>` is EMPTY (not just the resolved row -- empty entirely; the v1 stub short-circuits at the call boundary). Confirm NO server route `/sync/conflicts/history` is called (network spy). The history endpoint is reserved for Horizon-1; v1 must not silently hit it. Per §7.11. |

### §11.6 P08-G36 -- Boundary divider record i18n key (MEDIUM)

- **Source:** phase-08.md §7.4 step 3 -- boundary divider record uses an i18n key.
- **Target test section:** §1.2
- **Category:** Missing Unit Test

§2.1's `boundary_divider_inserted_when_source_switches` asserts shape only.

| Module | Test | Asserts |
|-|-|-|
| `audit/merge-paginator.ts` | `boundary_divider_carries_audit_boundary_crossed_local_retention_i18n_key` | Construct a paginator response that crosses the boundary; inspect the synthetic divider record. Assert its `kind === 'boundary_divider'` AND its `label_key === 'audit.boundary.crossed_local_retention'` (the exact key string). Verify both `en` and `ar` locale files contain this key with non-empty values. A renaming refactor breaks loudly. Per §7.4 step 3. |

### §11.7 P08-G37 -- Soak harness synthetic rate-mix (MEDIUM)

- **Source:** phase-08.md §5 Soak Harness step 2 -- "100 visits / 8h" rate matching PRD §1.3, with a specific mix of visits:locks:shifts:adjustments.
- **Target test section:** §6.6
- **Category:** Missing Edge Coverage

§10's coverage targets the report artifact (§9.2) and steady-state metrics (§7.16) but never asserts the input mix.

| Scenario | Asserts |
|-|-|
| `soak_synthetic_generator_produces_documented_pattern_mix` | Run the soak generator for 1 simulated hour (compressed timescale). Bucket emitted operations by kind: `visit_create`, `visit_lock`, `shift_clock_in`, `shift_clock_out`, `inventory_adjustment`. Assert the ratios match PRD §1.3 within +/-10%: roughly 100 visits/8h => ~12.5 visit creates per simulated hour; ~12 locks (assume 96% lock rate); ~2 clock-in / 2 clock-out shifts (P2 Mehdi's day pattern); ~5 inventory adjustments (consumption + restock blend). A regression that emitted only locks would pass §7.16 but never stress the conflict surface. |

### §11.8 P08-G38 -- DiagnosticsModal KPI tnum assertion (LOW)

- **Source:** phase-08.md §3 Frontend `<DiagnosticsModal>` + design-system §5.5 KPI tile rule + testing.md §14 anti-pattern.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DiagnosticsModal>` (`describe.each([['ltr'],['rtl']])`) | `kpi_numeric_values_render_with_font_feature_settings_tnum` | Render the modal seeded with diagnostics state covering all 5 KPI tiles (`lock_latency_p95_ms`, `outbox_depth`, `conflict_count_7d`, `receipt_print_success_rate_30d`, plus the 5th sync-pill value). For each tile's numeric value element (`data-testid="kpi-value-<key>"`): `getComputedStyle(el).fontFeatureSettings` MUST contain `'tnum'`. A regression dropping the tabular-numeral CSS would cause receipt and ledger figures to shift on render. Per design-system §5.5 + testing.md §14 anti-pattern "Numeric columns without tnum assertion". |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 4 Phase-08 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P08-G39 through P08-G42). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P08-G39 -- Soak harness 5-minute drain wallclock + terminal zero outbox (HIGH)

- **Source:** phase-08.md §5 Soak Harness steps 4-5 -- "all rows arrive on the server within 5 minutes" + "zero outbox rows remain".
- **Target test section:** §6.6 / §4.3
- **Category:** Missing Edge Coverage

| Scenario | Asserts |
|-|-|
| `soak_drain_completes_within_5_minutes_wallclock_and_outbox_reaches_zero` | Run the soak harness with an 8h offline period generating ~100 outbox ops (per PRD §1.3 + §11.7 mix). At T=8h, restore network. Start a wallclock timer. Poll `SELECT COUNT(*) FROM outbox` every 5s. Assert: (a) the count reaches 0 within 5 minutes wallclock (300s); (b) the terminal state is EXACTLY 0 (not 1, not 5 -- zero); (c) re-poll at +10s after first zero -- still zero (no resurrection). The 5-minute SLO and the exact-zero terminal condition are both load-bearing build-spec promises. Per §5 steps 4-5. |

### §12.2 P08-G40 -- generate_handler! registration for phase-08 commands (MEDIUM)

- **Source:** phase-08.md §3 Tauri -- "Register the new commands in `src-tauri/src/lib.rs::generate_handler!`" for `audit::query`, `audit::vacuum_now`, `diagnostics::summary`, and updated `sync::list_conflicts`.
- **Target test section:** §2.2
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `lib_rs_registers_phase08_commands_in_generate_handler_macro` | Static-analysis variant: grep `src-tauri/src/lib.rs` for each of `audit::query`, `audit::vacuum_now`, `diagnostics::summary`, `sync::list_conflicts` appearing inside a `generate_handler![...]` invocation. Runtime variant: invoke each via the Tauri test harness; assert each returns successfully (not `command not found`). A regression that defined a command function but forgot to register it compiles fine and fails at first IPC call. Mirror of phase-01 P01-G33. |

### §12.3 P08-G41 -- MergeEditor "edit manually" typed-input branch (MEDIUM)

- **Source:** phase-08.md §4 Frontend `<ConflictResolverPanel>` step 4 + `<MergeEditor>` -- "pick local or server, or edit manually" three-branch per-field affordance.
- **Target test section:** §4.1 / §2.4
- **Category:** Missing E2E Scenario

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `conflict-resolver-merge-editor-edit-manually.e2e.ts` | P3 Mariam (`superadmin`) | 1) Park a `visits` manual-policy conflict. 2) Open the resolver; click Merge to launch `<MergeEditor>`. 3) For one specific field (e.g. `void_reason`), neither pick local nor server -- type a custom value into the input field. 4) Submit Merge. | (a) The merged payload server-side reflects the typed custom value (NOT the local OR server value); (b) the `resolve_op_id` canonical hash matches the documented `sha256(opId\|choice\|merged_canonical_json)` invariant (§10.2); (c) a second resolve with the same custom value computes the SAME `resolve_op_id` (idempotency cache hits); (d) the audit row carries `delta.choice='merge'`. Per §4 step 4 third branch. |

### §12.4 P08-G42 -- v1 does NOT introduce BullMQ (LOW)

- **Source:** phase-08.md §5 -- "v1 does NOT introduce BullMQ".
- **Target test section:** §6.7 / §3.1
- **Category:** Missing Edge Coverage

| Scenario | Asserts |
|-|-|
| `phase08_does_not_introduce_bullmq_dependency_or_worker` | Read `sync-server/package.json`. Assert NO `bullmq`, `bull`, or `bullmq-pro` entry under `dependencies` or `devDependencies`. Read every Fastify plugin registration in `sync-server/src/app/**/*.ts`; assert NONE invokes `bullmq.Worker` or `bullmq.Queue` constructors. A regression that introduced BullMQ for a "background audit job" would silently drift the v1 server runtime profile (adds Redis as a HARD dependency where it is currently optional). Per §5 negative scope. |
