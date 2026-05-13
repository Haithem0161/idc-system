# Phase 04: Operator Shifts -- Test Plan

**Proves:** A receptionist or superadmin can clock an active operator in and out on the local device, a superadmin can retroactively edit or soft-delete a shift with full audit + overlap guards, the `/reception/shifts` page surfaces open shifts and today's history, and every shift mutation flows through the additive-only sync contract with the LWW-within-additive update tiebreak documented in phase-04 §7.6 / §7.9.

**Surfaces under test:** Frontend, Tauri/Rust, Sync Server (no new routes -- `/sync/push` + `/sync/pull` only).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, audit-first `with_audit`, outbox), Phase 02 test (auth + roles), Phase 03 test (operators catalog: `is_active`, `entity_id`).
**Test Data:**
- Factories: `src-tauri/tests/support/factories.rs::{make_operator, make_user, make_shift_open, make_shift_closed}` (new in this phase); `src/test-utils/factories.ts::makeShift` (new); `sync-server/test/support/factories.ts::makeOperatorShift` (new).
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- already contains 2 closed `operator_shifts` rows for Tuesday and is the seed for persona P2 (Mehdi). The phase-04 plan does NOT mutate the fixture schema; it consumes the existing rows.
**Tool prerequisites:**
- Rust: `cargo` (in use), `cargo-llvm-cov` (NEW: `cargo install cargo-llvm-cov` -- first phase to introduce coverage gating).
- Frontend: `vitest`, `@testing-library/react`, `@testing-library/jest-dom`, `jsdom`, `@vitest/coverage-v8` (NEW: `pnpm add -D ...` -- first phase to introduce frontend tests).
- E2E: `webdriverio`, `@wdio/cli`, `@wdio/local-runner`, `@wdio/spec-reporter`, `@wdio/mocha-framework`, `tauri-driver` binary (NEW: per `.claude/rules/testing.md` §13). Multi-instance via `MULTI_DEVICE=true` env per `personas.md`.
- Contract: `ajv@8`, `ajv-formats`, `@apidevtools/json-schema-ref-parser` (NEW: harness for §3.1).
- Sync server: `node --test` + `c8` already in `sync-server/package.json`.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services (`#[cfg(test)] mod tests`, pure logic, no SQLite)

| Module | Test | Asserts |
|-|-|-|
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `open_sets_check_in_now_and_clears_check_out` | `OperatorShift::open()` produces `check_out_at=None`, fresh UUID v7, `version=0`, `dirty=1`, `entity_id` echoed from input. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `open_rejects_empty_note_string` | `note = Some("")` is normalized to `None` (no whitespace-only notes -- prevents UI noise rows). |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `close_sets_check_out_at_and_bumps_version` | `close(by, at)` returns a shift with `check_out_at=Some(at)`, `check_out_by_user_id=Some(by)`, `version` incremented by 1, `dirty=true`. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `close_rejects_already_closed` | Calling `close()` on a shift whose `check_out_at` is already `Some(_)` returns `AppError::Validation`. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `close_rejects_check_out_before_check_in` | `at < self.check_in_at` returns `AppError::Validation` -- matches the SQL CHECK constraint. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `edit_times_rejects_inverted_window` | `OperatorShiftEditInput { check_in_at: T1, check_out_at: Some(T0) }` where `T0 < T1` -> `Validation`. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `edit_times_allows_reopen_when_check_out_at_none` | Setting `check_out_at: None` is allowed at the entity layer; the overlap check belongs to the service. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `edit_times_replaces_note_when_some_else_keeps` | `OperatorShiftEditInput::note = Some(Some("x"))` overwrites; `None` leaves note unchanged; `Some(None)` clears. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `soft_deleted_sets_deleted_at_and_bumps_version` | `soft_deleted()` returns a clone with `deleted_at=Some(now)`, `version+=1`, `dirty=true`. |
| `src-tauri/src/domains/shifts/domain/entities/operator_shift.rs` | `is_open_returns_true_only_when_check_out_at_is_none_and_not_deleted` | Matrix: open + live -> true; closed + live -> false; open + deleted -> false. |
| `src-tauri/src/domains/shifts/service/shift_service.rs::tests` | `require_role_accepts_listed_role` | `require_role(Superadmin, &[Superadmin])` returns `Ok(())`. |
| `src-tauri/src/domains/shifts/service/shift_service.rs::tests` | `require_role_rejects_other_role` | `require_role(Receptionist, &[Superadmin])` returns `Validation` carrying the role list. |
| `src-tauri/src/domains/shifts/service/shift_service.rs::tests` | `first_overlap_detects_inclusive_start_exclusive_end` | Pure overlap predicate: `[10:00..11:00)` overlaps `[10:59..12:00)`, does NOT overlap `[11:00..12:00)`. |
| `src-tauri/src/domains/shifts/service/shift_service.rs::tests` | `first_overlap_treats_open_shift_as_open_ended_until_now` | A `check_out_at = None` sibling is bounded by `now`; an edit landing inside that virtual window conflicts. |
| `src-tauri/src/domains/shifts/service/push_payloads.rs::tests` | `payload_round_trips_through_messagepack` | `OperatorShiftPushPayload::from(&shift)` -> `rmp_serde::encode` -> `decode` -> equal struct. |
| `src-tauri/src/domains/shifts/service/push_payloads.rs::tests` | `payload_includes_dirty_zero_after_push` | After a successful push the encoded payload's `dirty` is `false` (server sees clean state). |

### §1.2 TS pure functions / value objects (Vitest, no IPC, no React)

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/shift.ts` | `ShiftSchema_parses_minimal_open_shift` | Round-trip parse of an open-shift JSON sample (no `check_out_at`, `check_out_by_user_id=null`). |
| `src/lib/schemas/shift.ts` | `ShiftSchema_rejects_invalid_uuid` | Non-UUID `id` -> ZodError on path `["id"]`. |
| `src/lib/schemas/shift.ts` | `ClockInInputSchema_normalizes_empty_note_to_null` | `note: ""` survives parsing (the trim happens server-side); but `note` over 1024 chars rejects. |
| `src/lib/schemas/shift.ts` | `ShiftEditSchema_rejects_check_out_before_check_in` | `.refine()` emits the documented message; path `["check_out_at"]`. |
| `src/lib/schemas/shift.ts` | `SoftDeleteShiftSchema_requires_non_empty_reason` | `reason: ""` rejects; `reason` over 512 chars rejects. |
| `src/features/shifts/queries.ts` | `shiftKeys_overlaps_serializes_undefined_to_all` | `shiftKeys.overlaps(undefined)` -> `['shifts','overlaps','all']`; `shiftKeys.overlaps('op-1')` -> `['shifts','overlaps','op-1']`. (Pure key shape; no React Query runtime.) |
| `src/features/shifts/format.ts` (NEW helper, extracted from `<ShiftHistoryToday>`) | `formatShiftDuration_returns_hh_mm_for_closed` | Closed shift `(in, out)` -> `"2h 14m"`; open shift -> `"--"`; out-before-in -> throws (caught upstream). |
| `src/features/shifts/format.ts` | `formatShiftDuration_respects_locale_digit_shape` | When `arabicNumerals === true`, `"2h 14m"` becomes `"٢h ١٤m"` (digits only). |

### §1.3 Coverage target
- `src-tauri/src/domains/shifts/domain/**` >= 90% lines (per `.claude/rules/testing.md` §8).
- `src-tauri/src/domains/shifts/service/**` >= 90% lines (service is domain-adjacent, audit-first orchestration).
- `src/lib/schemas/shift.ts` + `src/features/shifts/format.ts` >= 90% lines (per §8 "frontend domain hooks/services").
- Measured with `cargo llvm-cov --lib --fail-under-lines 90 --include-build-script -- domains::shifts` (and `--include` filter for service) and `vitest --coverage --coverage.provider=v8 --coverage.thresholds.lines=90` scoped to the listed files via `coverage.include`.

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests (`src-tauri/tests/shifts_phase04.rs`, real in-memory SQLite + all migrations)

Existing scenarios in the file at HEAD (do not duplicate):
- `clock_in_succeeds_for_receptionist`
- `clock_in_rejects_double_open`
- `clock_in_rejects_inactive_operator`
- `clock_out_works`
- `edit_rejects_non_superadmin`
- `edit_succeeds_for_superadmin`
- `edit_rejects_when_overlapping_another_shift`
- `soft_delete_succeeds_then_rejects_second_call`
- `audit_row_lands_for_each_mutation`
- `overlap_detection_surfaces_concurrent_shift_rows`
- `history_today_returns_open_and_closed_shifts_within_window`
- `migration_creates_operator_shifts_table`
- `outbox_op_enqueued_per_mutation`

New scenarios to extend (one test = one assertion focus, share setup via `seed()`):

| Scenario | Asserts |
|-|-|
| `clock_in_rejects_when_operator_belongs_to_other_tenant` | Operator with `entity_id != caller.entity_id` -> `Validation` ("different tenant"); no row created; outbox unchanged. |
| `clock_in_rejects_when_operator_soft_deleted` | Operator with `deleted_at IS NOT NULL` -> `Validation`; covers §4 `ShiftService::clock_in` step 1 second branch. |
| `clock_out_rejects_already_closed_shift` | Calling `clock_out` twice on the same id -> second call returns `Validation`; outbox grows by exactly 1 (the first call). |
| `clock_out_rejects_soft_deleted_shift` | Soft-delete a shift, then attempt clock_out -> `Validation`. |
| `edit_rejects_when_target_shift_soft_deleted` | Per §7.8 step 0: editing a deleted shift -> `Validation` ("shift is deleted"). |
| `edit_rejects_reopen_when_other_open_exists` | Per §7.4: setting `check_out_at=None` on shift A while shift B is open for the same operator -> `Conflict`. |
| `edit_clears_note_when_replace_with_none` | `ShiftEditInput::note = Some(None)` clears; reload shows `note IS NULL`. |
| `edit_audit_delta_records_old_and_new_times` | Audit row's `delta` JSON contains `check_in_at`, `check_out_at`, and (if changed) `note` with `before` / `after` keys. |
| `soft_delete_records_reason_in_audit_delta` | Audit row for `soft_delete` has `delta.reason == "<reason>"`. |
| `soft_delete_rejects_non_superadmin` | Receptionist caller -> `Validation`; no row mutation. |
| `list_open_filters_by_entity_id` | Seed two tenants; assert `list_open("tenant-x")` returns only `tenant-x` rows. |
| `list_open_excludes_soft_deleted` | Soft-deleted open shift never appears in `list_open()`. |
| `history_today_excludes_yesterday_and_tomorrow` | Insert one shift in yesterday's window and one in tomorrow's window; `history_today(today_start, today_end)` returns exactly today's rows. |
| `history_today_uses_asia_baghdad_midnight` | Drive the boundary at Asia/Baghdad local midnight (UTC+03:00 fixed); assert a shift checked-in at 23:59 local time falls into the correct day window. (Time math is owned by the caller -- this test pins the contract.) |
| `list_overlaps_returns_empty_when_no_overlap` | Two non-overlapping shifts for the same operator -> empty result. |
| `list_overlaps_respects_operator_filter_vs_tenant_wide` | Operator A has overlap, operator B does not. `list_overlaps(entity, Some(A))` returns 1 pair; `list_overlaps(entity, Some(B))` returns 0; `list_overlaps(entity, None)` returns 1. |
| `with_audit_rolls_back_business_write_on_audit_failure` | Force the `audit_log` INSERT to fail (drop the `audit_log` table inside the tx). Expect: no `operator_shifts` row, no outbox row, no audit row. Proves audit-first ordering from phase-01 §7.7 still holds for shifts. |
| `with_audit_rolls_back_audit_on_business_failure` | Make the shift INSERT fail (FK violation on `operator_id`). Expect: no audit row, no outbox row. |
| `partial_unique_index_blocks_concurrent_open_shifts_at_db_layer` | Bypass the service: try two raw INSERTs with `check_out_at IS NULL` for the same operator -> the second hits `SQLITE_CONSTRAINT_UNIQUE`. Belt-and-suspenders next to the service-level `has_open_for_operator` check. |
| `history_today_index_used_by_query_plan` | `EXPLAIN QUERY PLAN` for `history_today()` SELECT mentions `operator_shifts_today`. Locks the index used in §1 migration (§7.2). |

### §2.2 Tauri IPC handler tests (`#[cfg(test)]` in `commands.rs` + `src-tauri/tests/shifts_commands_phase04.rs`)

One test per command in this phase. Happy + at least one error path.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `shifts_clock_in` | `clock_in_returns_serialized_shift_with_uuid_id` -> args `{ operator_id: <uuid>, note: null }`; assert returned `OperatorShift` round-trips through `serde_json` and has `check_out_at=null`. | `clock_in_returns_typed_app_error_when_operator_id_malformed` -> args `{ operator_id: "not-a-uuid" }` -> serialized `AppError::Validation` payload `{ kind: "Validation", message: ... }`. |
| `shifts_clock_out` | `clock_out_closes_open_shift` -> open then close; assert `check_out_at != null`. | `clock_out_returns_not_found_for_unknown_shift_id` -> random UUID -> `AppError::NotFound`. |
| `shifts_list_open` | `list_open_returns_hydrated_operator_name_and_phone` -> assert response field shape matches `ShiftWithMeta` (`operator_name`, `operator_phone`). | `list_open_returns_not_authenticated_when_no_session` -> `AppState` has no current user -> `AppError::NotAuthenticated`. |
| `shifts_history_today` | `history_today_returns_today_window_rows` -> open + close one shift, fetch history, assert 1 row, `shift.check_out_at` populated. | `history_today_returns_not_authenticated_when_no_session` -> `NotAuthenticated`. |
| `shifts_edit` | `edit_replaces_window_and_note_for_superadmin` -> happy path; assert returned `OperatorShift` reflects the new `check_in_at`, `check_out_at`, and the `NoteUpdate::Replace { value: Some("x") }` semantics. | `edit_rejects_non_superadmin_via_typed_error` -> receptionist caller -> serialized `AppError::Validation`. |
| `shifts_soft_delete` | `soft_delete_returns_unit_and_marks_row_deleted` -> assert IPC return is `()`, then a follow-up `list_open` excludes the row. | `soft_delete_returns_not_found_for_unknown_id` -> random UUID -> `AppError::NotFound`. |
| `shifts_list_overlaps` | `list_overlaps_returns_pairs_when_filter_unset` -> seed an overlap, omit `operator_id`, assert 1 pair returned. | `list_overlaps_rejects_malformed_operator_id` -> `operator_id: "x"` -> `AppError::Validation`. |
| `shifts_lines_run_today` | Owned by Phase 05 test (introduced in phase-04 §7.7). Cross-referenced here: phase-05-test.md §2.2 row `shifts_lines_run_today`. This phase does NOT add new tests for it. | (cross-ref) |

Notes:
- IPC tests construct `AppState` directly, register the same services the runtime uses, and exercise the `#[tauri::command]` async fn (callable as a plain async fn in tests). This is what existing `sync_phase01.rs` already does for its commands -- continue the convention.
- Each test asserts the serialized error shape, not the Rust enum -- the frontend only sees the JSON, so the JSON is the contract.

### §2.3 Sync server route handlers (`sync-server/test/sync/operator-shifts.test.ts`)

Phase 04 adds NO new routes -- all shift traffic flows through `/sync/push` and `/sync/pull`. The server-side tests therefore live under `sync/` (shared route module) but are scoped to the `OperatorShift` entity.

DB: real Prisma test DB (Postgres in docker-compose `sync-server-test`); per-test teardown via `prisma.$transaction([prisma.operatorShift.deleteMany(...), ...])`.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_accepts_new_operator_shift_insert` | Push payload conforming to `OperatorShiftPushSchema` -> `200 { success: true, data: { results: [{ op_id, status: "applied" }] } }`; row exists with `originDeviceId="dev-A"`, `lastSyncedAt` non-null. |
| `POST /sync/push` | `push_is_idempotent_on_op_id` | Replay the same `op_id` -> identical response from `ProcessedOp` cache; row count unchanged (no duplicate INSERT). |
| `POST /sync/push` | `push_applies_update_with_lww_within_additive` | Two updates to the same row with `updated_at` 1s apart -> later wins; earlier is no-op (`status: "skipped"`). Per §7.6. |
| `POST /sync/push` | `push_lww_tiebreak_by_origin_device_id_lex` | Two updates with identical `updated_at`, different `origin_device_id` -> lexicographically smaller `origin_device_id` wins. Per §7.6. |
| `POST /sync/push` | `push_applies_soft_delete_as_update_not_hard_delete` | Push with `deleted_at != null` -> row persists, `deleted_at` column populated, `tombstone=false` (additive policy, not tombstone). Per §7.9. |
| `POST /sync/push` | `push_rejects_payload_missing_op_id_with_400` | `op_id` absent -> 400 + `error.code: "MISSING_OP_ID"`. Sync-server.md rule. |
| `POST /sync/push` | `push_rejects_payload_with_mismatched_entity_id_403` | Token's `entityId` != payload `entity_id` -> 403 (tenant guard). |
| `POST /sync/push` | `push_rejects_check_out_before_check_in_422` | Domain invariant rejected at the server's `OperatorShift.create()` -> 422 with field path. |
| `GET /sync/pull` | `pull_returns_operator_shift_rows_since_cursor` | Seed 2 shifts; `GET /sync/pull?since=<initial>&limit=10` -> both rows in `changes`, `nextCursor` advances past the latest `updatedAt`. |
| `GET /sync/pull` | `pull_excludes_other_tenants_shifts` | Two tenants; the token's tenant gets only its rows. |
| `GET /sync/pull` | `pull_sets_pulledAt_on_returned_rows` | After a successful pull, the rows' `pulledAt` column is populated. Per §7.13. |
| `GET /sync/pull` | `pull_respects_limit_and_returns_hasMore` | Seed 30 rows; `limit=10` -> `changes.length === 10`, `hasMore === true`, `nextCursor` reflects the 10th row. |

### §2.4 React Query mutation / query flows (`src/features/shifts/__tests__/queries.test.tsx`, mocked IPC)

Mocked IPC via `vi.mock('@/lib/ipc', ...)` returning typed stubs. Assert cache invalidation, optimistic update, rollback on error.

| Hook | Test | Asserts |
|-|-|-|
| `useOpenShifts` | `useOpenShifts_returns_data_and_caches_under_shifts_open_key` | First mount -> loading -> data; second mount uses cache (no extra `invoke`). |
| `useOpenShifts` | `useOpenShifts_is_disabled_outside_tauri` | `isTauri()` mocked to `false` -> `enabled=false`, query never fires. |
| `useShiftHistoryToday` | `useShiftHistoryToday_uses_60s_stale_time` | After 30s, `dataUpdatedAt` unchanged (cache fresh). |
| `useShiftClockIn` | `clock_in_invalidates_all_shifts_keys` | After `mutateAsync`, `queryClient.invalidateQueries({ queryKey: ['shifts'] })` is observed (spy on QC). |
| `useShiftClockIn` | `clock_in_passes_note_null_when_omitted` | Calling with `{ operator_id }` -> IPC arg `{ operator_id, note: null }`. |
| `useShiftClockIn` | `clock_in_surfaces_typed_app_error_to_caller` | IPC mock rejects with `{ kind: 'Conflict', message: '...' }` -> `mutation.error` carries the typed shape. |
| `useShiftClockOut` | `clock_out_invalidates_all_shifts_keys` | Same invalidation pattern. |
| `useShiftEdit` | `edit_sends_note_undefined_when_caller_omits_note` | Caller omits `note` -> IPC arg has no `note` key (server keeps current). |
| `useShiftEdit` | `edit_sends_note_value_null_when_caller_clears_note` | Caller passes `{ note: { value: null } }` -> IPC arg preserves `NoteUpdate::Replace { value: null }` shape. |
| `useShiftSoftDelete` | `soft_delete_invalidates_all_shifts_keys` | Invalidation. |
| `useShiftOverlaps` | `overlaps_query_key_includes_operator_id_or_all` | `useShiftOverlaps(undefined)` and `useShiftOverlaps('op-1')` produce distinct cache entries. |

Components covered separately:
- `<OnShiftTable>` renders skeleton during loading, empty state when zero rows, hydrated operator name column when data present.
- `<ShiftHistoryToday>` renders the lines-run column with `0` placeholder per §7.7 (until phase-05 wires the real query).
- `<ClockInDialog>` operator combobox filters out operators with an open shift (the hook returns only eligible operators).
- `<RetroactiveShiftEditor>` rejects submit when `check_out_at < check_in_at` (Zod error surfaces as inline message).
- `<OpenShiftConflictBanner>` renders when `useShiftOverlaps()` returns non-empty; hides otherwise.
- `<ResolveOverlappingShifts>` submit dispatches `useShiftClockOut` then `useShiftSoftDelete` in sequence; rolls back UI state on either failure.

Each component test runs in `dir=ltr` AND `dir=rtl` per anti-pattern row in `.claude/rules/testing.md` §14 -- one `describe.each([['ltr'], ['rtl']])` wrapper, asserting layout invariants per `.claude/rules/design-system.md` §12.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Phase 04 adds NO server routes -- shift traffic flows through `/sync/push` and `/sync/pull` (both already declared in phase-01). The contract surface this phase adds is the **OperatorShift payload schema** embedded in those routes.

Harness: `sync-server/test/contract/operator-shifts-contract.test.ts`. On boot, fetch `GET /documentation/json`, dereference with `@apidevtools/json-schema-ref-parser`, compile the relevant subschemas with Ajv 8 + `ajv-formats`. For each canonical payload, validate the actual response against the schema.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (request) | `OperatorShiftPushSchema` (push envelope `op.payload.operator_shifts.*`) | `fixtures/payloads/operator-shift-push-insert.json`, `...push-update-clockout.json`, `...push-soft-delete.json`. Each MUST validate. |
| `POST /sync/push` (response) | `SyncPushResponseSchema` (per-op `results[]`) | Captured live response after the three pushes above. Validates `status in ['applied','skipped','conflict','rejected']`. |
| `GET /sync/pull` (response) | `SyncPullResponseSchema` (with `OperatorShiftResponseSchema` discriminated under `entity: 'operator_shifts'`) | Captured live response for the seeded tenant. Each row MUST validate including the new `pulled_at` field from §7.13. |
| (negative) | `OperatorShiftPushSchema` | `fixtures/payloads/operator-shift-push-missing-op-id.json` MUST fail Ajv with `additionalProperties` / `required` error mentioning `op_id`. |

### §3.2 IPC shape contract

Diff Rust `serde` JSON shape vs TS `Zod` declaration. Fail on drift. Harness: `src/test-utils/ipc-contract.test.ts` -- starts the Tauri binary in test mode, calls each command with a canonical input, captures the JSON, runs `ShiftSchema.safeParse(...)`. Companion Rust test (`src-tauri/tests/shifts_contract_phase04.rs`) calls each command in-process and serializes the result to `serde_json::Value`, then writes the JSON to a temp file the TS harness reads -- two-process diff.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `shifts_clock_in` | `OperatorShift` (from `domains::shifts::domain::entities`) | `ShiftSchema` (`src/lib/schemas/shift.ts`) |
| `shifts_clock_out` | `OperatorShift` | `ShiftSchema` |
| `shifts_list_open` | `Vec<ShiftWithMeta>` | (NEW) `ShiftWithMetaSchema = ShiftSchema.extend({ operator_name: z.string(), operator_phone: z.string().nullable() })` |
| `shifts_history_today` | `Vec<ShiftWithMeta>` | `ShiftWithMetaSchema` |
| `shifts_edit` | `OperatorShift` | `ShiftSchema` (and `ShiftEditSchema` for input) |
| `shifts_soft_delete` | `()` | `z.void()` (assert IPC returns `null` / `undefined`) |
| `shifts_list_overlaps` | `Vec<OverlapPairResponse { left, right }>` | (NEW) `OverlapPairSchema = z.object({ left: ShiftSchema, right: ShiftSchema })` |
| (Error envelope) | `AppError` (`{ kind, message }`) serialized via `Serialize` impl | `AppErrorSchema = z.object({ kind: z.enum([...]), message: z.string() })` -- one shared schema referenced by every command's error path. |

The harness MUST also assert the inverse: every field present in the Zod schema is present in the Rust JSON. A field added on either side without updating the other fails the contract test.

### §3.3 Sync envelope contract

- **Push payload conforms.** `OperatorShiftPushPayload` (Rust) serialized to JSON -> validate against `OperatorShiftPushSchema` (TypeBox on server). Test fixture: `fixtures/payloads/operator-shift-push-canonical.json`.
- **Pull payload conforms.** Server's `OperatorShiftResponseSchema` JSON output -> validate against a mirrored Zod schema on the client (the row applies through the same path that frontend would read).
- **Conflict-resolution policy declared and matches expectation.** Pull a pair of conflicting updates; assert the engine's policy registry lists `('operator_shifts', additive-only)` AND the LWW-within-additive update rule (`higher updated_at wins; origin_device_id lex tiebreak`) is exercised. Per phase-04 §4 sync-semantics table + §7.6 + §7.9.
- **Versioned envelope.** Assert the push body carries `envelope_version: 1` (registered in phase-01); a stub at `envelope_version: 999` is rejected with a clear error (engine forward-compat guard).
- **Snapshot files** (per §10): `expected/sync/operator-shift-push-canonical.json.sha256`, `expected/sync/operator-shift-pull-row.json.sha256`. Canonicalize via `serde_jsonc::to_string_pretty` (or equivalent stable canonical JSON helper); hash the bytes; commit the hash.

---

## §4 E2E Tests (Pyramid Layer 4)

WebdriverIO + `tauri-driver`. Specs live under `e2e/specs/shifts/`. Every selector is `data-testid` per `.claude/rules/testing.md` §14 anti-patterns -- never CSS classes, never DOM position.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `clock-in-and-out.e2e.ts` | Mehdi (`receptionist`) | 1) Boot, log in. 2) Navigate to `/reception/shifts`. 3) Assert empty state. 4) Click `[+ Clock in operator]` (`data-testid="shifts-page-header-clock-in"` from §7.15). 5) In dialog, pick operator "Kareem". 6) Submit. 7) Assert row appears in on-shift table with `data-testid="on-shift-row-<id>"`. 8) Click clock-out on the row. 9) Assert row moves to today's history table with `check_out_at` populated. | Two cache invalidations observed; final `outbox` count = 0 within 5s of network restoration (sync server reachable). One `clock_in` and one `clock_out` audit row in `audit_log` (assert via a debug IPC). |
| `superadmin-retro-edit.e2e.ts` | Mariam (`superadmin`) | 1) Log in. 2) Navigate to `/reception/shifts`. 3) Open the row's `<EditShiftRowAction>` (§7.15, `data-testid="edit-shift-row-<id>"`). 4) Shift `check_in_at` back 15 min. 5) Save. 6) Assert duration recomputes. | Audit row `action='update'` with delta covering both timestamps. The row's `version` increments by 1. |
| `superadmin-soft-delete.e2e.ts` | Mariam | 1) Log in. 2) Open shifts page. 3) Open the overlap banner (seeded via fixture override). 4) Click "Resolve". 5) In `<ResolveOverlappingShifts>`, pick "close A now, soft-delete B". 6) Submit. | Exactly one shift remains open (`list_open` returns 1). Audit log: 1 `clock_out` + 1 `soft_delete`. Banner disappears. |
| `non-superadmin-cannot-edit.e2e.ts` | Mehdi | Try to invoke `<EditShiftRowAction>`. | Button not rendered (gated `useCurrentUser().role === 'superadmin'`). |
| `reception-shifts-route-role-guard.e2e.ts` | Accountant Asma | Attempt to navigate to `/reception/shifts`. | Redirected by `<RequireRole roles={['receptionist','superadmin']}>` per phase-05 §7.58 (cross-ref §7.16). |

### §4.2 Failure-path flows

- **`offline-clock-in-drains-on-reconnect.e2e.ts`** -- Set `--offline` flag on tauri-driver; clock in; assert UI confirms; assert sync pill shows `offline`; lift the flag; assert pill goes `pushing` -> `idle`; assert server has the row via `mcp__curl__curl_get /sync/pull?since=0`.
- **`token-expiry-mid-clock-in.e2e.ts`** -- Force the JWT to expire by clock-skewing the test rig; click clock-in; assert one 401 -> automatic refresh -> retry succeeds; assert no duplicate `audit_log` row.
- **`server-5xx-during-push-retries-with-backoff.e2e.ts`** -- WireMock the sync server to return 503 three times then 200; clock in; assert the outbox row's `attempts` advances; assert it eventually drains; assert no row duplication.
- **`additive-clock-in-overlap-surfaces-banner.e2e.ts`** -- Multi-device or fixture-injected: two open shifts for the same operator (`origin_device_id` differs); assert `<OpenShiftConflictBanner>` renders inside `<ShiftsPage>`; assert `shifts_list_overlaps()` returns the pair. (Verifies §7.1.)
- **`edit-then-overlap-rejected.e2e.ts`** -- Two closed shifts, A and B, non-overlapping. As superadmin, attempt to edit B's `check_in_at` into A's window. Assert `<RetroactiveShiftEditor>` surfaces `Conflict` (`AppError::Conflict("edit would overlap shift ...")`). No DB mutation; assert via `audit_log` count unchanged.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

Two binaries, shared sync server seeded from `clinical-day.sql`.

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-clock-in-clock-out.e2e.ts` | Device A clocks Kareem in; Device A reconnects; assert Device B's pull surfaces the open shift in its on-shift table. Device B clocks Kareem out. Device B reconnects. Assert Device A's pull surfaces the closed shift in today's history. | Both devices converge to identical `check_out_at` on the row. No conflicts. Outbox empty on both within 30s. |
| `two-device-concurrent-clock-in-additive.e2e.ts` | Both devices offline. Both clock the same operator in (different `id` per device -- additive). Both reconnect. | Server has both rows. Both devices' pull surfaces both rows. Both devices show the overlap banner. `list_overlaps(operator)` returns the pair on both devices. (Verifies §4 sync semantics + §7.1.) |
| `two-device-concurrent-clock-out-lww.e2e.ts` | Device A and Device B both clock out the SAME shift offline (same row id). Device A's `updated_at` is 1s newer. | Server keeps Device A's `check_out_at`. Both devices converge to Device A's value. Per §7.6 LWW-within-additive. |
| `two-device-concurrent-clock-out-lww-tiebreak.e2e.ts` | Same as above but identical `updated_at` (clock-skewed to match). | Server keeps the row whose `origin_device_id` is lex-smaller. Per §7.6. |
| `two-device-soft-delete-propagation.e2e.ts` | Device A soft-deletes a shift. Device A reconnects. Device B reconnects. | Device B's `useOpenShifts` and `useShiftHistoryToday` both exclude the row. Per §7.9. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

These are manual checks for things automation cannot verify cheaply:
- **Visual RTL of `/reception/shifts`** -- Switch locale to `ar`; switch `arabic_numerals: true`; confirm: (a) eyebrow rule renders on the right, (b) `<OnShiftTable>` numeric column ("since") right-aligned with `tnum` Geist Mono digits in Arabic-Indic form, (c) `<EditShiftRowAction>` icon button mirrors to row-leading edge, (d) `<OpenShiftConflictBanner>` "Resolve" button text never wraps mid-word.
- **Dialog modality** -- `<ClockInDialog>` and `<RetroactiveShiftEditor>` trap focus, return focus to trigger on close, respect `Escape`, and announce as `role="dialog"` to screen readers.
- **Date / time picker behavior on retro-edit** -- In `<RetroactiveShiftEditor>`, the date-time picker accepts both en and ar digit shapes and never silently coerces a future time without surfacing the validation error.
- **Operator combobox** -- `<ClockInDialog>` combobox virtualizes when >50 operators; keyboard navigation works in RTL (Right arrow opens; Left arrow collapses; mirrored vs LTR).

### §5.2 Cross-references to `personas.md`

Phase 04 surfaces are exercised end-to-end by:
- `personas.md` -> **P2 Mehdi the Receptionist** -> steps 2, 4, 6 (clock-in at start of day, work offline during lunch, clock-out at end of day). Required for §8 DoD.
- `personas.md` -> **P4 Two-Device Conflict** -> steps 3-10 partially exercise the additive shift policy when cross-coupled with patient/visit edits. Optional reinforcement.
- `personas.md` -> **P5 Year-End Audit** -> step 5 reads aggregate operator hours from historical shifts (read-only). Optional.

P2 is the canonical persona for §8 DoD ("at least one persona script in `personas.md` exercises this phase's surfaces end-to-end and passes"). P2 MUST pass for phase-04-test status to flip to `complete`.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone
- **Asia/Baghdad fixed-offset midnight rollover.** `history_today_uses_asia_baghdad_midnight` (§2.1) drives `today_start` / `today_end` from `Asia/Baghdad` local midnight; a shift checked-in at `23:59:30 +03:00` falls into the correct day. Iraq does NOT observe DST -- the code MUST NOT call `chrono_tz::Tz::Baghdad.from_local_datetime(...).single()` on a wall-clock that assumes DST; assert via a unit test that `today_start.offset()` equals `+03:00` year-round.
- **Clock skew vs server.** Device clock is 10 min ahead of the server. Push a clock-in; the server's `updatedAt` is server-authoritative; pull-back replaces the local `updated_at` with the server stamp (per `.claude/rules/offline-first.md` "Common Pitfalls" item 2). Asserted in an integration test `clock_in_server_updated_at_wins_on_pullback`.
- **Day-boundary edge.** Clock in at `23:58 local`, clock out at `00:02 local` next day. `history_today` for the IN day shows the shift (it started today); `history_today` for the OUT day does NOT show it (the start is yesterday). Asserted in `history_today_uses_check_in_at_window`.
- **DST defensive.** Although Iraq has no DST, add a unit test that asserts `chrono::FixedOffset::east_opt(3 * 3600)` is the only TZ constructor used for shift windows -- a fuzz `cargo clippy` lint or a `grep` test in CI forbids `chrono_tz::Tz::Baghdad` in the shifts module.

### §6.2 i18n & RTL
- **en/ar swap on `/reception/shifts`.** Snapshot the page in both locales; assert all strings come from `reception.shifts.*` keys (new namespace per §7.5) -- no string literals in JSX. Test: a `grep`-style RTL component test verifies every visible string maps to a `t()` call.
- **Arabic-Indic numerals on every numeric column.** `<OnShiftTable>::since` and `<ShiftHistoryToday>::{in, out, duration, lines_run}` render `٠-٩` when `settings.arabic_numerals === true`. Asserted in `format-shift-duration.test.ts` + component tests (RTL slice).
- **RTL layout invariants.** Eyebrow rule on the right, `tnum` numerics right-aligned (which in RTL means left-aligned to the page edge), pill dots leading their label both directions, `<EditShiftRowAction>` icon mirrors via the row's flex direction not via `transform: scaleX(-1)`.
- **Mixed-direction text in the note field.** `note` field accepts Arabic + English mixed: assert no Unicode bidi mangling on save+reload (the stored bytes are exactly the input bytes).

### §6.3 Offline & Network
- **Full offline mode.** `offline-clock-in-drains-on-reconnect.e2e.ts` (§4.2). All 7 shift commands work offline; the UI never blocks on a network call for any read (§ offline-first invariant 1).
- **Intermittent connection.** Push 5 ops; drop the connection mid-3rd op; assert the engine retries from op 3, not op 1 (the outbox cursor advanced after op 2's success). Test: `intermittent-push-resumes-cleanly.e2e.ts`.
- **Token expiry mid-sync.** `token-expiry-mid-clock-in.e2e.ts` (§4.2). One 401 triggers refresh + retry once; second 401 emits `session_expired` and pauses pushes (queued ops preserved).
- **Server returns 5xx.** `server-5xx-during-push-retries-with-backoff.e2e.ts` (§4.2). Exponential backoff respected. The outbox `attempts` advances and `last_error` is populated for surfacing in the sync status UI.
- **Partial-batch push.** Push 50 ops where op 27 violates a server-side invariant. Assert ops 1-26 are `applied`, op 27 is `rejected` with a reason, ops 28-50 are still `applied` (the engine does not roll back the whole batch -- per push contract). Integration test `partial_batch_push_handles_per_op_results`.

### §6.4 Concurrency & Conflicts
- **2-device same row.** `two-device-concurrent-clock-out-lww.e2e.ts` (§4.3). Tests the LWW-within-additive update rule per §7.6.
- **3-device chain.** Devices A, B, C all clock the same shift out offline, then reconnect in random order. Assert deterministic convergence on the row with the highest `updated_at` (ties broken by `origin_device_id` lex). Integration test `three_device_lww_within_additive_converges`.
- **Conflict policy invocation.** Assert the policy registry returns `additive-only` for `operator_shifts`; assert no `manual` 409 response is ever emitted for a shifts push (this entity never parks).
- **Conflict resolver round-trip.** N/A for shifts -- this entity's policy is `additive-only` + LWW-within-additive updates, so it never parks in `ConflictParked`. Assert that the conflict resolver UI lists ZERO shift rows even when overlaps exist. Overlap surfacing is via the dedicated `<OpenShiftConflictBanner>` (§7.1), NOT the global conflicts page.

### §6.5 Crash & Recovery
- **SIGKILL during clock-in transaction.** Spawn the binary in a test harness, fire `shifts_clock_in`, kill the process after the audit INSERT but before the business INSERT (instrument via a feature-gated `panic!` after audit step). Reopen; assert: (a) no `operator_shifts` row, (b) no `audit_log` row, (c) no `outbox` row. Audit-first ordering plus tx rollback guarantees this. Test: `crash_mid_clock_in_leaves_no_partial_state` (Rust integration with a child process).
- **SQLite WAL after crash.** Kill the binary while WAL has uncommitted frames. Reopen with `journal_mode=WAL` + `busy_timeout=5000`; assert recovery is clean, no orphan WAL files, all queries succeed. Test: `wal_recovery_after_crash`.
- **Disk full.** Mount a tmpfs sized just below the migration footprint + 1 row; attempt clock-in; assert `AppError::Db` with a clear "disk full" message; no half-written row. Test: `disk_full_on_clock_in_returns_typed_error` (skipped in CI by default, runnable locally with `--ignored`).
- **Atomicity of multi-step transactions.** `with_audit_rolls_back_business_write_on_audit_failure` + `with_audit_rolls_back_audit_on_business_failure` in §2.1 cover the two failure modes.

### §6.6 Scale & Performance
- **10k shift rows.** Synthetic factory `make_shifts(10_000)` populated into the test fixture. `list_open()` returns within the SLO; `history_today()` over a 24h window returns within the SLO; `list_overlaps()` over the whole tenant returns within 200ms p99 (informational; not gated -- shifts are bounded by clinic size).
- **FTS5 search at 1k+ patients.** N/A for shifts in this phase -- shifts has no FTS surface (operator picker uses a simple LIKE query against `operators.name`). The `<ClockInDialog>` combobox is asserted at 200 active operators with a sub-50ms render. Cross-cutting FTS5 is owned by phase-05.
- **Outbox drain throughput.** Backlog of 500 shift ops -> drain at >= 50 ops/sec (default SLO from `.claude/rules/testing.md` §9). Asserted in `outbox_drain_throughput_on_shift_backlog`.

### §6.7 Security & Permissions
- **Role bypass attempts.** Receptionist tries `shifts_edit` -> `Validation`; receptionist tries `shifts_soft_delete` -> `Validation`. Asserted in §2.2 error-path tests + IPC contract `AppError` envelope.
- **JWT tampering.** Alter `role` claim from `receptionist` to `superadmin` and replay against the sync server's `/sync/push`. Assert 401 (signature invalid) -- the server NEVER trusts the claim shape; it verifies RS256. Cross-cutting test owned by `security.md` but referenced here.
- **FTS5 query injection.** N/A -- shifts owns no FTS5 surface in this phase. Operator picker LIKE query is parameterized via `sqlx::query!`. Assert via a smoke test: input `'; DROP TABLE operators; --` to the operator picker -> no table dropped, query returns zero rows.
- **Soft-delete bypass.** Soft-delete a shift via `shifts_soft_delete`; then call `shifts_list_open`, `shifts_history_today`, `shifts_list_overlaps` -- assert ALL of them exclude the row. Then bypass via a raw `sqlx::query!("SELECT * FROM operator_shifts WHERE id = ?", id)` -- assert the row IS still there in the table (soft delete is a tombstone, not a hard delete). Integration test `soft_delete_hides_from_reads_but_persists_in_table`.
- **Refresh-token replay.** N/A for shifts -- token refresh is owned by phase-02 + cross-cutting `security.md`. Cross-reference receipt only.

### §6.8 Data Integrity
- **Migration replay forward.** `cargo test migration_creates_operator_shifts_table` (already in §2.1) re-runs migrations 001..004 on a fresh DB and on a DB seeded with `clinical-day.sql`; both succeed without error. The `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` makes this idempotent.
- **Migration replay against populated DB.** Run migration 004 against a DB that already has phase-01..03 data + a pre-seeded `operator_shifts` snapshot (simulating an upgrade from a hypothetical out-of-band install). Asserted: rows preserved, indexes present, no constraint violations. Test: `migration_004_idempotent_on_populated_db`.
- **FK enforcement.** Insert a shift with `operator_id` pointing to a non-existent operator -> SQLite returns `FOREIGN KEY` constraint failure (foreign_keys = ON). Same for `check_in_by_user_id`, `check_out_by_user_id`. Test: `fk_enforcement_blocks_orphan_shifts`.
- **`ON DELETE RESTRICT` on user FKs.** Attempt a hard-delete of a user referenced by a shift -> SQLite returns `RESTRICT`. Test: `restrict_user_hard_delete_when_referenced_by_shift`. (Per §7.14.)
- **Soft-delete cascade rules.** Soft-deleting an operator does NOT soft-delete their shifts (shifts are an independent historical record). Soft-deleting a user does NOT soft-delete their shifts. Asserted in `soft_delete_operator_keeps_shift_rows`.
- **`sync_version` monotonicity.** Every mutation MUST increment `version` by exactly 1 (clock-in: 0; clock-out: 1; edit: 2; soft-delete: 3). Asserted in `version_increments_monotonically_per_mutation`. A regression that double-bumps or skips a version is a data-integrity bug.

---

## §7 Performance SLOs (this phase's surfaces)

Default SLOs in `.claude/rules/testing.md` §9 apply. Phase-04 overrides + per-operation pins:

| Surface | Operation | Threshold | Test name | Notes |
|-|-|-|-|-|
| Tauri (SQLite) | `shifts_list_open` over 100 open shifts | < 30 ms p99 | `perf_list_open_at_100_rows` | Hits `operator_shifts_open` partial index. Default list-query SLO. |
| Tauri (SQLite) | `shifts_history_today` over a single tenant day with 500 shifts | < 30 ms p99 | `perf_history_today_at_500_rows` | Hits `operator_shifts_today` index (§7.2). |
| Tauri (SQLite) | `shifts_clock_in` (full transaction: validate + audit + business + outbox) | < 50 ms p99 | `perf_clock_in_transaction` | Tighter than the default 200ms lock SLO -- shifts have no inventory/pricing fan-out. |
| Tauri (SQLite) | `shifts_clock_out` (full transaction) | < 50 ms p99 | `perf_clock_out_transaction` | Same as above. |
| Tauri (SQLite) | `shifts_list_overlaps` for a single operator over 30 days of shifts | < 100 ms p99 | `perf_list_overlaps_for_operator_30d` | The cross-product is bounded by ~30 rows; this is informational and gated. |
| Tauri (IPC) | Single shift IPC round-trip (Tauri serialize + Rust + SQLite + deserialize) | < 80 ms p99 | `perf_ipc_round_trip_clock_in` | End-to-end as seen by React Query. |
| Sync engine | Drain a 500-op shift backlog | >= 50 ops/sec | `perf_outbox_drain_500_shift_ops` | Default per §9. |
| Sync engine | Push a single shift op (round-trip) | < 1 s p95 | `perf_push_single_shift_op` | Default per §9. |
| Sync server (Postgres) | `/sync/push` handler latency for a 50-op shifts batch | < 200 ms p95 | `perf_push_handler_50_ops` | Default per §9. |
| Sync server (Postgres) | `/sync/pull` handler latency for a 100-row shifts page | < 200 ms p95 | `perf_pull_handler_100_rows` | Default per §9. |
| Frontend | `<ShiftsPage>` first paint after route navigation (warm cache) | < 100 ms | `perf_shifts_page_warm_paint` | Asserted via React Profiler in the E2E rig. |
| Frontend | `<ShiftsPage>` first paint cold (no cache) | < 300 ms | `perf_shifts_page_cold_paint` | One IPC + one render pass. |

Perf tests run in a dedicated `cargo test --test shifts_perf_phase04 --release` invocation and a `vitest run --mode benchmark` invocation. Variance failures are real bugs, not flakes -- fix the variance, do not relax the threshold.

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI (`cargo test -p app_lib --lib` + `vitest run --project unit`).
- [ ] All §2 integration tests green in CI (`cargo test --test shifts_phase04 --test shifts_commands_phase04` + `vitest run --project integration` + `pnpm --filter sync-server test -- sync/operator-shifts`).
- [ ] All §3 contract tests green in CI (`pnpm test:contract`).
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- shifts/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P2 Mehdi the Receptionist** runs end-to-end and passes (record date/runner in row below).
- [ ] §6 all eight edge categories addressed (no empty subsections).
- [ ] §7 SLOs met for every row in the perf table.
- [ ] Coverage gates met:
  - [ ] Rust domain (`domains::shifts::domain`) >= 90% (`cargo llvm-cov --lib --fail-under-lines 90 -- domains::shifts::domain`).
  - [ ] Rust service (`domains::shifts::service`) >= 90%.
  - [ ] Rust infrastructure (`domains::shifts::infrastructure`) >= 75%.
  - [ ] Frontend feature (`src/features/shifts/**`, `src/lib/schemas/shift.ts`) >= 90% (`vitest --coverage`).
  - [ ] Frontend presentation (`src/pages/reception/shifts.tsx`, `src/components/reception/*shift*.tsx`, `src/components/reception/*on-shift*.tsx`) >= 60%.
  - [ ] Sync server route handlers (`/sync/push`, `/sync/pull` for the `operator_shifts` slice) >= 85%.
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed: `expected/sync/operator-shift-push-canonical.json.sha256`, `expected/sync/operator-shift-pull-row.json.sha256`. (No A5 / thermal receipt for this phase -- shifts have no print surface.)
- [ ] `testing-status.md` row updated (Unit / Integration / Contract / E2E / Manual counts, Coverage %, Started / Completed dates, Open Defects).
- [ ] Lint, typecheck, build all green (`pnpm lint && pnpm build && cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd ../sync-server && pnpm lint && pnpm typecheck && pnpm test`).

**Persona run record:**
| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| P2 Mehdi the Receptionist | -- | -- | -- | -- |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P04-G01 -- ShiftService::edit rejects future check_in_at (HIGH)

- **Source:** phase-04.md §7.8 step 2 (`new_check_in_at > now -> ShiftError::CheckInInFuture`).
- **Target test section:** §2.1
- **Category:** Missing Integration Test

The build spec mandates that a superadmin retro-edit cannot move `check_in_at` past `now`. §2.1 currently covers overlap rejection and inverted-window rejection but never the future-time guard. Without this row the documented `ShiftError::CheckInInFuture` branch is unverified and a regression could silently allow "future shifts" to land in the audit log.

| Scenario | Asserts |
|-|-|
| `edit_rejects_when_new_check_in_at_in_future` | Superadmin calls `ShiftService::edit` with `new_check_in_at = now + 1 minute` on a closed shift -> `Err(ShiftError::CheckInInFuture)`; no row mutation (`version` unchanged, `dirty` unchanged); no audit row; outbox unchanged. Mirrors the inverted-window test shape and uses the same `seed()` helper. |

### §9.2 P04-G02 -- DB-layer CHECK constraint on check_out_at >= check_in_at (HIGH)

- **Source:** phase-04.md §1 `CHECK (check_out_at IS NULL OR check_out_at >= check_in_at)`.
- **Target test section:** §2.1
- **Category:** Missing Integration Test

The §1 migration encodes the time-window invariant in SQL so the DB is the final defence even if the service layer is bypassed. §1.1 covers the entity layer and §2.1 covers the service layer, but no test exercises the raw SQLite CHECK directly. A migration regression that drops or weakens the constraint would pass every existing test.

| Scenario | Asserts |
|-|-|
| `db_check_constraint_blocks_check_out_before_check_in` | Bypass the service: open a raw `sqlx::query!` `INSERT INTO operator_shifts (..., check_in_at, check_out_at, ...) VALUES (..., '2026-05-12T10:00:00Z', '2026-05-12T09:00:00Z', ...)` -> err with `SQLITE_CONSTRAINT_CHECK`; row count on `operator_shifts` unchanged. Pair with a positive control row where `check_out_at IS NULL` succeeds, locking the disjunction. |

### §9.3 P04-G03 -- ClockOutInputSchema unit coverage (HIGH)

- **Source:** phase-04.md §3 Frontend Zod schemas table -- `ClockOutInputSchema` lives in `src/lib/schemas/shift.ts`.
- **Target test section:** §1.2
- **Category:** Missing Unit Test

§1.2 enumerates parse tests for `ShiftSchema`, `ClockInInputSchema`, `ShiftEditSchema`, and `SoftDeleteShiftSchema` but omits `ClockOutInputSchema`. The schema gates every clock-out IPC arg shape from the frontend; without a unit test, a refactor that silently widens it (e.g. accepts an extra `force: boolean`) would not be caught at the unit layer.

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/shift.ts` | `ClockOutInputSchema_accepts_minimal_id_only_payload` | `{ id: '<uuid>' }` parses; `check_out_at` defaults to current ISO timestamp at schema layer (or remains undefined and is filled IPC-side, whichever the schema declares). |
| `src/lib/schemas/shift.ts` | `ClockOutInputSchema_rejects_invalid_uuid_id` | `{ id: 'not-a-uuid' }` -> ZodError on path `["id"]`. |
| `src/lib/schemas/shift.ts` | `ClockOutInputSchema_rejects_additional_properties` | `{ id: '<uuid>', force: true }` rejects via `.strict()` -- locks the closed shape against silent widening. |

### §9.4 P04-G04 -- operator_shifts present in server TENANT_MODELS (MEDIUM)

- **Source:** phase-04.md §5 TENANT_MODELS additions -- the array MUST include `'operator_shifts'`.
- **Target test section:** §3.3
- **Category:** Missing Contract Test

Without `'operator_shifts'` in `TENANT_MODELS`, push/pull bypasses the tenant guard and the entity will not route through the per-tenant scoping middleware. §3.3 already asserts envelope versioning and conflict policy but never the membership itself.

| Route | Test | Asserts |
|-|-|-|
| (server static) | `tenant_models_array_contains_operator_shifts` | Import `TENANT_MODELS` from `sync-server/src/sync/tenant-models.ts`; assert the array includes the literal `'operator_shifts'`. Pair with negative control: importing `'operator_shifts_open'` (the local-only index name) MUST NOT appear. Asserted at module-load time, no Prisma roundtrip needed. |

### §9.5 P04-G05 -- ShiftsPage ErrorState renders with Retry (MEDIUM)

- **Source:** phase-04.md §7.5 `<ShiftsPage>` states (`<Skeleton>`, `<Empty>`, `<ErrorState>` with "Retry").
- **Target test section:** §2.4
- **Category:** Missing Edge Coverage

§2.4 covers the skeleton + empty state of `<ShiftsPage>` but never the error state. The build spec calls out a "Retry" button on query failure; a regression that swallows the error and leaves a perpetual skeleton would not be caught.

| Hook / component | Test | Asserts |
|-|-|-|
| `<ShiftsPage>` | `ShiftsPage_renders_ErrorState_with_Retry_when_useOpenShifts_fails` | Mock `useOpenShifts` to reject with `AppError::Db`; render under both `dir=ltr` and `dir=rtl` (via `describe.each`); assert `data-testid="shifts-page-error-state"` is in the DOM, the localized "Retry" button is present with the `reception.shifts.retry` i18n key, and clicking it invokes `queryClient.invalidateQueries({ queryKey: ['shifts'] })`. Assert the skeleton and empty state are absent. |

### §9.6 P04-G06 -- list_open returns joined operator specialties (MEDIUM)

- **Source:** phase-04.md §3 IPC table -- `shifts::list_open` "With joined operator name + specialties"; §7.5 `<OnShiftTable>` Operator + specialties column.
- **Target test section:** §2.1 / §2.2
- **Category:** Missing Integration Test

§2.2 row `list_open_returns_hydrated_operator_name_and_phone` only covers name + phone; the build spec also promises joined specialties from `operator_specialties`. Without a test, a join regression would hide the specialty column from the on-shift table without surfacing a visible failure in CI.

| Layer | Scenario | Asserts |
|-|-|-|
| §2.1 Rust integration | `list_open_returns_operator_specialties_joined` | Seed an operator with two `operator_specialties` rows (e.g. "Echo", "Doppler") and an open shift; `shifts::list_open(entity)` returns one `ShiftWithMeta` whose `operator_specialties: Vec<String>` contains exactly those two values in deterministic order. An operator with zero specialties returns `[]`, not `null`. |
| §2.2 Tauri IPC | `list_open_ipc_serializes_operator_specialties_array` | The IPC return JSON includes `"operator_specialties": ["Echo","Doppler"]` (asserted via `serde_json::Value::pointer`); locks the wire shape for the frontend `ShiftWithMetaSchema` extension. |

### §9.7 P04-G07 -- ClockInDialog filters to operators without an open shift (MEDIUM)

- **Source:** phase-04.md §4 -- `<ClockInDialog>` operator combobox lists "Active operators NOT currently on an open shift".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§2.4 notes the filter intent ("`<ClockInDialog>` operator combobox filters out operators with an open shift") but does not enumerate a test row. The current coverage is mocked at the hook layer; the filter logic itself -- whether the eligible-operators query joins against `operator_shifts WHERE check_out_at IS NULL AND deleted_at IS NULL` -- is unverified against real SQLite.

| Layer | Scenario | Asserts |
|-|-|-|
| §2.1 Rust integration | `operators_list_eligible_for_clock_in_excludes_operators_with_open_shift` | Seed: operator A (active, no shifts), operator B (active, open shift), operator C (active, closed shift only), operator D (soft-deleted). Call the eligible-operators query (whatever IPC the dialog uses -- e.g. `operators::list_active_without_open_shift`). Assert returns exactly `[A, C]` in name order; B excluded by open-shift filter; D excluded by `deleted_at`. |
| §2.4 Component (mocked IPC) | `ClockInDialog_combobox_omits_operators_with_open_shift` | Mock the eligible-operators hook to return `[A, C]`; render `<ClockInDialog>` in both `dir=ltr` and `dir=rtl`; type into the combobox -- only A and C appear in the listbox. The dialog never displays B even when the user types B's name verbatim. |

### §9.8 P04-G08 -- EditShiftRowAction role gate at component layer (LOW)

- **Source:** phase-04.md §7.15 `<EditShiftRowAction>` gated `useCurrentUser().role === 'superadmin'`.
- **Target test section:** §2.4
- **Category:** Missing Unit Test

§4.1 covers the role gate end-to-end via `non-superadmin-cannot-edit.e2e.ts`, but there is no component-level assertion. An E2E failure is expensive to diagnose; a component test fails immediately on the regression and points at the exact prop / hook that broke.

| Component | Test | Asserts |
|-|-|-|
| `<EditShiftRowAction>` | `EditShiftRowAction_renders_nothing_when_role_is_not_superadmin` | Mock `useCurrentUser` to return `{ role: 'receptionist' }`; render the component inside a `<ShiftHistoryToday>` row; assert `queryByTestId('edit-shift-row-<id>')` returns `null`. Repeat for `'accountant'`. Then mock `{ role: 'superadmin' }` -> assert the button IS present. Run both `dir=ltr` and `dir=rtl` via `describe.each`. |

### §9.9 P04-G09 -- envelope_version: 999 rejection fixture / snapshot (LOW)

- **Source:** phase-04.md §3.3 "Versioned envelope... a stub at `envelope_version: 999` is rejected with a clear error."
- **Target test section:** §10 / §8 DoD
- **Category:** Missing Snapshot

§3.3 names the negative envelope test but no fixture / snapshot file is committed. §10 lists positive canonicals only. Without the negative artifact, a regression that silently accepts future envelope versions would not be detectable via hash diff.

| Artifact | Format | Comparison method |
|-|-|-|
| `fixtures/payloads/operator-shift-push-envelope-version-999.json` | JSON request body | Committed under `expected/sync/operator-shift-push-envelope-version-999.json.sha256`; canonicalized via the same helper used for the v1 push canonical; hash committed. |
| Server response to the v999 envelope | JSON error response | Committed under `expected/sync/operator-shift-push-envelope-version-999-response.json.sha256`; asserts the error body is stable (status, `error.code`, `error.message` keys) so any rejection-shape drift fails CI. |

Add a corresponding §8 DoD checkbox row: `[ ] Negative envelope_version: 999 fixture + response snapshot committed (P04-G09)`.

---

## §10 Gap Analysis Pass 2 Additions

These rows encode the 14 Phase-04 gaps surfaced by [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md) (P04-G10 through P04-G23). Each subsection follows the §9 format -- `Target test section` names the existing §X.Y subsection that should incorporate the new test row(s) during test authoring; Pass 3 verification expects every row below to read as covered.

### §10.1 P04-G10 -- Server acceptPush insert-vs-update branch (HIGH)

- **Source:** phase-04.md §7.6 -- "Add to server `ShiftService::accept_push` a step that distinguishes insert vs update by row presence."
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.6 mandates that the server `ShiftService::acceptPush` chooses the insert vs update branch by checking row presence BEFORE applying the LWW-within-additive update rule. §2.3 already covers the insert happy-path (`push_accepts_new_operator_shift_insert`) and the LWW update tiebreak (`push_applies_update_with_lww_within_additive`) in isolation, but no test pins the BRANCHING DECISION itself: a regression that always takes the update branch (silently no-op'ing a true insert because the LWW check fires before the row check) would still pass `push_applies_update_with_lww_within_additive` and could pass the insert test by luck of timing.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_distinguishes_insert_from_update_by_row_presence_before_lww` | Push a payload whose `id` does not exist server-side -> server selects the INSERT branch (assert via a debug header or log spy showing `branch: 'insert'`), row created with `version=0`. Push the SAME `id` again with a higher `version` and a newer `updated_at` -> server selects the UPDATE branch (`branch: 'update'`) and applies LWW: row's `version` becomes 1, `check_out_at` reflects the second payload. The branch selector MUST be a row-presence query, NOT a `version > 0` heuristic (assert by pushing an INSERT with `version=5` from a recovering device -> still goes INSERT, not UPDATE). Per §7.6. |

### §10.2 P04-G11 -- Audit-first ordering for edit and soft_delete (HIGH)

- **Source:** phase-04.md §7.11 -- "Each of `clock_in`, `clock_out`, `edit`, `soft_delete` invokes the two-pass `with_audit` from phase-01 §7.7."
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1's `with_audit_rolls_back_business_write_on_audit_failure` and `with_audit_rolls_back_audit_on_business_failure` cover audit-first ordering generically (the path exercised happens to be `clock_in`), but §7.11 calls out FOUR specific entrypoints that must each go through the two-pass closure: `clock_in`, `clock_out`, `edit`, `soft_delete`. A regression that wires `edit` or `soft_delete` through a single-pass write (audit + business in one INSERT) would pass every existing rollback test because those test the wrapper, not each callsite's invocation of it.

| Scenario | Asserts |
|-|-|
| `edit_rolls_back_business_write_when_audit_insert_fails` | Seed a closed shift; force `audit_log` INSERT failure inside the `edit` tx (drop the table mid-tx, matching the existing `with_audit_rolls_back_business_write_on_audit_failure` shape). Expect: shift row unchanged (`version` unchanged, `check_in_at` unchanged, `note` unchanged); no outbox row; no audit row. Proves `edit` invokes the two-pass closure rather than INSERTing both rows in one statement. Per §7.11. |
| `edit_rolls_back_audit_when_business_update_fails` | Force the UPDATE to fail (drop `operator_shifts` mid-tx or seed a check_in_at value that trips the `check_out_at >= check_in_at` CHECK). Expect: no audit row, no outbox row. |
| `soft_delete_rolls_back_business_write_when_audit_insert_fails` | Same shape for `soft_delete`: force `audit_log` failure; expect `operator_shifts.deleted_at` remains `NULL`, no outbox row, no audit row. Per §7.11. |
| `soft_delete_rolls_back_audit_when_business_update_fails` | Force the soft-delete UPDATE to fail (drop `operator_shifts` mid-tx); expect no audit row, no outbox row. |

### §10.3 P04-G12 -- pulledAt write-back after pull batch (HIGH)

- **Source:** phase-04.md §7.13 -- "`pulledAt` ... set by `SyncPullService` after each successful pull batch ship."
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§2.3 row `pull_sets_pulledAt_on_returned_rows` asserts the `pulled_at` field is present in the response, but the build spec ties `pulledAt` to a SERVER-SIDE WRITE that happens after the batch ships, not just a derived response field. A regression that computes `pulled_at` on-the-fly from a transient timestamp (without persisting it to the row) would pass the response assertion but break phase-08 diagnostics that read the stored `pulled_at` column. The cross-phase pattern is called out in the gap-analysis-pass-2 "Cross-Phase Patterns" §2 (Hook REGISTRATION vs hook BEHAVIOUR).

| Route | Test | Asserts |
|-|-|-|
| `GET /sync/pull` | `pull_persists_pulled_at_to_operator_shift_row_after_batch_ship` | Seed 2 operator_shifts with `pulled_at IS NULL`. Issue `GET /sync/pull?since=0&limit=10`; wait for the response. Open a raw Postgres client and `SELECT id, pulled_at FROM operator_shifts WHERE entity_id = $1` -- BOTH rows MUST have non-null `pulled_at`, set to a timestamp within 5s of the request. Issue a second pull -> the rows' `pulled_at` advances (newer than the first). The write-back MUST happen AFTER the response ships (not before), so a failure to ship leaves `pulled_at` untouched: simulate a client disconnect mid-response and assert `pulled_at` did NOT advance for that batch. Per §7.13. |

### §10.4 P04-G13 -- ResolveOverlappingShifts rollback on partial failure (HIGH)

- **Source:** phase-04.md §7.1 + §7.12 -- `<ResolveOverlappingShifts>` "submit dispatches `useShiftClockOut` then `useShiftSoftDelete` in sequence; rolls back UI state on either failure."
- **Target test section:** §2.4
- **Category:** Missing Edge Coverage

§2.4 lists `<ResolveOverlappingShifts>` as covered by the bullet "submit dispatches `useShiftClockOut` then `useShiftSoftDelete` in sequence; rolls back UI state on either failure" but no concrete test row exists for the PARTIAL-FAILURE rollback: clock_out succeeds, soft_delete fails. Without this row a regression that commits the clock_out optimistic cache update but never reconciles after the soft_delete reject would leave the UI showing an inconsistent state (banner gone, but the orphan row still open server-side after the next pull). The §4 verification step 11 narrative covers the success case only.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<ResolveOverlappingShifts>` (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `resolve_overlapping_shifts_rolls_back_clock_out_when_soft_delete_fails` | Mock `useShiftClockOut` to resolve successfully and `useShiftSoftDelete` to reject with `AppError::Conflict`. Render the modal; click "Close A now, soft-delete B"; submit. Assert: (a) the dialog stays open with `data-testid="resolve-overlapping-error"` rendered, (b) a destructive toast surfaces the conflict message, (c) `queryClient.invalidateQueries({ queryKey: ['shifts'] })` is called exactly once to force a refetch (NOT trusting the optimistic cache after partial failure), (d) the optimistic cache update for shift A's `check_out_at` is REVERTED (assert the cache reflects the pre-submit state). Per §7.1 + §7.12. |
| `<ResolveOverlappingShifts>` | `resolve_overlapping_shifts_rolls_back_when_clock_out_fails_before_soft_delete_runs` | Mock `useShiftClockOut` to reject; assert `useShiftSoftDelete` is NEVER called (ordering invariant), the dialog stays open with the error, and no cache mutation lingers. |

### §10.5 P04-G14 -- Sync envelope golden file completeness (HIGH)

- **Source:** phase-04.md §10 DoD snapshot listing vs §3.1 push payload catalogue.
- **Target test section:** §10 / §8 DoD
- **Category:** Missing Snapshot

§8 DoD lists `expected/sync/operator-shift-push-canonical.json.sha256` and `operator-shift-pull-row.json.sha256` but §3.1 enumerates THREE push fixtures (`...push-insert.json`, `...push-update-clockout.json`, `...push-soft-delete.json`) -- the update-clockout and soft-delete canonicals have neither a snapshot file nor a DoD checkbox. A regression that subtly changed the soft-delete envelope (e.g. emitted a `tombstone: true` flag despite §7.9's "soft-delete is a permitted update under additive-only, NOT a tombstone") would not trip CI. (P04-G23 LOW is the same omission read from a different angle and is closed by the same snapshot work.)

| Snapshot file | Asserts |
|-|-|
| `expected/sync/operator-shift-push-update-clockout-canonical.json.sha256` | Hash of the canonicalized JSON for a `shifts::clock_out`-derived push payload: `{ envelope_version: 1, op_id, entity: 'operator_shifts', op: 'update', payload: { id, check_out_at, check_out_by_user_id, version: 1, updated_at, origin_device_id, entity_id, ... } }`. Canonicalize via the same helper used for `operator-shift-push-canonical.json.sha256`. Per §3.1 fixture catalogue. |
| `expected/sync/operator-shift-push-soft-delete-canonical.json.sha256` | Hash of the canonicalized JSON for a `shifts::soft_delete`-derived push payload, MUST carry `op: 'update'` (NOT `op: 'delete'`) per §7.9 additive-only contract; `payload.deleted_at` populated; `payload.tombstone` field absent; `audit_log` companion row's `delta.reason` field present. |

Add corresponding §8 DoD checkbox rows: `[ ] expected/sync/operator-shift-push-update-clockout-canonical.json.sha256 (NEW for this phase, P04-G14)`, `[ ] expected/sync/operator-shift-push-soft-delete-canonical.json.sha256 (NEW for this phase, P04-G14)`.

### §10.6 P04-G15 -- soft_delete outbox under additive contract (MEDIUM)

- **Source:** phase-04.md §7.10 + §7.9 -- "Soft-delete is a permitted update under additive-only ... outbox row under additive contract."
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1 includes `outbox_op_enqueued_per_mutation` generically but no test pins the SHAPE of the soft-delete outbox row specifically -- and per §7.9 the shape is what carries the contract (an additive-only `op: 'update'` envelope with `deleted_at` populated, NOT a tombstone). A regression that emitted `op: 'delete'` with `tombstone: true` would still enqueue ONE outbox row (so `outbox_op_enqueued_per_mutation` would still pass) but would break server-side acceptance and the additive-only invariant.

| Scenario | Asserts |
|-|-|
| `soft_delete_outbox_row_carries_additive_update_envelope_not_tombstone` | Superadmin soft-deletes a shift. Inspect the enqueued outbox row: `op == 'update'` (NOT `'delete'`); `payload.deleted_at` is non-null; `payload.tombstone` field is ABSENT (no key, not just `false`); `envelope_version == 1`; `entity == 'operator_shifts'`; `payload.version` incremented by 1 vs pre-delete row. Decode via the same path the engine uses for ship (`rmp_serde::decode`); assert the round-trip shape matches `OperatorShiftPushPayload`. Per §7.9 + §7.10. |

### §10.7 P04-G16 -- NoteUpdate tagged-union contract diff (MEDIUM)

- **Source:** phase-04.md §3 + §1.1 entity test `edit_times_replaces_note_when_some_else_keeps` -- `OperatorShiftEditInput::note = Some(Some("x"))` overwrites; `None` keeps; `Some(None)` clears.
- **Target test section:** §3.2
- **Category:** Missing Contract Test

§1.1 unit-tests the three NoteUpdate states (keep / replace-with-value / clear) on the Rust side, and §2.4 row `edit_sends_note_value_null_when_caller_clears_note` covers the wire shape from the TS side. But §3.2 (the IPC shape contract layer) does not diff the Rust serde tagged-union output against the TS Zod `NoteUpdate` schema. A regression that renamed the Rust variant from `Replace { value }` to `Set { value }` would pass both unit tests and pass the TS hook test, but break the IPC contract silently.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `shifts_edit` (input shape) | `OperatorShiftEditInput::note: NoteUpdate` enum -- variants `Keep` (unit), `Replace { value: Option<String> }` -- serialized via serde's `#[serde(tag = "kind")]` or untagged convention (whichever §3 chose). | `NoteUpdateSchema = z.discriminatedUnion('kind', [ z.object({ kind: z.literal('Keep') }), z.object({ kind: z.literal('Replace'), value: z.string().nullable() }) ])` -- or the untagged equivalent matching whichever serde representation is wired. Contract harness MUST diff the Rust JSON shape against this schema for all three inputs: `{kind:'Keep'}`, `{kind:'Replace',value:'x'}`, `{kind:'Replace',value:null}`. A drift on either side (renamed `Replace` variant; reordered discriminant; missing `value: null` carriage) fails the contract. Per §3 Frontend Zod schemas + §1.1 entity test. |

### §10.8 P04-G17 -- ON DELETE RESTRICT differentiated per FK column (MEDIUM)

- **Source:** phase-04.md §7.14 -- "Update both FK declarations in §1 to `ON DELETE RESTRICT` ... check_in_by_user_id ... check_out_by_user_id."
- **Target test section:** §6.8
- **Category:** Missing Integration Test

§6.8 row `restrict_user_hard_delete_when_referenced_by_shift` tests the RESTRICT clause but does not differentiate between the two FK columns the §7.14 migration touches (`check_in_by_user_id` vs `check_out_by_user_id`). A regression that only applied `ON DELETE RESTRICT` to one column would pass the existing test because the test seeds a shift where both columns reference the same user. The build spec explicitly calls out BOTH columns.

| Scenario | Asserts |
|-|-|
| `restrict_user_hard_delete_when_referenced_as_check_in_by` | Seed user A (`check_in_by`) and user B (`check_out_by`) on a closed shift, where A and B are distinct. Attempt `DELETE FROM users WHERE id = A.id` via raw SQL -> SQLite returns `SQLITE_CONSTRAINT_FOREIGNKEY` (RESTRICT); row count on `users` unchanged. Per §7.14 first FK clause. |
| `restrict_user_hard_delete_when_referenced_as_check_out_by` | Same setup. Attempt `DELETE FROM users WHERE id = B.id` -> SQLite returns `SQLITE_CONSTRAINT_FOREIGNKEY` (RESTRICT). Per §7.14 second FK clause. The two tests together prove BOTH FKs carry the RESTRICT, closing the regression window where only one was migrated. |

### §10.9 P04-G18 -- operator_shifts_today index creation in migration 004 (MEDIUM)

- **Source:** phase-04.md §7.2 -- "Append to §1 migration: `CREATE INDEX operator_shifts_today ON operator_shifts(entity_id, check_in_at);`"
- **Target test section:** §2.1 / §6.8
- **Category:** Missing Integration Test

§2.1 row `history_today_index_used_by_query_plan` asserts the query plan MENTIONS `operator_shifts_today` -- but if the migration silently dropped the `CREATE INDEX` line, the planner would fall back to a table scan and the assertion would correctly fail; however, the test as written would be hard to diagnose (the failing assertion points at the planner output, not the missing migration line). A direct migration-replay assertion that the index EXISTS in `sqlite_master` after migration 004 catches the regression at its source.

| Scenario | Asserts |
|-|-|
| `migration_004_creates_operator_shifts_today_index` | Apply migrations 001..004 to a fresh in-memory SQLite. Query `SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='operator_shifts'`. Assert the result set contains `operator_shifts_today` AND `operator_shifts_open` (both indexes the phase declares). Assert the `operator_shifts_today` index's column list is `(entity_id, check_in_at)` in that exact order (via `PRAGMA index_info('operator_shifts_today')`). Per §7.2 + §1 migration declaration. |

### §10.10 P04-G19 -- ShiftsPageHeader clock-in button manual review (MEDIUM)

- **Source:** phase-04.md §7.15 -- `<ShiftsPageHeader>` top-right `[+ Clock in operator]` button; i18n key `reception.shifts.actions.clock_in_operator`.
- **Target test section:** §5.1
- **Category:** Manual Step

§5.1 manual scripts cover the dialog modality, the date picker, and the operator combobox, but no manual checklist row covers the `<ShiftsPageHeader>` top-right button position, RTL mirror, or its visual relationship to the eyebrow rule and `<OnShiftTable>` below it. A `[+ Clock in operator]` button that floated into the wrong corner under RTL would not trip any automated test (selectors are `data-testid`, not position-based) but is the most visually important affordance on the page.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| Manual visual review: `<ShiftsPageHeader>` clock-in button position (LTR + RTL) | P2 Mehdi (any role with access) | 1) Boot the app under locale `en` (LTR). 2) Navigate to `/reception/shifts`. 3) Inspect the page header: `[+ Clock in operator]` button sits on the LEFT-LEADING side (i.e. RIGHT edge in LTR) of the header row, aligned with the eyebrow rule baseline. 4) Switch locale to `ar` (RTL). 5) Re-inspect: the button mirrors to the RIGHT-LEADING side (i.e. LEFT edge in RTL). 6) Confirm the button text comes from i18n key `reception.shifts.actions.clock_in_operator` (no hardcoded string). 7) Confirm the `+` icon sits on the LEADING edge of the label in both directions (NOT a `transform: scaleX(-1)` mirror -- the icon's direction stays natural and flexbox handles the order). | Button position, icon orientation, and i18n wiring all pass the design-system §5.7 / §12 RTL convention. No layout regression vs the rendered design at `option-2-reception.html`. Per §7.15. |

### §10.11 P04-G20 -- ClockInDialog optimistic cache update and rollback (MEDIUM)

- **Source:** phase-04.md §4 Frontend -- "On success, optimistically updates `['shifts','open']` and `['shifts','today']` query caches."
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§2.4 row `clock_in_invalidates_all_shifts_keys` covers the post-success invalidation pattern, but §4's Frontend bullet 4 commits to an OPTIMISTIC UPDATE path: the dialog inserts a synthetic shift row into BOTH `['shifts','open']` and `['shifts','today']` caches BEFORE the IPC resolves, so the operator appears on-shift instantly. The optimistic path and its rollback-on-failure semantics are unverified.

| Hook / Component | Test | Asserts |
|-|-|-|
| `useShiftClockIn` | `clock_in_optimistically_inserts_into_open_and_today_caches_before_ipc_resolves` | Render a component using `useShiftClockIn`; spy on `queryClient.getQueryData(['shifts','open'])` and `getQueryData(['shifts','today'])`. Call `mutateAsync({ operator_id, note: null })` with a mocked IPC that resolves AFTER 100ms. Assert: between mutation start and IPC resolution, BOTH caches contain a synthetic row with `operator_id` matching the call, `check_out_at: null`, and a synthetic `id` that is replaced with the real `id` after IPC resolves. Per §4 Frontend bullet 4. |
| `useShiftClockIn` | `clock_in_rolls_back_optimistic_caches_when_ipc_rejects` | Mock IPC to reject with `AppError::Conflict`. Call `mutateAsync`; assert the synthetic row is REMOVED from both caches and the caches return to their pre-call state (deep-equal). The mutation's `error` carries the typed shape (existing `clock_in_surfaces_typed_app_error_to_caller` is reinforced here). |

### §10.12 P04-G21 -- ProcessedOp idempotency cache response shape (MEDIUM)

- **Source:** phase-04.md §7.9 -- "Replays of the same `op_id` return the cached `ProcessedOp` response regardless of operation kind."
- **Target test section:** §3.3
- **Category:** Missing Contract Test

§3.3 + §2.3 row `push_is_idempotent_on_op_id` asserts BEHAVIOUR (the second push doesn't create a duplicate row) but does not assert the SHAPE of the cached response returned on replay. A regression that returned an empty `{}` or a freshly-computed response (instead of the byte-identical cached envelope) would pass the row-count behaviour assertion but break the documented `ProcessedOp` contract. The cross-phase pattern is called out in gap-analysis-pass-2 cross-phase pattern §1 (Pass 1 row that enumerates a closed set must be paired with exhaustive assertion).

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` (replay) | `processed_op_replay_returns_byte_identical_cached_response_shape` | Push `{ op_id: 'A', payload: ... }`; capture the response body as `R1`. Replay the same `op_id` (identical payload bytes); capture `R2`. Assert `R2 === R1` byte-for-byte (NOT just structurally equal -- the cache stores the serialized response and returns it verbatim). The response shape MUST validate `{ success: true, data: { results: [{ op_id: 'A', status: 'applied', server_version: <int>, server_updated_at: <iso8601> }] } }`. Replay an `op_id` corresponding to a soft_delete -> response carries the same shape with `status: 'applied'` (NOT `'deleted'` -- additive-only means soft_delete is just an update). Per §7.9 + §3.3 envelope versioning. |

### §10.13 P04-G22 -- Sync-server route coverage gate invocation (LOW)

- **Source:** phase-04.md §8 DoD -- "Sync server route handlers (`/sync/push`, `/sync/pull` for the `operator_shifts` slice) >= 85%."
- **Target test section:** §8 DoD / §1.3
- **Category:** Missing Coverage Gate

§8 DoD lists a 85% sync-server route coverage target for the `operator_shifts` slice, but §1.3 enumerates coverage invocations only for Rust + frontend code paths -- the sync-server c8 invocation (with the path glob scoped to the `operator_shifts` slice) is not enumerated, so the CI gate has no concrete command to run.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `sync-server/src/sync/**` filtered to push/pull branches that handle `entity === 'operator_shifts'` | >= 85% lines | `pnpm --filter sync-server exec c8 --reporter=text --reporter=lcov --include='src/sync/push.ts' --include='src/sync/pull.ts' --include='src/sync/operator-shifts/**' --lines 85 --branches 70 node --test test/sync/operator-shifts.test.ts test/contract/operator-shifts-contract.test.ts`. Added as a new §1.3 row so the gate has a concrete command CI can run; matches the §8 DoD target. Per `.claude/rules/testing.md` §8 "Sync server routes >= 85% lines (c8)". |

### §10.14 P04-G23 -- Update-clockout and soft-delete fixture catalogue snapshots (LOW)

- **Source:** phase-04.md §3.1 fixture catalogue -- `operator-shift-push-update-clockout.json`, `operator-shift-push-soft-delete.json`.
- **Target test section:** §10
- **Category:** Missing Snapshot

§3.1 names three push fixtures (`operator-shift-push-insert.json`, `operator-shift-push-update-clockout.json`, `operator-shift-push-soft-delete.json`) used by the contract harness to validate the push envelope. The §10 snapshot listing only commits a hash for the canonical insert (`operator-shift-push-canonical.json.sha256`) and the pull row -- the two update/soft-delete fixtures have no committed hash, so a renderer change to those fixtures (e.g. reordering keys, dropping a field) would not be detectable by CI. This row is the snapshot-completeness twin of P04-G14: G14 adds the CANONICAL produced-by-the-engine snapshots; G23 adds hashes for the FIXTURE-INPUT files themselves so the contract harness's INPUT can't drift silently.

| Snapshot file | Asserts |
|-|-|
| `expected/sync/fixtures/operator-shift-push-update-clockout-fixture.json.sha256` | Hash of `fixtures/payloads/operator-shift-push-update-clockout.json` (the input fixture loaded by `sync-server/test/contract/operator-shifts-contract.test.ts`). Locks the fixture bytes so a hand-edit to the fixture (e.g. changing a UUID, adding a `tombstone` key) trips a hash mismatch in CI. |
| `expected/sync/fixtures/operator-shift-push-soft-delete-fixture.json.sha256` | Hash of `fixtures/payloads/operator-shift-push-soft-delete.json`. Same rationale -- locks the fixture's bytes so the soft-delete contract harness has stable INPUT under hash review. |

Add corresponding §8 DoD checkbox rows: `[ ] expected/sync/fixtures/operator-shift-push-update-clockout-fixture.json.sha256 (P04-G23)`, `[ ] expected/sync/fixtures/operator-shift-push-soft-delete-fixture.json.sha256 (P04-G23)`.

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 3 Phase-04 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P04-G24 through P04-G26). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these three rows close the symmetry gaps left by §10: exhaustive `with_audit` callsite coverage, exhaustive partial-index migration assertion, and symmetric optimistic-cache coverage for clock_out matching the clock_in path.

### §11.1 P04-G24 -- OnShiftTable clock-out optimistic cache symmetry (MEDIUM)

- **Source:** phase-04.md §4 Frontend -- `<OnShiftTable>` clock-out button "Updates the same two caches" (parallel to `<ClockInDialog>`).
- **Target test section:** §2.4
- **Category:** Missing Integration Test

P04-G20 (§10.11) closed the clock_in optimistic-cache path but the symmetric clock_out path remained.

| Hook / Component | Test | Asserts |
|-|-|-|
| `useShiftClockOut` | `clock_out_optimistically_mutates_open_and_today_caches_before_ipc_resolves` | Render a component using `useShiftClockOut`; pre-seed `['shifts','open']` with a row for shift S, and `['shifts','today']` with the same row (`check_out_at: null`). Spy on `queryClient.getQueryData`. Call `mutateAsync({ shift_id: S })` with a mocked IPC that resolves after 100ms. Assert: between mutation start and IPC resolution, `['shifts','open']` cache has S REMOVED, and `['shifts','today']` cache has S with `check_out_at` set to a synthetic timestamp. Per §4 Frontend `<OnShiftTable>` clock-out bullet. |
| `useShiftClockOut` | `clock_out_rolls_back_optimistic_caches_when_ipc_rejects` | Mock IPC to reject with `AppError::Conflict`. Call `mutateAsync`; assert S is RE-INSERTED into `['shifts','open']` and the `check_out_at` mutation in `['shifts','today']` is REVERTED -- caches return to their pre-call state (deep-equal). |

### §11.2 P04-G25 -- operator_shifts_open partial-index DDL assertion (MEDIUM)

- **Source:** phase-04.md §1 migration + §5 Local SQLite indexes -- `CREATE UNIQUE INDEX operator_shifts_open ON operator_shifts(entity_id, operator_id) WHERE check_out_at IS NULL AND deleted_at IS NULL`.
- **Target test section:** §2.1
- **Category:** Missing Integration Test

P04-G18 (§10.9) added a parallel assertion for `operator_shifts_today` but the open-shift partial index (the linchpin of `partial_unique_index_blocks_concurrent_open_shifts_at_db_layer`) has no direct sqlite_master assertion. A migration regression dropping the WHERE predicate would silently break the single-open-per-operator guarantee.

| Scenario | Asserts |
|-|-|
| `migration_004_creates_operator_shifts_open_partial_index_with_exact_where_clause` | Apply migrations 001..004 to a fresh in-memory SQLite. `SELECT sql FROM sqlite_master WHERE type='index' AND name='operator_shifts_open'`. Assert the result contains: (a) `UNIQUE` keyword (it is a unique index); (b) `(entity_id, operator_id)` column list in that order; (c) the substring `WHERE check_out_at IS NULL AND deleted_at IS NULL` (case-insensitive but token-exact -- a missing `AND deleted_at IS NULL` would let soft-deleted open shifts re-collide with new ones). `PRAGMA index_info('operator_shifts_open')` returns exactly two columns in (entity_id, operator_id) order. Per §1 + §5. |

### §11.3 P04-G26 -- clock_out audit-first rollback (MEDIUM)

- **Source:** phase-04.md §7.11 -- "Each of `clock_in`, `clock_out`, `edit`, `soft_delete` invokes the two-pass `with_audit` from phase-01 §7.7".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.11 explicitly names FOUR entrypoints. §2.1 `with_audit_rolls_back_business_write_on_audit_failure` exercises clock_in; P04-G11 (§10.2) covers `edit` and `soft_delete`. `clock_out` is the missing fourth callsite. A regression wiring `clock_out` through a single-pass write would pass every existing rollback test because those exercise other callsites.

| Scenario | Asserts |
|-|-|
| `clock_out_rolls_back_business_write_when_audit_insert_fails` | Seed an open shift; force `audit_log` INSERT failure inside the `clock_out` tx (drop the table mid-tx, matching the existing `with_audit_rolls_back_business_write_on_audit_failure` shape but driven through `shifts::clock_out`). Expect: shift row unchanged (`check_out_at` remains `NULL`, `version` unchanged, `check_out_by_user_id` unchanged); no outbox row; no audit row. Proves `clock_out` invokes the two-pass closure rather than INSERTing both rows in one statement. Per §7.11. |
| `clock_out_rolls_back_audit_when_business_update_fails` | Force the `UPDATE` to fail (drop `operator_shifts` mid-tx, or seed a `check_out_at` value that trips the `check_out_at >= check_in_at` CHECK). Expect: no audit row, no outbox row, shift row unchanged. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 2 Phase-04 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P04-G27 through P04-G28). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P04-G27 -- useShiftEdit cache invalidation (MEDIUM)

- **Source:** phase-04.md §3 Frontend hooks table -- `useShiftEdit` mutation hook (sibling to `useShiftClockIn`, `useShiftClockOut`, `useShiftSoftDelete` -- all of which carry invalidation rows in §2.4).
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `useShiftEdit` | `edit_invalidates_all_shifts_keys_on_success` | Render a component using `useShiftEdit`. Pre-seed `['shifts','open']` and `['shifts','today']` with rows containing the target shift's pre-edit values. Call `mutateAsync({ id, patch: { check_in_at: <new> } })` against a mocked IPC that resolves successfully. Assert: (a) `queryClient.invalidateQueries({ queryKey: ['shifts'] })` is called exactly once; (b) both caches refetch on next read; (c) the refetched data reflects the edited `check_in_at`. Closes the closed-set exhaustiveness gap (clock_in / clock_out / soft_delete all have invalidation rows; edit does not). |

### §12.2 P04-G28 -- ShiftsPage Empty CTA "Clock in" (LOW)

- **Source:** phase-04.md §7.5 -- `<ShiftsPage>` states list includes `<Empty action="Clock in">`.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<ShiftsPage>` (`describe.each([['ltr'],['rtl']])`) | `empty_state_renders_clock_in_cta_button_that_opens_dialog` | Render `<ShiftsPage>` with the `shifts::list_open` IPC mock returning `[]`. Locate `data-testid="shifts-empty-cta"`. Assert: (a) the button exists with text resolved from i18n key `reception.shifts.empty.clock_in` (recorded via instrumented `t()` lookup); (b) clicking it opens `<ClockInDialog>` (assert `screen.queryByTestId('clock-in-dialog')` becomes non-null). The CTA mirrors P04-G05's ErrorState Retry coverage. Per §7.5. |
