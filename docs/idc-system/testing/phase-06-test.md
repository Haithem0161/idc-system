# Phase 06: Inventory Operations -- Test Plan

**Proves:** A receptionist or superadmin can browse the inventory list (with status pills, active/inactive filter, search), drill into an item's four tabs (Overview, Consumption Map, Adjustments, Audit), and submit a `receive` / `writeoff` / `count_correction` adjustment via `<AdjustForm>` that lands as one `inventory_adjustments` row plus an in-tx recompute of `inventory_items.quantity_on_hand` -- with audit-first ordering, the per-reason delta-sign CHECK enforced on BOTH local and server schemas, role gating (`count_correction` superadmin only) enforced at the UI, IPC, AND server `acceptPush` layers, the additive-only sync policy preserved, and pull-time recompute overriding any incoming server-computed `quantity_on_hand` with the locally-summed value. Voided-visit reversal adjustments render as positive offset rows in the item's Adjustments tab.

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, audit-first `with_audit`, outbox, envelope versioning, `SyncPullService::on_pull_applied_inventory` hook surface from phase-01 §7.7 receipt), Phase 02 test (auth + roles, `<RequireRole>`, role guard on `/inventory/*`), Phase 03 test (`inventory_items` table from catalog -- `name`, `unit`, `low_stock_threshold`, `quantity_on_hand`, `entity_id`, `deleted_at`; `<DirtyDot>` component from frontend conventions), Phase 04 test (`<DirtyDot>` per-row pending-sync receipt convention from phase-04 §7.29 forward-ref, finalised in phase-05 §7.29), Phase 05 test (`inventory_adjustments` table + entity + sync policy `additive-only`; voided-visit `compute_void_offsets` writer; lock-workflow `consume_visit` adjustments).

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_inventory_item, make_inventory_item_with_threshold, make_adjustment_receive, make_adjustment_writeoff, make_adjustment_count_correction, make_adjustment_consume_visit, make_adjustment_offset_reversal}` (extended in this phase -- `make_adjustment_consume_visit` and the offset variant already exist from phase-05; the three operational constructors are NEW).
- Factories (TS): `src/test-utils/factories.ts::{makeInventoryItem, makeInventoryItemWithStatus, makeAdjustmentInput, makeAdjustmentRow}`.
- Factories (Sync server): `sync-server/test/support/factories.ts::{makeAdjustmentPushRow}` already exists from phase-05; extend with the three operational reason variants.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- already contains the full inventory items catalog + 30 days of `inventory_adjustments` covering all four reasons. The phase-06 plan consumes the fixture; it does NOT mutate the schema.
- Synthetic scale fixture: `fixtures/scale/inventory-1k-items.sql` (NEW for §6.6) -- 1k inventory items, 50k adjustments across 18 months. Loaded only for the scale drill; owned by `performance-soak.md` once that plan is wired.

**Tool prerequisites:**
- Rust: `cargo`, `cargo-llvm-cov` (installed in phase-04-test execution).
- Frontend: `vitest` + `@testing-library/react` + `jsdom` + `@vitest/coverage-v8` + `msw@2` (all installed in phase-04 / phase-05 execution).
- E2E: `webdriverio` + `tauri-driver` (installed in phase-04-test).
- Contract: `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser` (installed in phase-04-test).
- Sync server: `node --test` + `c8` already present.
- None new -- inherits the toolchain from phase-04-test and phase-05-test.

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`.
- JWT `role`-claim tamper replay against `/sync/push` -- owned by `security.md` (cross-referenced here for the count_correction defence-in-depth check).
- 3xN conflict matrix for `inventory_adjustments` cross-coupled with other entities -- the `additive-only` cell for `inventory_adjustments` is exercised here; the cross-product against `visits` (manual policy interactions on consume-vs-void races) is in `sync-conflicts.md`.
- Page-by-page i18n / RTL snapshots for `/inventory/*` -- this plan asserts core invariants per `.claude/rules/design-system.md` §12; the full visual page-by-page sweep is in `i18n-rtl.md`.
- 12-month scale drill aggregated -- owned by `performance-soak.md`; §6.6 here references the perf SLOs the soak harness aggregates.
- Conflict resolver round-trip (parked -> resolve -> audit row) -- N/A for this entity (`additive-only`), but the global resolver UI lives in phase-08 test.

**Cross-phase commands:** none. All five IPC commands in this phase (`inventory_list_items`, `inventory_get_item`, `inventory_list_adjustments`, `inventory_create_adjustment`, `inventory_recompute_on_hand`) are both registered AND conceptually owned by phase-06. Catalog-side `inventory_catalog_*` and `inventory_consumption_*` commands are phase-03's; phase-05's `consume_visit` writer is wrapped inside `visits::lock` and tested in phase-05-test.md §2.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`InventoryAdjustment` entity constructors (`src-tauri/src/domains/inventory/domain/entities/inventory_adjustment.rs`)** -- the four operational constructors are pure logic and the single most important reasoning surface in the phase.

| Module | Test | Asserts |
|-|-|-|
| `InventoryAdjustment::try_receive` | `receive_produces_positive_delta_and_correct_reason` | `try_receive(item, qty=5, by, note)` returns an `InventoryAdjustment` with `delta == 5`, `reason == AdjustmentReason::Receive`, `visit_id == None`, fresh UUID v7 id, `entity_id` echoed from input. |
| `InventoryAdjustment::try_receive` | `receive_rejects_qty_zero` | `qty=0` -> `Err(AdjustmentError::QtyNonPositive)`. Per PRD §6.1.14 inv 2. |
| `InventoryAdjustment::try_receive` | `receive_rejects_negative_qty` | `qty=-3` -> `Err(AdjustmentError::QtyNonPositive)`. |
| `InventoryAdjustment::try_receive` | `receive_trims_empty_note_to_none` | `note=Some("")` and `note=Some("   ")` both normalize to `None` (no whitespace-only audit noise). |
| `InventoryAdjustment::try_writeoff` | `writeoff_stores_negative_delta_from_positive_qty` | `try_writeoff(item, qty=5, by, note)` returns `delta == -5`, `reason == AdjustmentReason::Writeoff`. Per PRD §6.1.14 inv 3 + phase-06 §4 frontend step 3. |
| `InventoryAdjustment::try_writeoff` | `writeoff_rejects_qty_zero` | `qty=0` -> `Err(AdjustmentError::QtyNonPositive)`. |
| `InventoryAdjustment::try_writeoff` | `writeoff_rejects_negative_qty_input` | The constructor takes a POSITIVE qty and flips the sign; passing `-5` -> `Err` (the caller must not pre-flip). |
| `InventoryAdjustment::try_count_correction` | `count_correction_accepts_positive_signed_delta` | `try_count_correction(item, signed_delta=+7, by, note)` -> `delta == 7`. |
| `InventoryAdjustment::try_count_correction` | `count_correction_accepts_negative_signed_delta` | `try_count_correction(item, signed_delta=-7, by, note)` -> `delta == -7`. |
| `InventoryAdjustment::try_count_correction` | `count_correction_rejects_zero_delta` | `signed_delta=0` -> `Err(AdjustmentError::CountCorrectionMustBeNonZero)`. Per PRD §6.1.14 inv 4 + §7.7. |
| `InventoryAdjustment::try_consume_visit` | `consume_visit_requires_visit_id` | `try_consume_visit(item, qty=2, visit_id, by)` -> `delta == -2`, `visit_id == Some(...)`. Per phase-05 §1 CHECK + §6.1.14 inv 5. |
| `InventoryAdjustment::try_consume_visit` | `consume_visit_rejects_qty_zero` | Mirror of receive/writeoff. |
| `InventoryAdjustment` | `is_append_only_at_domain_layer` | No `edit` / `update_delta` / `set_reason` method exists on the entity; mutators are constructors only. Compile-time check via a doc-test or `proptest` that asserts the trait surface. (Inventory adjustments are immutable per phase-05 §7.33.) |
| `InventoryAdjustment` | `entity_id_required_and_propagated` | The `entity_id` field is non-`Option`; the constructors copy it through from the actor's context. |

**`AdjustmentService` (`src-tauri/src/domains/inventory/service/adjustment_service.rs`)** -- pure helpers (role gating, sanity caps); I/O goes to §2.

| Module | Test | Asserts |
|-|-|-|
| `AdjustmentService::require_role_for_reason` | `receive_allowed_for_receptionist_and_superadmin` | Both roles return `Ok(())`. |
| `AdjustmentService::require_role_for_reason` | `receive_rejected_for_accountant` | `Err(AdjustmentError::Forbidden)`. |
| `AdjustmentService::require_role_for_reason` | `writeoff_allowed_for_receptionist_and_superadmin` | -- |
| `AdjustmentService::require_role_for_reason` | `writeoff_rejected_for_accountant` | `Err(Forbidden)`. |
| `AdjustmentService::require_role_for_reason` | `count_correction_allowed_for_superadmin_only` | Superadmin OK; receptionist + accountant -> `Err(Forbidden)`. Per phase-06 §4 permission gates + §7.6. |
| `AdjustmentService::require_role_for_reason` | `consume_visit_never_emitted_via_ui_path` | `require_role_for_reason(ConsumeVisit, _)` -> `Err(AdjustmentError::NotUserSelectable)`. The lock workflow bypasses this gate via a separate constructor path. Per phase-06 §4 permission table row 4. |
| `AdjustmentService::sanity_cap_warning` | `warns_above_1000_abs_delta` | `|delta| > 1000` returns `Warning::UnusuallyLarge`; exactly 1000 does not. Per §7.8. |
| `AdjustmentService::sanity_cap_warning` | `does_not_block_above_1000` | Returns a `Warning`, NOT an `Err`. The service still constructs and persists the adjustment. Per §7.8: "warns but does not block". |

**`QuantityRecomputer` (`src-tauri/src/domains/inventory/service/quantity_recomputer.rs`)** -- pure SQL builder; I/O verified in §2.

| Module | Test | Asserts |
|-|-|-|
| `QuantityRecomputer::build_sql` | `produces_sum_with_tombstone_filter_and_in_clause` | The emitted SQL contains `SELECT COALESCE(SUM(delta), 0) FROM inventory_adjustments WHERE item_id = inventory_items.id AND deleted_at IS NULL` and `WHERE id IN (?, ?, ...)` with placeholders matching the input slice length. Per §7.2. |
| `QuantityRecomputer::build_sql` | `bumps_version_and_marks_dirty` | The emitted UPDATE sets `version = version + 1`, `dirty = 1`, `updated_at = :now`. Per §7.2. |
| `QuantityRecomputer::build_sql` | `accepts_empty_slice_as_noop` | Empty input -> emits a no-op statement (or returns `Ok(())` without executing); never builds an invalid `IN ()`. |

**`InventoryStatus` value object (`src-tauri/src/domains/inventory/domain/value_objects/stock_status.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `StockStatus::from_item` | `ok_when_quantity_above_threshold` | `quantity=10, threshold=3` -> `StockStatus::Ok`. |
| `StockStatus::from_item` | `low_when_quantity_at_or_below_threshold_but_non_negative` | `quantity=3, threshold=3` -> `Low`; `quantity=0, threshold=3` -> `Low`. Boundary inclusive per PRD §7.3.1. |
| `StockStatus::from_item` | `negative_when_quantity_below_zero` | `quantity=-1, threshold=3` -> `Negative` (precedence over `Low`). Per §7.10 + PRD §7.3.1. |
| `StockStatus::from_item` | `negative_takes_precedence_over_low_when_threshold_above_zero` | `quantity=-5, threshold=10` -> `Negative`, NOT `Low`. |
| `StockStatus::from_item` | `ok_when_threshold_is_null_and_quantity_non_negative` | `low_stock_threshold IS NULL` -> never `Low`; only `Negative` or `Ok`. Per PRD §6.1.12 inv 1. |

### §1.2 TS pure functions / value objects

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `receive_branch_requires_positive_qty` | `{ reason: 'receive', qty: 5 }` parses; `{ reason: 'receive', qty: 0 }` -> Zod error on path `["qty"]`; `{ reason: 'receive', qty: -1 }` -> error. Per phase-06 §3 Zod schemas + §4 frontend. |
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `writeoff_branch_requires_positive_qty` | Same matrix: positive accepted; zero/negative rejected. The TS layer sends positive qty; the Rust constructor flips to negative delta. |
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `count_correction_branch_accepts_signed_non_zero_delta` | `{ reason: 'count_correction', signed_delta: -3 }` parses; `signed_delta: 0` -> Zod error. Per §7.7. |
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `count_correction_branch_rejects_non_superadmin_at_form_layer` | The schema is `.refine`d with the actor's role from a closure; receptionist + `count_correction` -> error. Mirror of `AdjustmentService::require_role_for_reason` on the Rust side. |
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `note_optional_and_trimmed` | `note: ""` and `note: "  "` normalize to `null`. |
| `src/lib/schemas/inventory.ts::AdjustmentInputSchema` | `note_max_length_1024` | 1025-char note -> Zod error. |
| `src/lib/schemas/inventory.ts::ListItemsFilterSchema` | `status_filter_accepts_ok_low_neg_or_undefined` | Enum values + `undefined` (all). Anything else -> error. |
| `src/lib/schemas/inventory.ts::ListItemsFilterSchema` | `include_inactive_defaults_to_false` | `.parse({})` -> `{ include_inactive: false }`. Per §7.5 default. |
| `src/lib/schemas/inventory.ts::ListItemsFilterSchema` | `query_trimmed_and_min_2_chars_when_present` | Empty/1-char query treated as `undefined`; 2+ chars passes through trimmed. Per §7.5 search debounce contract. |
| `src/features/inventory/format.ts` | `format_quantity_uses_arabic_digits_when_setting_true` | `arabic_numerals=true` + `47` -> `"٤٧"`. `arabic_numerals=false` -> `"47"`. |
| `src/features/inventory/format.ts` | `format_quantity_renders_negative_with_minus_prefix_in_both_locales` | `-3` -> `"-3"` (en) or `"-٣"` (ar). Bidi-safe (LRM marks only where required to prevent the minus from flipping). |
| `src/features/inventory/format.ts` | `format_status_pill_label_maps_each_status_to_i18n_key` | `Ok` -> `inventory.status.ok`; `Low` -> `inventory.status.low`; `Negative` -> `inventory.status.neg`. Per `.claude/rules/design-system.md` §5.2 status pill convention. |
| `src/features/inventory/queries.ts::inventoryKeys` | `keys_segment_by_filter_shape` | `inventoryKeys.list({ status: 'low' })` distinct from `inventoryKeys.list({ status: 'neg' })` distinct from `inventoryKeys.list({})`. |
| `src/features/inventory/queries.ts::inventoryKeys` | `audit_key_includes_item_id` | `inventoryKeys.audit('item-1')` -> `['inventory','audit','item-1']`. |
| `src/features/inventory/sanity-cap.ts` (helper extracted from `<AdjustForm>`) | `returns_warning_above_1000_abs_delta_in_either_direction` | `+1001`, `-1001` -> warning; `+1000`, `-1000` -> no warning. Mirror of the Rust sanity cap; pure helper. |
| `src/features/inventory/voided-visit-reversal.ts` (helper) | `detects_offset_pair_by_visit_id_and_sign` | Given a consume row and a positive row with the same `visit_id` and equal abs delta, returns `{ isReversal: true }`. Per §7.15 + phase-05 `compute_void_offsets`. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/inventory/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::inventory::domain` |
| `src-tauri/src/domains/inventory/service/**` (excludes the catalog/consumption-map services scoped to phase-03; this phase's services are `adjustment_service`, `quantity_recomputer`) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::inventory::service::adjustment_service domains::inventory::service::quantity_recomputer` |
| `src-tauri/src/domains/inventory/infrastructure/**` (the SQLite adjustment repo, the partial-index-driven status queries) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::inventory::infrastructure` |
| `src-tauri/src/sync/pull/on_pull_applied_inventory.rs` (the pull-time recompute hook from §7.9) | >= 95% lines (sync engine code per `.claude/rules/testing.md` §8) | `cargo llvm-cov --lib --fail-under-lines 95 -- sync::pull::on_pull_applied_inventory` |
| `src/features/inventory/**`, `src/lib/schemas/inventory.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/inventory/**,src/lib/schemas/inventory.ts"` |
| `src/pages/inventory/**`, `src/components/inventory/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/inventory/**,src/components/inventory/**"` |
| `sync-server/src/app/domains/inventory/domain/**` + `service/**` (the server-side `InventoryAdjustmentService::acceptPush` with the in-tx recompute) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/inventory/presentation/**` (no new routes -- only the push/pull handlers' inventory-adjustment branches) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

Drop rows that don't apply to this phase. Do NOT relax a threshold silently -- a documented override requires §8 sign-off.

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/inventory_phase06.rs` (extend; the file already exists at HEAD from the build cycle with whatever scenarios landed in commit `9c15e33`).
- Auxiliary file: none -- adjustments and items live in the same bounded context and share the same factories.

Existing scenarios at HEAD (do not duplicate; cross-reference only):
- (Whatever the build cycle authored under `commit 9c15e33: phase 6: inventory operations`. Read the file before extending to avoid name collisions.)

**New scenarios in `inventory_phase06.rs`:**

| Scenario | Asserts |
|-|-|
| `create_adjustment_receive_writes_one_row_and_bumps_on_hand` | `qty=5` on an item with `quantity_on_hand=10` -> after commit, the table has one new adjustment row, `inventory_items.quantity_on_hand=15`, `inventory_items.version+=1`, `dirty=1`. |
| `create_adjustment_writeoff_stores_negative_delta_in_db` | Reason `writeoff`, qty `3` -> the persisted `inventory_adjustments.delta == -3`. The user passed a positive qty; the service flipped the sign. |
| `create_adjustment_count_correction_signed_positive_persisted_as_positive` | Reason `count_correction`, signed_delta `+7` -> `delta == 7` in DB; quantity recomputes from new sum. |
| `create_adjustment_count_correction_signed_negative_persisted_as_negative` | signed_delta `-7` -> `delta == -7`. |
| `create_adjustment_count_correction_rejects_zero_delta_at_db_layer` | Bypass the service: try a raw `INSERT ... reason='count_correction', delta=0` -> SQLite returns `CONSTRAINT` violation on `inventory_adjustments_delta_sign` (the unified CHECK from §7.1). Belt-and-suspenders next to the service-level constructor check. |
| `create_adjustment_receive_rejects_negative_delta_at_db_layer` | Raw `INSERT ... reason='receive', delta=-5` -> CHECK violation. Per §7.1. |
| `create_adjustment_writeoff_rejects_positive_delta_at_db_layer` | Raw `INSERT ... reason='writeoff', delta=5` -> CHECK violation. Per §7.1. |
| `create_adjustment_count_correction_rejected_for_receptionist` | Receptionist caller -> `Err(AdjustmentError::Forbidden)`; no row created; outbox unchanged. Per §7.6. |
| `create_adjustment_count_correction_rejected_for_accountant` | Accountant caller -> `Err(Forbidden)`. |
| `create_adjustment_count_correction_succeeds_for_superadmin` | Superadmin caller with signed_delta `-3` -> commits; audit row's `delta` JSON includes `reason='count_correction'` + `actor.role='superadmin'`. |
| `create_adjustment_writes_audit_first_then_business_then_outbox` | Per §7.11 + phase-01 audit-first invariant. Inspect WAL frames or instrument the writer: the `audit_log` rows (1 `create` on `inventory_adjustments` + 1 `update` on `inventory_items` with before/after `quantity_on_hand` per §7.11 step 3.2) precede the `inventory_adjustments` INSERT and the `inventory_items` UPDATE. The outbox INSERTs come last. |
| `create_adjustment_rolls_back_business_when_audit_fails` | Drop the `audit_log` table inside the tx (test feature flag). Expect: no `inventory_adjustments` row, no `inventory_items` update (quantity unchanged), no outbox row. |
| `create_adjustment_rolls_back_audit_when_business_fails` | Force the `inventory_adjustments` INSERT to fail (use an `item_id` that violates FK). Expect: no audit row, no outbox row. |
| `create_adjustment_recompute_uses_sum_with_tombstone_filter` | Seed item with `quantity_on_hand=20` and 3 prior adjustments `[+10, -5, +15]` (sum=20). Soft-delete one of them (`deleted_at=now`). Submit a new `receive` of qty `2`. Expect: recompute yields `(+15 + (-5)) + 2 = 12` because the soft-deleted `+10` is excluded. Per §7.2 SQL. |
| `create_adjustment_emits_metrics_event_when_above_1000_sanity_threshold` | qty `+1500` -> persisted, but `metrics_events` table has a `sanity_cap_warning` row with `reason=receive`, `delta=1500`. Per §7.8. (If the design decision is to NOT emit metrics, this test is replaced by `does_not_emit_metrics` and §7.8 stays a UI-only warning.) |
| `create_adjustment_outbox_op_id_is_stable_across_retries` | Submit the same adjustment with the same client-side `op_id`; the engine treats the second submission as idempotent (per `additive-only` policy + phase-01 idempotency); the outbox has exactly one row. |
| `list_items_filter_status_low_uses_partial_index` | `EXPLAIN QUERY PLAN` for `list_items(status: Low)` mentions `inventory_items_low_stock` partial index. Locks the index from §7.10. |
| `list_items_filter_status_negative_uses_partial_index` | Same for `inventory_items_negative`. |
| `list_items_filter_status_ok_does_not_require_partial_index` | The `Ok` case scans live rows with `deleted_at IS NULL` + filters in Rust (since no expression-based predicate fits a partial index). Asserted: query plan does NOT reference the low/neg indexes; latency still within SLO. |
| `list_items_filter_query_searches_name_case_insensitive_after_2_chars` | `query="Li"` matches "Lidocaine" and "Litholink"; `query="L"` is rejected by Zod (Rust IPC handler asserts the 2-char minimum AFTER trim). Per §7.5. |
| `list_items_filter_include_inactive_default_excludes_is_active_zero` | Items with `is_active=0` excluded by default; `include_inactive=true` includes them. Per §7.5. |
| `list_items_excludes_soft_deleted_regardless_of_filter` | `deleted_at IS NOT NULL` rows never returned. |
| `list_items_includes_dirty_flag_per_row` | Returned `InventoryItemWithStatus` includes `dirty: bool`. Per §7.12 (receipt for phase-05 §7.29). |
| `list_items_response_includes_status_pill_value` | Each row carries `status: 'ok' | 'low' | 'neg'` computed against `low_stock_threshold`. Per PRD §7.3.1. |
| `get_item_returns_joined_consumption_map_and_recent_adjustments` | Response `{ item, consumption_map, recent_adjustments }` populated; consumption_map joins from phase-03; recent_adjustments limited to ~50 most recent (matches §7.15 `<ItemAdjustmentsList>` initial page). |
| `list_adjustments_chronological_pagination` | Seed 75 adjustments for an item; `limit=50, offset=0` -> 50 newest; `limit=50, offset=50` -> remaining 25. Order: `created_at DESC, id DESC`. |
| `list_adjustments_excludes_soft_deleted_consume_visit_offsets_NOT_excluded` | Voided visits' offset rows ARE returned (they are positive +delta rows, NOT soft-deleted). Per §7.15 + phase-05 `compute_void_offsets`. |
| `list_adjustments_returns_reversal_pair_metadata` | Response rows include `is_reversal: bool` flag computed by matching `visit_id` + opposite-sign sibling. Per §7.15. |
| `recompute_on_hand_superadmin_only_at_ipc_layer` | Receptionist caller -> `Err(AdjustmentError::Forbidden)`; superadmin -> succeeds. Per §7.4. |
| `recompute_on_hand_writes_one_update_audit_row` | After a successful recompute, `audit_log` contains a single `update` row on `inventory_items` with `delta.before` and `delta.after` for `quantity_on_hand`. Per §7.4. |
| `recompute_on_hand_does_not_write_an_adjustment_row` | Per §3 IPC description: the debug command recomputes without inserting an adjustment. `inventory_adjustments` row count unchanged. |
| `recompute_on_hand_recovers_drift` | Manually set `inventory_items.quantity_on_hand=999` while the sum of live adjustments is `12`. Run `recompute_on_hand`; assert the value is corrected to `12` and an audit row records the drift. |
| `pull_apply_inventory_adjustment_triggers_recompute_hook` | Per §7.9: simulate a pull batch carrying 2 new `inventory_adjustments` rows for item I; the `SyncEngine::on_pull_applied_inventory` callback fires INSIDE the same SQLite tx as the apply; `QuantityRecomputer::recompute([I.id])` runs; the final `quantity_on_hand` is the locally-summed value. |
| `pull_apply_inventory_item_with_quantity_on_hand_999_is_overwritten` | Per §7.9: pull a batch with an `inventory_items` row carrying `quantity_on_hand=999` AND an `inventory_adjustments` row for the same item with delta `-5`. After apply, the local row has the locally-computed sum (NOT 999). Server-pushed `quantity_on_hand` is informational only. |
| `pull_apply_with_zero_adjustment_rows_does_not_call_recompute` | Pull batch with only `inventory_items` rows (no adjustments) -> the recompute hook is not invoked (empty `affected_item_ids` slice). Per `QuantityRecomputer::recompute` no-op-on-empty contract from §1.1. |
| `migration_006_creates_partial_indexes_idempotently` | `006_inventory_ops.sql` (and the §7.10 partial-index variant) is idempotent: fresh DB AND a DB seeded through `clinical-day.sql` both succeed. After replay, both `inventory_items_low_stock` and `inventory_items_negative` exist (`SELECT name FROM sqlite_master WHERE type='index'`). |
| `migration_006_unified_delta_sign_check_blocks_all_invalid_combos` | After migration, raw INSERT attempts with every invalid `(reason, delta)` combo from §7.1 are rejected. Drives the CHECK predicate end-to-end. |

### §2.2 Tauri IPC handler tests

One test per command in this phase. Happy + at least one error path.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `inventory_list_items` | `list_items_returns_typed_rows_with_status_and_dirty` -> args `{ status: 'low', include_inactive: false, query: undefined }`; assert returned `Vec<InventoryItemWithStatus>` round-trips through `serde_json` and each row carries `status`, `dirty`, `last_adjusted_at`. | `list_items_returns_validation_for_invalid_status` -> args `{ status: 'unknown' }` -> serialized `AppError::Validation` payload `{ kind: "Validation", message: ... }`. |
| `inventory_get_item` | `get_item_returns_item_with_consumption_map_and_recent_adjustments` -> seed an item with a 2-row consumption map and 3 adjustments; assert all three sub-objects populated; assert `is_reversal: false` on all 3 adjustments (no voided visits in setup). | `get_item_returns_not_found_for_unknown_id` -> random UUID -> `AppError::NotFound`. |
| `inventory_list_adjustments` | `list_adjustments_returns_paginated_chronological_rows` -> assert order and limit. | `list_adjustments_rejects_malformed_item_id` -> `item_id="x"` -> `AppError::Validation`. |
| `inventory_create_adjustment` | `create_adjustment_returns_persisted_row_with_updated_item_context` -> reason=receive, qty=5; assert response includes the new adjustment + the updated item snapshot (`{ adjustment, item: { quantity_on_hand: new_value } }` -- the exact response shape lives in the entity context). | `create_adjustment_rejects_count_correction_from_receptionist_via_typed_error` -> receptionist + `count_correction` -> serialized `AppError::Forbidden { kind: 'Forbidden', message: 'count_correction requires superadmin' }`. Per §7.6. |
| `inventory_create_adjustment` | (additional happy-path row) `create_adjustment_warns_on_unusually_large_delta_without_blocking` -> qty=1500; assert: (a) response includes a `warnings: [{ kind: 'unusually_large', threshold: 1000, value: 1500 }]` field; (b) the adjustment IS persisted; (c) `inventory_items.quantity_on_hand` updates. Per §7.8. | (covered above) |
| `inventory_recompute_on_hand` | `recompute_on_hand_returns_new_on_hand_value` -> superadmin caller; manually corrupt `quantity_on_hand=999` with sum of live adjustments=`12`; assert returned `{ newOnHand: 12 }`. | `recompute_on_hand_rejects_non_superadmin_via_typed_error` -> receptionist -> `AppError::Forbidden`. Per §7.4. |

Notes:
- IPC tests construct `AppState` directly, register the same services the runtime uses, and exercise the `#[tauri::command]` async fn (callable as a plain async fn in tests). Same convention as phase-04 and phase-05.
- Each test asserts the serialized error shape, not the Rust enum -- the frontend only sees the JSON, so the JSON is the contract.

### §2.3 Sync server route handlers

Phase 06 adds NO new routes. All adjustment traffic flows through `/sync/push` and `/sync/pull` (declared in phase-01, extended by phase-05 with `inventory_adjustments` entity support). Phase-06's server-side delta is:
- The new raw-SQL migration `<ts>_inventory_adjustments_delta_sign/migration.sql` (per §7.14).
- The role-defence-in-depth check in `InventoryAdjustmentService::acceptPush` rejecting `count_correction` rows whose authoring user is not a superadmin (per §7.6).
- The in-tx recompute of `inventory_items.quantityOnHand` per push apply (per §7.3).

File: `sync-server/test/sync/inventory-adjustments-phase06.test.ts` (NEW; extends `sync/inventory-adjustments.test.ts` from phase-05).

DB: real Prisma test DB; per-test teardown.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_accepts_receive_adjustment_and_recomputes_quantity_on_hand` | Push payload `{ reason: 'receive', delta: 5, item_id: I }`; after apply, `inventory_items` row for `I` has `quantityOnHand` matching the locally-recomputed sum (not the value from a hypothetical client-side push of `quantityOnHand`); `version` incremented; one `audit_log` row written for the item update. Per §7.3. |
| `POST /sync/push` | `push_accepts_writeoff_adjustment_with_negative_delta` | Payload `{ reason: 'writeoff', delta: -3, item_id: I }` -> applied; `quantityOnHand` reflects the decrement. |
| `POST /sync/push` | `push_accepts_count_correction_from_superadmin_actor` | Payload `{ reason: 'count_correction', delta: +7, byUserId: superadmin_user_id }` -> applied. |
| `POST /sync/push` | `push_rejects_count_correction_from_receptionist_actor` | Payload `{ reason: 'count_correction', byUserId: receptionist_user_id }` -> 403 + `error.code: 'COUNT_CORRECTION_REQUIRES_SUPERADMIN'`. Per §7.6 server-side defence in depth. Even if the client bypassed the IPC role check (compromised binary), the server stops the row. |
| `POST /sync/push` | `push_rejects_count_correction_zero_delta_via_check_constraint` | Payload `{ reason: 'count_correction', delta: 0 }` -> 422 from Postgres CHECK violation; row absent from DB. Per §7.7 + §7.14. |
| `POST /sync/push` | `push_rejects_receive_with_negative_delta_via_check_constraint` | Payload `{ reason: 'receive', delta: -5 }` -> 422 CHECK violation. Per §7.1 + §7.14. |
| `POST /sync/push` | `push_rejects_writeoff_with_positive_delta_via_check_constraint` | Payload `{ reason: 'writeoff', delta: 5 }` -> 422. Per §7.1 + §7.14. |
| `POST /sync/push` | `push_is_idempotent_on_op_id_with_recompute_only_once` | Replay the same `op_id` -> identical response from `ProcessedOp` cache; row count unchanged; `quantityOnHand` not double-decremented. Per phase-05 §7.36 + §7.3. |
| `POST /sync/push` | `push_applies_consume_visit_adjustment_only_when_visit_exists` | `consume_visit` adjustment whose `visitId` references a server-existent locked visit -> applied; one pointing at a non-existent visit -> 422 FK violation. Per phase-05 inv 5. (Existing scenario from phase-05 -- referenced here for cross-coupling.) |
| `POST /sync/push` | `push_inventory_adjustment_writes_audit_row_for_item_update_in_same_tx` | Per §7.3: every `acceptPush` of an adjustment writes one `audit_log` row `{ entity: 'inventory_items', op: 'update', delta: { before, after, reason } }` inside the same Prisma `$transaction`. Tx rollback removes both rows together. |
| `POST /sync/push` | `push_inventory_adjustment_typebox_validates_delta_sign_before_constraint` | Defence in depth: TypeBox schema rejects the invalid combo BEFORE the SQL CHECK fires. Response is 400 (TypeBox), not 422 (CHECK). The CHECK exists only as a backstop for raw-SQL access. Per §7.14. |
| `GET /sync/pull` | `pull_returns_inventory_adjustments_for_tenant_only` | Two tenants seeded; the token's tenant gets only its rows. |
| `GET /sync/pull` | `pull_does_NOT_include_server_recomputed_quantity_in_a_separate_envelope` | The pull response carries `inventory_items` rows with `quantityOnHand` as a regular column. The CLIENT is expected to recompute (§7.9). Server's value is treated as informational; the contract here is that the pull payload does NOT include a "server-authoritative quantity" marker -- the client decides. (This test pins the protocol contract; if the contract changes, this test must change first.) |

### §2.4 React Query mutation / query flows

`src/features/inventory/__tests__/queries.test.tsx`. Mocked IPC via `vi.mock('@/lib/ipc', ...)` returning typed stubs.

RTL invariant (mandatory): every component / hook test that renders DOM MUST run in both `dir=ltr` AND `dir=rtl`. Use `describe.each([['ltr'], ['rtl']])(...)` and assert layout invariants per `.claude/rules/design-system.md` §12.

| Hook | Test | Asserts |
|-|-|-|
| `useInventoryItems` | `caches_under_inventory_items_list_filter_key` | First mount -> loading -> data; second mount uses cache (no extra `invoke`). Distinct filter -> distinct cache entry. |
| `useInventoryItems` | `is_disabled_outside_tauri` | `isTauri()` mocked to `false` -> `enabled=false`, query never fires. |
| `useInventoryItems` | `passes_status_query_include_inactive_through_to_ipc` | Caller args reach `invoke('inventory_list_items', ...)` unchanged. |
| `useInventoryItem` | `passes_id_to_ipc_and_caches_per_id` | -- |
| `useInventoryAdjustments` | `uses_60s_stale_time` | After 30s, `dataUpdatedAt` unchanged. |
| `useInventoryAdjustmentCreate` | `invalidates_inventory_items_and_audit_and_adjustments_keys` | After `mutateAsync`, the QueryClient observes invalidation of `['inventory','items']`, `['inventory','adjustments', itemId]`, `['inventory','audit', itemId]`. |
| `useInventoryAdjustmentCreate` | `surfaces_forbidden_typed_error_to_caller` | IPC mock rejects with `{ kind: 'Forbidden', message: 'count_correction requires superadmin' }` -> `mutation.error` carries the typed shape; the form surfaces an i18n message via `errors:inventory.forbidden.count_correction`. |
| `useInventoryAdjustmentCreate` | `surfaces_unusually_large_warning_via_onSuccess_payload` | IPC stub returns `{ adjustment, item, warnings: [{ kind: 'unusually_large', ... }] }`; assert the mutation's `onSuccess` receives the warning; assert the form renders the warning toast WITHOUT blocking the save. |
| `useInventoryItemAuditLog` | `caches_per_item_id_and_invalidates_on_create_adjustment` | -- |

Components covered separately (component tests run in `describe.each([['ltr'], ['rtl']])`):
- `<InventoryItemsTable>` renders columns: Name / Unit / On hand (mono, tnum) / Threshold / Status pill / Last adjusted / Pending-sync. Pending-sync column renders `<DirtyDot dirty={row.dirty === 1} />` (per §7.12, receipt for phase-05 §7.29). RTL: numeric columns left-aligned (page edge); status pill dot leads label in both directions.
- `<InventoryItemsTable>` filter row: status chips (`OK | LOW | NEG`), active toggle (`Active only | All`), search input (debounced 250ms, min 2 chars; reuses `src/lib/search.ts`). Per §7.5.
- `<StockStatusPill>` renders the `OK / LOW / NEG` color + text with the `.claude/rules/design-system.md` §5.2 pill convention (Inter 600 uppercase 11px, `0.04em` tracking, color from `--success / --gold / --crimson`).
- `<ItemDetailTabs>` orchestrates tab switching with i18n keys `inventory.item.tabs.{overview,consumption_map,adjustments,audit}`. Per §7.15.
- `<ItemOverview>` renders on-hand + threshold + status badges. Tabular numerals on the on-hand value.
- `<ItemConsumptionMapTable>` is read-only with a redirect button to `/admin/catalog/checks/<slug>` (phase-03 surface). Per phase-06 §3 frontend.
- `<ItemAdjustmentsList>` renders chronological rows. Reversal rows from voided visits show `<Badge variant="reversal">` with a tooltip linking to the voided visit at `/reception/visits/:id?mode=readonly` (per phase-05 §7.24 + §7.15). Reversal detection uses the `is_reversal` flag from the IPC response.
- `<ItemAuditTab>` filters `audit_log` to `entity='inventory_items'` + `entity_id = item.id`.
- `<AdjustForm>` flow: item picker (combobox; reuses the catalog picker from phase-03) -> reason radio (`receive` / `writeoff` / `count_correction`) -> qty/delta input -> optional note -> submit. Per §4 frontend.
- `<AdjustForm>` qty input: positive integer for receive/writeoff; signed integer for count_correction with helper text `inventory.adjust.helper.count_correction_signed` ("Negative values reduce stock"). Per §7.15.
- `<AdjustForm>` count_correction radio: HIDDEN for non-superadmin users (per phase-06 §4 permission gates); the form's Zod schema also blocks the value if a compromised client tries to submit it via dev tools.
- `<AdjustForm>` action bar: `[Save adjustment]` (primary crimson per design-system §9) and `[Cancel]` (ghost, navigates to `/inventory`). Per §7.15.
- `<AdjustForm>` sanity warning: renders an inline `<Alert variant="warning">` when `|delta| > 1000` ("Unusually large adjustment - confirm"); does NOT block submit. Per §7.8.
- `<AdjustForm>` submit dispatches `useInventoryAdjustmentCreate` and navigates back to `/inventory` or `/inventory/items/:id` on success.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Phase 06 adds NO new server routes. The contract surface this phase adds is:
- The `count_correction` branch of `InventoryAdjustmentPushSchema` (extending phase-05's schema).
- The new error code `COUNT_CORRECTION_REQUIRES_SUPERADMIN` in the push response envelope.

Harness: `sync-server/test/contract/inventory-adjustments-phase06-contract.test.ts` (extends the phase-05 harness).

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (request) | `InventoryAdjustmentPushSchema` (per-reason discriminator: receive / writeoff / count_correction / consume_visit) | `fixtures/payloads/adjustment-push-receive.json`, `...-writeoff.json`, `...-count-correction-signed-positive.json`, `...-count-correction-signed-negative.json`. Each MUST validate. |
| `POST /sync/push` (request, negative) | `InventoryAdjustmentPushSchema` | `fixtures/payloads/adjustment-push-receive-negative-delta.json` MUST fail TypeBox with the custom keyword `deltaSignByReason` mentioning `reason='receive'`. `adjustment-push-writeoff-positive-delta.json` MUST fail similarly. `adjustment-push-count-correction-zero-delta.json` MUST fail. |
| `POST /sync/push` (response) | `SyncPushResponseSchema` (per-op `results[]` with the new error code) | Captured live response after pushing a count_correction from a receptionist token. Validates `status='rejected'` + `details.error.code='COUNT_CORRECTION_REQUIRES_SUPERADMIN'`. |
| `GET /sync/pull` (response) | `InventoryAdjustmentResponseSchema` (already declared in phase-05; phase-06 asserts no schema drift) | Captured live; validates. The phase-06 contract test asserts that NO new field has been silently added on either the server or the client side. |

### §3.2 IPC shape contract

Diff Rust `serde` JSON shape vs TS `Zod` declaration. Fail on drift.

Harness: `src/test-utils/ipc-contract.test.ts` -- already established by phase-04 and phase-05; phase-06 extends it with the 5 new commands.

The last row is FIXED -- every phase that adds an IPC command also exercises the shared error envelope. Do not remove it.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `inventory_list_items` | `Vec<InventoryItemWithStatus>` | (NEW) `InventoryItemWithStatusSchema = InventoryItemSchema.extend({ status: z.enum(['ok','low','neg']), dirty: z.boolean(), last_adjusted_at: z.string().datetime().nullable() })` per phase-06 §3 + §7.12. |
| `inventory_get_item` | `InventoryItemDetail { item, consumption_map, recent_adjustments }` | (NEW) `InventoryItemDetailSchema = z.object({ item: InventoryItemWithStatusSchema, consumption_map: z.array(ConsumptionMapEntrySchema), recent_adjustments: z.array(InventoryAdjustmentWithMetaSchema) })`. The consumption_map row schema is owned by phase-03. The `WithMeta` extension on adjustments adds `is_reversal: z.boolean()` per §7.15. |
| `inventory_list_adjustments` | `Vec<InventoryAdjustmentWithMeta>` | `z.array(InventoryAdjustmentWithMetaSchema)`. |
| `inventory_create_adjustment` | `CreateAdjustmentResult { adjustment, item, warnings }` | (NEW) `CreateAdjustmentResultSchema = z.object({ adjustment: InventoryAdjustmentSchema, item: InventoryItemWithStatusSchema, warnings: z.array(AdjustmentWarningSchema) })` where `AdjustmentWarningSchema = z.object({ kind: z.literal('unusually_large'), threshold: z.number(), value: z.number() })`. Per §7.8. |
| `inventory_recompute_on_hand` | `RecomputeResult { new_on_hand }` | (NEW) `RecomputeResultSchema = z.object({ new_on_hand: z.number().int() })`. |
| (Error envelope -- fixed) | `AppError` serialized via `Serialize` impl | `AppErrorSchema = z.object({ kind: z.enum([...]), message: z.string() })` -- one shared schema referenced by every command's error path. New variants this phase introduces: `AdjustmentError::Forbidden`, `AdjustmentError::QtyNonPositive`, `AdjustmentError::CountCorrectionMustBeNonZero`, `AdjustmentError::NotUserSelectable` -- each MUST be in the `kind` enum. |

The harness MUST also assert the inverse: every Zod-declared field appears in the Rust JSON. A field added on either side without updating the other fails the contract test.

### §3.3 Sync envelope contract

- **Push payload conforms.** `InventoryAdjustmentPushPayload` (Rust, extending the phase-05 type with the `count_correction` reason branch) serialized to JSON -> validate against the server's `InventoryAdjustmentPushSchema` (TypeBox). Test fixture: `fixtures/payloads/adjustment-push-count-correction-canonical.json`.
- **Pull payload conforms.** Server's `InventoryAdjustmentResponseSchema` JSON output -> validate against a mirrored Zod schema on the client. The contract MUST NOT drift between phases.
- **Conflict-resolution policy declared and matches expectation.** Assert the engine's policy registry returns `('inventory_adjustments', 'additive-only')`. Per phase-06 §4 sync-semantics table (unchanged from phase-05).
- **Versioned envelope.** Push body carries `envelope_version: 1`; a stub at `envelope_version: 999` is rejected.
- **Snapshot files** (per `.claude/rules/testing.md` §10):
  - `expected/sync/adjustment-push-count-correction-canonical.json.sha256` -- the new reason branch.
  - Phase-05's existing snapshots (`adjustment-push-consume-canonical.json.sha256`, `adjustment-push-receive-canonical.json.sha256`, `adjustment-push-writeoff-canonical.json.sha256`) are inherited from phase-05; this phase asserts they have NOT drifted (re-hash and compare).

---

## §4 E2E Tests (Pyramid Layer 4)

WebdriverIO + `tauri-driver`. Specs live under `e2e/specs/inventory/`. Every selector is `data-testid` per `.claude/rules/testing.md` §14 anti-patterns -- never CSS classes, never DOM position.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `inventory-list-and-filter.e2e.ts` | Mehdi (`receptionist`) | 1) Log in. 2) Navigate to `/inventory`. 3) Assert items table renders with status pills. 4) Click the `Low` filter chip. 5) Assert the table updates to show only LOW + NEG items (NEG takes precedence). 6) Search "Lid". 7) Assert results filter to "Lidocaine". 8) Toggle `Active only` -> `All`; assert inactive items appear with a muted style. | Filter combinations are URL-stable (refresh restores state). The `inventory_items_low_stock` partial index is hit (asserted by a follow-up `EXPLAIN QUERY PLAN` IPC, not by the E2E itself -- the integration test owns that). |
| `inventory-item-detail-tabs.e2e.ts` | Mehdi | 1) From `/inventory`, click "Lidocaine". 2) Land on `/inventory/items/<id>`. 3) Assert `<ItemDetailTabs>` renders 4 tabs: Overview, Consumption Map, Adjustments, Audit. 4) Switch through each; assert content loads. 5) On Adjustments, assert the most recent voided-visit reversal row renders with `<Badge variant="reversal">`; click the tooltip; assert it links to `/reception/visits/:id?mode=readonly`. | Each tab loads within the SLO; consumption map shows the read-only redirect to `/admin/catalog`; audit tab shows the filtered audit log entries. |
| `adjust-receive-happy-path.e2e.ts` | Mehdi | 1) From `/inventory`, click `+ Adjust`. 2) On `/inventory/adjust`, pick "Lidocaine"; pick reason=`receive`; enter qty=5; submit. 3) Assert navigation back to `/inventory`. 4) Assert "Lidocaine"'s `quantity_on_hand` increased by 5. | Outbox grows by 1; after server reachable, drains to 0; `audit_log` has 2 new rows (create on adjustment, update on item) in audit-first order; `inventory_items.version` incremented. |
| `adjust-writeoff-happy-path.e2e.ts` | Mehdi | Reason=writeoff; qty=3 (the UI shows "decrease by 3"). | The persisted `inventory_adjustments.delta == -3`. `quantity_on_hand` decreases by 3. |
| `adjust-count-correction-superadmin.e2e.ts` | Mariam (`superadmin`) | 1) Log in. 2) Navigate to `/inventory/adjust`. 3) Assert the `count_correction` radio is VISIBLE. 4) Pick `count_correction`; enter signed delta `-3`; submit. | Persisted row has `reason='count_correction', delta=-3`. Audit row carries the actor's superadmin id. |
| `adjust-count-correction-hidden-for-receptionist.e2e.ts` | Mehdi (`receptionist`) | Navigate to `/inventory/adjust`. | The `count_correction` radio is NOT rendered. Per §7.6 UI gate. |
| `adjust-unusually-large-warns-but-saves.e2e.ts` | Mehdi | Pick reason=receive; enter qty=1500; submit. | Warning toast surfaces: "Unusually large adjustment - confirm". The save proceeds; the row IS persisted; the form does NOT block. Per §7.8. |
| `inventory-route-role-guard-for-accountant.e2e.ts` | Asma (`accountant`) | Attempt to navigate to `/inventory`. | Redirected by `<RequireRole roles={['receptionist','superadmin']}>` to `/no-access`. The `<UserMenu>` does NOT show the Inventory link. Per §7.13. |
| `inventory-link-hidden-for-accountant.e2e.ts` | Asma | Open `<UserMenu>`. | No `Inventory` link in the menu (role-gated render). |
| `voided-visit-reversal-renders-positive-row.e2e.ts` | Mehdi | 1) Open an item that was consumed by a voided visit. 2) Switch to Adjustments tab. | The reversal row renders as POSITIVE delta with the `reversal` badge. The original consume row (negative delta) also appears. Both rows have the same `visit_id`. The math nets to zero. |

### §4.2 Failure-path flows

- **`offline-adjust-receive-drains-on-reconnect.e2e.ts`** -- Set `--offline` on tauri-driver; submit a receive adjustment; assert the UI confirms; sync pill shows offline; the row IS persisted locally + recompute fired locally. Lift the offline flag; assert sync pill resumes; assert the adjustment lands on the server; assert the server's recompute yields the same `quantityOnHand` as the local recompute.
- **`token-expiry-mid-adjust-submission.e2e.ts`** -- Force JWT expiry just before submit; click submit; assert one 401 -> automatic refresh -> retry succeeds; assert no duplicate audit row.
- **`server-5xx-during-push-retries-with-backoff.e2e.ts`** -- WireMock the sync server to return 503 three times then 200; submit; assert the outbox row's `attempts` advances; assert it eventually drains; assert no row duplication.
- **`count-correction-bypass-attempt-from-receptionist-rejected-by-ipc.e2e.ts`** -- Receptionist tries to dispatch `inventory_create_adjustment` with `reason='count_correction'` via dev tools (simulating UI bypass). Assert IPC returns `AppError::Forbidden`; assert no row created. Per §7.6.
- **`count-correction-bypass-attempt-from-receptionist-rejected-by-server.e2e.ts`** -- Even if a compromised local binary writes a `count_correction` row to its outbox, the server's `acceptPush` rejects it with `COUNT_CORRECTION_REQUIRES_SUPERADMIN`. The local row is then marked as `rejected` in the outbox and surfaced to the user. Per §7.6 server-side defence in depth.
- **`recompute-on-hand-superadmin-only.e2e.ts`** -- Receptionist tries to invoke `inventory_recompute_on_hand` via the admin-only debug screen (or dev tools). Assert `AppError::Forbidden`. Per §7.4.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

Two binaries, shared sync server seeded from `clinical-day.sql`.

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-concurrent-receive-and-writeoff.e2e.ts` | Device A receives 10 of Lidocaine; Device B writes off 3 of Lidocaine simultaneously. Both offline. Both reconnect. | Server has BOTH rows. Both devices' `quantity_on_hand` for Lidocaine matches the sum: `original + 10 - 3`. No conflict (additive-only). Per phase-06 §4 sync-semantics. |
| `two-device-consume-and-receive-converge.e2e.ts` | Device A locks a visit consuming 1 Lidocaine; Device B receives 5 Lidocaine. Both offline. Both reconnect. | Both rows survive on server. Both devices converge to `original - 1 + 5`. |
| `pull-overrides-server-quantity-on-hand-with-local-recompute.e2e.ts` | Device A locks 3 visits consuming Lidocaine. Device A reconnects. Device B (offline meanwhile) pulls. Manually corrupt the server-side `quantityOnHand` to `999` between A's push and B's pull. | After B's pull applies, B's local `quantity_on_hand` for Lidocaine equals the locally-summed value (NOT 999). Per §7.9. The pull-time recompute hook OVERWRITES the pulled value. |
| `voided-visit-reversal-propagates-to-second-device.e2e.ts` | Device A locks a visit consuming 2 Lidocaine. Device A reconnects. Device B pulls (sees `quantity_on_hand` reflects the consume). Device A voids the visit. Device A reconnects. Device B pulls again. | Device B's Adjustments tab for Lidocaine now shows the reversal row with the `reversal` badge. `quantity_on_hand` recovered to the pre-consume value. |
| `three-device-additive-chain.e2e.ts` | Devices A, B, C each submit one receive on the same item offline. All reconnect in random order. | All three rows survive on server. Each device's `quantity_on_hand` converges to `original + sum_of_three_deltas`. No conflicts. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

These are manual checks for things automation cannot verify cheaply:

- **Visual RTL of `/inventory` and `/inventory/items/:id`.** Switch locale to `ar`; switch `arabic_numerals: true`; confirm: (a) eyebrow rule renders on the right, (b) `<InventoryItemsTable>` numeric columns (On hand, Threshold) right-aligned (page edge in RTL) with `tnum` Geist Mono digits in Arabic-Indic form, (c) `<StockStatusPill>` dot leads the label in both directions, (d) `<DirtyDot>` leads the row in both directions, (e) `<ItemDetailTabs>` tab order mirrors (Audit on the far edge becomes Overview's neighbour in RTL).
- **Status pill color match against design system.** `<StockStatusPill>` `OK` uses `--success`, `LOW` uses `--gold`, `NEG` uses `--crimson`. Visually verify against `.claude/rules/design-system.md` §1.4 + §5.2. Snapshot the rendered colors via CSS-extracted hex.
- **Adjust form keyboard navigation.** Navigate `<AdjustForm>` with keyboard only: tab order is item picker -> reason radio group -> qty/delta input -> note -> Save -> Cancel. Enter submits when the form is valid. Escape on the page navigates back to `/inventory`. Per `.claude/rules/design-system.md` §8.
- **count_correction signed input behavior.** Verify the signed `<Input type="number">` accepts negative values with the minus key; verify Arabic-Indic numerals on display when the setting is on; verify the helper text reads correctly in both en and ar. Per §7.15.
- **Reversal tooltip readability.** Hover (or focus via keyboard) over a reversal row's `<Badge variant="reversal">` tooltip; assert the tooltip text fits within the viewport in both directions and doesn't overlap the table chrome. Verify the click navigates to `/reception/visits/:id?mode=readonly`.
- **Active/Inactive filter visual state.** With `Active only` selected, inactive items are absent. Toggle to `All`; inactive items appear with a muted style (`opacity: 0.6` per design-system §5 inactive-token convention). The toggle position visually mirrors in RTL.

### §5.2 Cross-references to `personas.md`

Phase 06 surfaces are exercised end-to-end by:
- `personas.md` -> **P2 Mehdi the Receptionist** -> steps that touch inventory across the day (receiving stock at start, writeoff during cleanup, browsing `/inventory` to confirm levels, voided-visit reversal verification). P2 is listed in `personas.md` as touching phases 02 / 04 / 05 / 06 -- this plan is the surface for the phase-06 coverage row. Required for §8 DoD.
- `personas.md` -> **P4 Two-Device Conflict** -> the inventory additive-only cell (concurrent receives + writeoffs across devices). Optional reinforcement.
- `personas.md` -> **P3 Mariam the Superadmin** -> step for count_correction + `inventory_recompute_on_hand` debug invocation. Optional reinforcement.

**Canonical: P2 Mehdi the Receptionist.** P2 MUST pass for §8 DoD to flip to `complete`.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **Asia/Baghdad fixed offset.** `inventory_list_items.last_adjusted_at` and `inventory_list_adjustments.created_at` are stored in UTC and rendered in Asia/Baghdad local time. A row with `created_at = 2026-05-13T20:59:30Z` (i.e. `23:59:30 +03:00` local) displays as `2026-05-13 23:59`, not the next day. Asserted in `last_adjusted_at_renders_in_baghdad_local_time` (TS) + `created_at_window_uses_baghdad_local_midnight` (Rust) for any date-bucketed list filter.
- **Day boundary on filter.** If `<InventoryItemsTable>` ever surfaces a "today's adjustments" filter (not in scope for phase-06 but kept for future-proofing), the window uses Baghdad local midnight, not UTC. Currently N/A -- this phase has no time-bucketed filters; the cross-check still lives here so a future surface inherits the convention.
- **Clock skew vs server.** Submit an adjustment locally; the server's `updatedAt` is server-authoritative; pull-back replaces the local `updated_at` with the server stamp. Asserted in `adjustment_pullback_uses_server_updated_at`.
- **DST defensive.** CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `domains/inventory/`; only `chrono::FixedOffset::east_opt(3 * 3600)`. Iraq has no DST -- the code must not assume DST anyway.

### §6.2 i18n & RTL

- **en/ar swap on every route this phase added.** `/inventory`, `/inventory/items/:id` (all 4 tabs), `/inventory/adjust`. Cross-cutting full sweep in `i18n-rtl.md`; this plan asserts every visible string comes from `inventory.*` or `errors:inventory.*` i18n keys -- no string literals in JSX. Asserted by a `grep`-style test in §2.4 component tests.
- **Arabic-Indic numerals on every numeric column.** `<InventoryItemsTable>::{on_hand,threshold,last_adjusted}`, `<ItemOverview>` on-hand value, `<AdjustForm>` qty input (the rendered value, not the underlying number model), `<ItemAdjustmentsList>` delta column. Per `.claude/rules/design-system.md` §2.4 + §11.
- **RTL layout invariants.** Eyebrow rule on the right of the page title; numeric columns aligned to the page edge in RTL (right-align in LTR -> left-align in RTL); pill dots leading their label in both directions; `<DirtyDot>` leading the row in both directions; `<ItemDetailTabs>` tab order mirrors (the active-tab underline indicator flips sides).
- **Mixed-direction note field.** The `note` field accepts Arabic + English mixed; assert no Unicode bidi mangling on save+reload (stored bytes match input bytes). Mirrors the patient-name byte-stability test from phase-05.
- **Signed-delta input with Arabic-Indic digits.** When `arabic_numerals=true`, typing `-` followed by `٣` produces an underlying numeric value of `-3` but renders as `-٣`. The minus sign must NOT flip via bidi to the right of the digit -- the form must apply an LRM mark to lock its position. Asserted in `format-quantity.test.ts` + `<AdjustForm>` component test (RTL slice).

### §6.3 Offline & Network

- **Full offline mode.** `offline-adjust-receive-drains-on-reconnect.e2e.ts` (§4.2). All 5 inventory IPC commands work offline; the UI never blocks on a network call for any read.
- **Intermittent connection.** Submit 5 adjustments in quick succession; drop the connection mid-3rd push; assert the engine retries from op 3, not op 1. Asserted in `intermittent-push-resumes-cleanly-for-adjustments.e2e.ts`.
- **Token expiry mid-sync.** `token-expiry-mid-adjust-submission.e2e.ts` (§4.2). One 401 -> refresh + retry once; second 401 -> pause pushes, surface `session_expired`.
- **Server returns 5xx.** `server-5xx-during-push-retries-with-backoff.e2e.ts` (§4.2). Exponential backoff respected.
- **Partial-batch push.** Push 50 adjustment ops where op 27 violates a server-side CHECK (e.g. count_correction with delta=0 -- bypasses TypeBox in a compromised client but trips the CHECK). Ops 1-26 applied, op 27 rejected, ops 28-50 still applied. Per `.claude/rules/sync-server.md` per-op result contract.

### §6.4 Concurrency & Conflicts

- **2-device same item (`additive-only` policy).** `two-device-concurrent-receive-and-writeoff.e2e.ts` (§4.3). Both rows survive; `quantity_on_hand` recomputes from the union of deltas.
- **3-device chain.** `three-device-additive-chain.e2e.ts` (§4.3). Deterministic convergence; all three rows survive.
- **Conflict policy invocation.** Assert the policy registry returns `additive-only` for `inventory_adjustments`; assert no `manual` 409 response is ever emitted for an inventory adjustment push. Per phase-05 §7.36 + phase-06 §4 sync-semantics.
- **Conflict resolver round-trip.** N/A -- owned by phase-08 test. `inventory_adjustments` never parks in `ConflictParked` (additive). Assert that the conflict resolver UI lists ZERO inventory rows even when adjustments are flying around. Cross-references phase-08 test.
- **Consume-vs-void race.** Device A locks a visit consuming Lidocaine; Device B (offline) voids a different visit that consumed Lidocaine yesterday. Both reconnect. Both consume + reversal rows survive; `quantity_on_hand` recomputes from the union. (This is the cross-coupling cell mentioned in `Out of scope`; the cross-product against `visits` manual policy is in `sync-conflicts.md`.)

### §6.5 Crash & Recovery

- **SIGKILL during create_adjustment transaction.** Spawn the binary in a test harness, fire `inventory_create_adjustment`, kill the process between (a) audit-row INSERT and (b) adjustment INSERT (instrument via a feature-gated `panic!` between the two writes). Reopen; assert: no `inventory_adjustments` row, no `audit_log` row, no `inventory_items` update (quantity unchanged), no `outbox` row. Audit-first ordering plus tx rollback guarantees this. Test: `crash_mid_create_adjustment_leaves_no_partial_state`.
- **SIGKILL between recompute and commit.** Instrument the writer to panic after the `inventory_items` UPDATE but before commit. Reopen; assert the whole tx rolled back (WAL frame never marked committed); assert `quantity_on_hand` unchanged; no audit / outbox / adjustment row.
- **SQLite WAL after crash.** Kill the binary while WAL has uncommitted frames from a create_adjustment. Reopen with `journal_mode=WAL` + `busy_timeout=5000`; assert recovery is clean, no orphan WAL files, all queries succeed. Test: `wal_recovery_after_adjustment_crash`.
- **Disk full on write path.** Mount a tmpfs sized just below the migration footprint + 1 row; attempt create_adjustment; assert `AppError::Db` with a clear "disk full" message; no half-written row. Test: `disk_full_on_create_adjustment_returns_typed_error` (gated `--ignored` in CI).
- **Atomicity of multi-step transactions.** `create_adjustment_rolls_back_business_when_audit_fails` + `create_adjustment_rolls_back_audit_when_business_fails` in §2.1 cover the two failure modes.
- **Pull-time recompute hook crash.** Per §7.9: simulate a crash WITHIN the pull-tx after applying adjustments but before the recompute callback runs. Assert the whole pull-batch tx rolls back (the recompute hook is inside the SAME tx, so its failure rolls back the entire apply). On reopen, the adjustment rows from that pull batch are absent. Test: `pull_apply_with_recompute_failure_rolls_back_entire_batch`.

### §6.6 Scale & Performance

- **10k inventory items.** `inventory_list_items({ status: 'low' })` on a 10k-item fixture with 30% LOW + 5% NEG: < 30 ms p99. The `inventory_items_low_stock` partial index is hit. Asserted in `perf_list_items_low_at_10k`.
- **1k items with 50k adjustments.** `inventory_get_item(id)` returning the joined consumption map (~5 entries) and recent_adjustments (50 most recent): < 30 ms p99. Asserted in `perf_get_item_at_50k_adjustments`.
- **Outbox drain throughput.** Backlog of 500 inventory-adjustment ops (mixed reasons) -> drain at >= 50 ops/sec (default SLO from `.claude/rules/testing.md` §9). Asserted in `perf_outbox_drain_adjustment_backlog`.
- **Server-side recompute under fan-out.** Per §7.3: a single `/sync/push` batch carrying 50 adjustments across 10 distinct items triggers 10 in-tx recompute UPDATEs; total handler latency < 200 ms p95. Asserted in `perf_server_push_50_adj_10_items_recompute`.
- **Pull-time recompute hook.** Pull a batch with 100 adjustment rows across 20 items; the local recompute callback runs once per item (20 UPDATEs) within the same tx; total apply latency < 300 ms p95. Asserted in `perf_pull_recompute_hook_100_rows_20_items`.

### §6.7 Security & Permissions

- **Role bypass: receptionist tries count_correction at IPC layer.** Per §2.2 error-path test. `AppError::Forbidden` returned; no mutation. Per §7.6.
- **Role bypass: receptionist tries count_correction at SERVER layer.** Per §2.3 server test. `acceptPush` rejects with 403 + `COUNT_CORRECTION_REQUIRES_SUPERADMIN` even if the local IPC layer was somehow bypassed. Defence in depth per §7.6.
- **Role bypass: accountant tries `/inventory/*` routes.** `<RequireRole roles={['receptionist','superadmin']}>` redirects to `/no-access`. Per §7.13. Asserted in `inventory-route-role-guard-for-accountant.e2e.ts`.
- **Role bypass: receptionist tries `inventory_recompute_on_hand`.** `AppError::Forbidden`. Per §7.4 + §2.2.
- **JWT tampering.** Alter `role` claim from `receptionist` to `superadmin` and replay against the sync server's `/sync/push` with a count_correction payload. Assert 401 (signature invalid) -- the server NEVER trusts the claim shape; it verifies RS256. Cross-cutting in `security.md`; receipt only here.
- **CHECK constraint as backstop against raw-SQL injection.** A SQL injection that bypasses the application layer and writes a `{reason: 'receive', delta: -5}` row via raw access STILL trips the SQLite CHECK constraint (local) and the Postgres CHECK constraint (server, per §7.14). Asserted in `create_adjustment_receive_rejects_negative_delta_at_db_layer` (Rust integration) + a sync-server integration test that exec's raw SQL.
- **Soft-delete bypass.** Soft-delete an adjustment row (this is unusual but technically allowed for sync ordering); then call `inventory_list_adjustments` -- assert the row IS still returned ONLY IF it's a voided-visit reversal pair sibling (per §7.15 -- reversals are real positive rows, not soft-deletes). Actual soft-deletes are excluded. Then bypass via raw `SELECT * FROM inventory_adjustments WHERE id = ?` -- assert the row IS still in the table. Integration test `soft_delete_adjustment_hides_from_reads_but_persists`.
- **count_correction zero-delta forgery.** Even with a compromised client that bypasses TypeBox, the SQLite + Postgres CHECK constraints reject `{reason:'count_correction', delta:0}`. Per §7.1 + §7.7 + §7.14. Asserted in §2.1 + §2.3.
- **Refresh-token replay.** N/A -- owned by `security.md`. Cross-reference receipt only.

### §6.8 Data Integrity

- **Migration replay forward.** `006_inventory_ops.sql` (and `006_inventory_indexes.sql` per §7.10) is idempotent on fresh DB AND on a DB seeded through `clinical-day.sql`. The per-reason delta-sign CHECK from §7.1 + the partial indexes from §7.10 are all re-creatable. Asserted in `migration_006_idempotent_on_populated_db`.
- **Migration replay against populated DB.** Pre-load phase-01..05 data + a snapshot of an in-flight 006 install; replay 006; assert no constraint violations on existing rows (the CHECK predicate must accept all existing seed rows). Test: `migration_006_check_constraint_accepts_existing_seed_rows`.
- **FK enforcement.** Insert an `inventory_adjustments` row with `item_id` pointing to a non-existent item -> FK violation. Same for `by_user_id`, `visit_id` (when reason=consume_visit). Test: `fk_enforcement_blocks_orphan_adjustments`.
- **Soft-delete cascade rules.** Soft-deleting an `inventory_items` row when adjustments reference it: per phase-03's rule (revisit in phase-03 test). Adjustments are NEVER hard-deleted by an item soft-delete; they stay as historical record.
- **`sync_version` monotonicity.** Every `create_adjustment` invocation increments `inventory_items.version` by exactly 1 (for the affected item). 3 adjustments in sequence -> `version` goes 0, 1, 2, 3 (assuming starting at 0). Asserted in `version_increments_monotonically_per_adjustment`.
- **CHECK constraint enforcement -- exhaustive.** Try all 4 invalid (reason, delta) combos via raw INSERT -> all rejected:
  - `{receive, delta=0}` -> CHECK violation.
  - `{receive, delta<0}` -> CHECK violation.
  - `{writeoff, delta=0}` -> CHECK violation.
  - `{writeoff, delta>0}` -> CHECK violation.
  - `{count_correction, delta=0}` -> CHECK violation.
  - `{consume_visit, visit_id=NULL}` -> CHECK violation (existing from phase-05 §1).
  Asserted in `inventory_adjustments_check_constraint_blocks_invalid_states` (one test per branch).
- **Append-only trigger from phase-05.** Attempting `UPDATE inventory_adjustments SET delta = -10 WHERE id = ?` -> RAISE(ABORT). Sync-metadata-only update (`version`, `dirty`, `last_synced_at`) is allowed. Per phase-05 §7.33. Re-asserted here for completeness.
- **`quantity_on_hand` consistency.** After any sequence of N adjustments, `inventory_items.quantity_on_hand == SUM(delta) WHERE deleted_at IS NULL`. Property test: 100 random sequences of mixed-reason adjustments + occasional soft-deletes; assert the invariant after every mutation. Asserted in `quantity_on_hand_matches_sum_of_live_adjustments_property_test`.

---

## §7 Performance SLOs (this phase's surfaces)

Default SLOs in `.claude/rules/testing.md` §9 apply unless overridden. The `Default?` column declares whether the threshold is the §9 default (`yes`) or a phase-specific override (`no`).

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `inventory_list_items({status:'low'})` over 10k items | < 30 ms p99 | yes | `perf_list_items_low_at_10k` | Default list-query SLO; index-driven via `inventory_items_low_stock`. |
| Tauri (SQLite) | `inventory_list_items({status:'neg'})` over 10k items | < 30 ms p99 | yes | `perf_list_items_neg_at_10k` | Default list-query SLO; index-driven via `inventory_items_negative`. |
| Tauri (SQLite) | `inventory_list_items({status: undefined, query: 'lid'})` over 10k items | < 50 ms p99 | no (LIKE query without expression index) | `perf_list_items_query_at_10k` | LIKE scans live rows with `name LIKE ?`; tighter than 80ms because it's user-visible search; loose enough to allow the scan since name is short. |
| Tauri (SQLite) | `inventory_get_item(id)` over 50k adjustments | < 30 ms p99 | yes | `perf_get_item_at_50k_adjustments` | Default list-query SLO; joined consumption_map + recent_adjustments (limit 50) is index-driven. |
| Tauri (SQLite) | `inventory_list_adjustments(itemId, limit=50, offset=0)` over 50k adjustments | < 30 ms p99 | yes | `perf_list_adjustments_first_page` | Default list-query SLO. |
| Tauri (SQLite) | `inventory_create_adjustment` typical case (1 adjustment + 1 item recompute + audit fan-out + outbox + commit) | < 50 ms p99 | no (tighter than §9's 200ms lock SLO since adjustments are simpler than visit locks) | `perf_create_adjustment_typical_under_50ms` | Adjust form is a high-frequency surface; under 50ms feels instant. |
| Tauri (SQLite) | `inventory_recompute_on_hand` for 1 item with 1k adjustments | < 30 ms p99 | yes | `perf_recompute_on_hand_at_1k_adj` | Default list-query SLO; single SUM with tombstone filter is index-driven. |
| Tauri (IPC) | `inventory_create_adjustment` full round-trip (Tauri serialize + Rust + commit + deserialize) | < 80 ms p99 | yes | `perf_create_adjustment_ipc_round_trip` | Default IPC round-trip target. |
| Sync engine | Drain a 500-op adjustment backlog | >= 50 ops/sec | yes | `perf_outbox_drain_adjustment_backlog` | §9 default. |
| Sync engine | Push a single adjustment op (round-trip) | < 1 s p95 | yes | `perf_push_single_adjustment_op` | §9 default. |
| Sync engine | Pull-time recompute hook for 20 affected items in a 100-adj batch | < 100 ms p95 | no | `perf_pull_recompute_hook_100_rows_20_items` | The hook runs in-tx; must complete fast or it bottlenecks the whole pull. |
| Sync server (Postgres) | `/sync/push` handler latency for a 50-op adjustments batch hitting 10 distinct items (10 in-tx recomputes) | < 200 ms p95 | yes | `perf_server_push_50_adj_10_items_recompute` | §9 default. |
| Sync server (Postgres) | `/sync/pull` handler latency for a 100-row adjustments page | < 200 ms p95 | yes | `perf_server_pull_100_adjustments` | §9 default. |
| Frontend | `<InventoryPage>` (`/inventory`) first paint with 100 rows | < 200 ms | -- (no §9 default) | `perf_inventory_page_cold_paint_100_rows` | Includes the `<DirtyDot>` per row; one IPC + render pass. |
| Frontend | `<ItemDetailPage>` (`/inventory/items/:id`) first paint | < 250 ms | -- | `perf_item_detail_cold_paint` | One IPC + 4-tab orchestration + initial Overview tab render. |
| Frontend | `<AdjustForm>` (`/inventory/adjust`) first paint | < 150 ms | -- | `perf_adjust_form_cold_paint` | Simple form; no joined IPC. |
| Frontend | `<AdjustForm>` submit-to-confirmation roundtrip | < 200 ms p99 | -- | `perf_adjust_form_submit_roundtrip` | IPC + cache invalidation + navigation. |

Perf tests run in `cargo test --test inventory_perf_phase06 --release` + `vitest run --mode benchmark`. Variance failures are real bugs.

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI (`cargo test -p app_lib --lib` + `vitest run --project unit`).
- [ ] All §2 integration tests green in CI:
  - `cargo test --test inventory_phase06`
  - IPC handler tests for all 5 commands listed in §2.2.
  - `pnpm --filter sync-server test -- sync/inventory-adjustments-phase06`
  - `vitest run --project integration`
- [ ] All §3 contract tests green in CI (`pnpm test:contract`).
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- inventory/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P2 Mehdi the Receptionist** runs end-to-end and passes (record date / runner in row below).
- [ ] §6 all eight edge categories addressed (no empty subsections).
- [ ] §7 SLOs met for every row; override rows have a recorded rationale in the test source.
- [ ] Coverage gates met per §1.3:
  - [ ] `domains::inventory::domain` >= 90%
  - [ ] `domains::inventory::service::adjustment_service` >= 90%
  - [ ] `domains::inventory::service::quantity_recomputer` >= 90%
  - [ ] `domains::inventory::infrastructure` >= 75%
  - [ ] `sync::pull::on_pull_applied_inventory` >= 95%
  - [ ] Frontend `src/features/inventory/**`, `src/lib/schemas/inventory.ts` >= 90%
  - [ ] Frontend `src/pages/inventory/**`, `src/components/inventory/**` >= 60%
  - [ ] Sync server `domains/inventory/domain/**` + `service/**` (the `acceptPush` recompute path) >= 90%
  - [ ] Sync server `domains/inventory/presentation/**` (push/pull handler inventory-adjustment branches) >= 85%
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed where `.claude/rules/testing.md` §10 applies:
  - `expected/sync/adjustment-push-count-correction-canonical.json.sha256` (NEW for this phase -- the count_correction reason branch)
  - Phase-05's existing adjustment snapshots verified non-drifting (re-hash + compare against the committed `.sha256` files):
    - `expected/sync/adjustment-push-consume-canonical.json.sha256`
    - `expected/sync/adjustment-push-receive-canonical.json.sha256`
    - `expected/sync/adjustment-push-writeoff-canonical.json.sha256`
- [ ] `testing-status.md` row updated (Unit / Integration / Contract / E2E / Manual counts, Coverage %, Started / Completed dates, Open Defects).
- [ ] Lint, typecheck, build all green (`pnpm lint && pnpm build && cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd ../sync-server && pnpm lint && pnpm typecheck && pnpm test`).

**Persona run record:**

The first row is the **canonical persona** -- the one persona script that gates `complete` per `.claude/rules/testing.md` §11 ("at least one persona script in `personas.md` exercises this phase's surfaces end-to-end and passes"). Pick exactly one from `personas.md`. Additional rows are optional reinforcement runs.

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P2 Mehdi the Receptionist** | -- | -- | -- | -- |
| P4 Two-Device Conflict (reinforcement) | -- | -- | -- | Optional, exercises `inventory_adjustments` additive-only across two devices. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P06-G01 -- Audit row delta payload shape (CRITICAL)

- **Source:** phase-06.md §7.11 audit-first ordering
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.11 mandates that the audit row for an `inventory_items` update carries a `delta: { before, after, reason }` JSON payload, where `before` and `after` snapshot `quantity_on_hand` and `reason` echoes the adjustment's reason. Existing §2.1 scenarios assert ordering (audit-first, then business, then outbox) but never inspect the payload shape, so a writer that emits a stub `delta: {}` would pass current tests. This gap is CRITICAL because the audit log is the legal record of every quantity change.

| Scenario | Asserts |
|-|-|
| `create_adjustment_audit_row_carries_explicit_before_after_reason_payload` | Submit `receive` qty=5 on an item with `quantity_on_hand=10`. Inspect the `audit_log` row for the `inventory_items` update: `delta` JSON deserializes to `{ before: 10, after: 15, reason: "receive" }`; `before` and `after` are integers (not strings); `reason` is the snake-case enum literal. Per §7.11 step 3.2. Run for all four reasons (`receive` / `writeoff` / `count_correction` / `consume_visit` via the lock writer) so every emitter is locked. |

### §9.2 P06-G02 -- IPC rejects caller-supplied `consume_visit` reason (HIGH)

- **Source:** phase-06.md §4 frontend step 3 / §7.6 NotUserSelectable
- **Target test section:** §2.1 / §6.4
- **Category:** Missing Integration Test

§7.6 declares `consume_visit` as `NotUserSelectable` -- the `inventory_create_adjustment` IPC must reject any caller-supplied `reason='consume_visit'` regardless of role, because that reason is reserved for the lock workflow's internal writer. Existing §2.2 IPC tests cover role gating for `count_correction` and existing §2.1 tests cover the lock workflow's positive path, but no test pins the IPC-layer rejection of the user-selectable bypass attempt. A regression would let a compromised client emit fake consume rows that look like visit consumption.

| Scenario | Asserts |
|-|-|
| `create_adjustment_rejects_caller_supplied_consume_visit_reason_for_every_role` | Submit `inventory_create_adjustment { reason: 'consume_visit', qty: 2, visit_id: <real visit> }` as receptionist, accountant, and superadmin in turn. Each call returns `AppError::Validation` (or the typed `NotUserSelectable` variant, surfaced as `Validation` to the frontend per §3.2's error envelope). No row created, no audit row, no outbox row, `inventory_items.quantity_on_hand` unchanged. Per §7.6 + §4 frontend permission table row 4. |

### §9.3 P06-G03 -- Server `acceptPush` positive cases for receive/writeoff (HIGH)

- **Source:** phase-06.md §7.6 server defence-in-depth positive
- **Target test section:** §2.3
- **Category:** Missing Integration Test

Existing §2.3 server tests cover the rejection branch (receptionist pushing `count_correction` -> 403) and the count_correction superadmin happy path, but never explicitly assert that `receive` and `writeoff` rows authored by ANY role (including the accountant role, which cannot author them at the IPC layer but COULD if a compromised client bypassed local gating) are accepted by the server. The server's role check is reason-scoped (`count_correction` only); a regression that broadened the check to all reasons would silently break receptionist receives without any test catching it.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_accepts_receive_from_every_role` | Push `{ reason: 'receive', delta: 5 }` once per actor role (receptionist, accountant, superadmin). All three succeed with 200; row persisted; `quantityOnHand` recomputed. Server's defence-in-depth role check is reason-scoped to `count_correction` only -- it does NOT reject other reasons by role. |
| `POST /sync/push` | `push_accepts_writeoff_from_every_role` | Same matrix for `{ reason: 'writeoff', delta: -3 }`. All three roles' pushes succeed. Per §7.6. |

### §9.4 P06-G04 -- Postgres CHECK migration raw-SQL replay (MEDIUM)

- **Source:** phase-06.md §7.14 Postgres CHECK migration
- **Target test section:** §6.8 / §2.3
- **Category:** Missing Integration Test

§7.14 introduces a raw-SQL Prisma migration `<ts>_inventory_adjustments_delta_sign/migration.sql` that adds the per-reason CHECK to the Postgres `inventory_adjustments` table. Existing server tests trip the CHECK via end-to-end pushes (TypeBox catches most before it fires), but no test replays the raw migration file against a populated Postgres database to assert that (a) the migration is idempotent, (b) existing seed rows pass the new CHECK, and (c) every invalid `(reason, delta)` combo is rejected by the constraint when bypassing the application layer.

| Scenario | Asserts |
|-|-|
| `pg_migration_delta_sign_idempotent_on_populated_db` | Boot a Postgres test DB; apply all migrations through phase-05; load the `clinical-day.sql` server-side adjustment seed; replay `<ts>_inventory_adjustments_delta_sign/migration.sql` twice. Both replays succeed; no existing seed row violates the CHECK; CHECK appears in `information_schema.check_constraints` exactly once. Per §7.14. |
| `pg_check_blocks_all_invalid_reason_delta_combos_via_raw_sql` | Bypass Prisma and TypeBox: open a raw `pg` client and `INSERT INTO inventory_adjustments` rows for every invalid combo from §7.1 (`{receive, 0}`, `{receive, -5}`, `{writeoff, 0}`, `{writeoff, 5}`, `{count_correction, 0}`, `{consume_visit, visit_id=NULL}`). Each INSERT raises `check_violation` (SQLSTATE 23514); no row persisted. Per §7.14 + §6.8 exhaustive CHECK enforcement. |

### §9.5 P06-G05 -- E2E status pill threshold cross on writeoff (MEDIUM)

- **Source:** phase-06.md §7.5 / §3 `<StockStatusPill>` threshold cross
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.5 declares that `<StockStatusPill>` reflects the live status of an item against its `low_stock_threshold`, and §3's frontend section commits to a `<StockStatusPill>` in both the list row and the `<ItemOverview>` badge. Existing §1.1 + §2.4 cover the value-object math and the pill renderer in isolation, but no E2E asserts the user-visible transition: writing off below the threshold flips the pill to `LOW` on both surfaces in real time (cache invalidation included). This is a MEDIUM gap because the threshold cross is the single most important inventory operational signal.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `adjust-writeoff-crosses-low-threshold-updates-pill-on-list-and-detail.e2e.ts` | Mehdi (`receptionist`) | 1) Seed Lidocaine with `quantity_on_hand=5`, `low_stock_threshold=3`. 2) Log in; navigate to `/inventory`; assert the row's `<StockStatusPill>` reads `OK`. 3) Click into `/inventory/items/<lidocaine-id>`; assert `<ItemOverview>` badge reads `OK`. 4) Navigate to `/inventory/adjust`; pick Lidocaine; reason=writeoff; qty=3; submit. 5) Return to `/inventory`. | The Lidocaine row's pill now reads `LOW` (color `--gold` per design-system §1.4). 6) Re-enter `/inventory/items/<lidocaine-id>`; the `<ItemOverview>` badge reads `LOW`. Both surfaces reflect the new state without a manual refresh (React Query invalidation per §2.4). Per §7.5 + PRD §7.3.1 boundary inclusive (`quantity == threshold` -> LOW). |

### §9.6 P06-G06 -- Sanity-cap warning UX (MEDIUM)

- **Source:** phase-06.md §7.8 sanity-cap warning UX
- **Target test section:** §6.3 / §2.4
- **Category:** Missing Edge Coverage

§7.8 says the sanity-cap warning "warns but does not block" submission of an unusually large adjustment, and §2.4 lists a `<AdjustForm>` row asserting the inline `<Alert variant="warning">` renders. Unit tests cover the boundary math (`|delta| > 1000`), and an existing §4.1 E2E asserts the row is still persisted, but no test pins the warning's user-facing affordances: the toast text content, dismissibility, and that the qty input value is RETAINED (not cleared) after the warning surfaces so the operator can review-and-resubmit without re-typing. A regression that cleared the input or made the warning non-dismissible would leak through.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<AdjustForm>` (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `sanity_cap_warning_renders_with_dismissible_alert_and_retains_qty` | Render `<AdjustForm>` with mocked IPC; pick reason=receive; type qty=1500; submit. The inline `<Alert variant="warning" role="alert">` renders with text matching i18n key `inventory.adjust.warning.unusually_large` ("Unusually large adjustment - confirm" in en; mirrored Arabic copy in ar). The alert has a dismiss button (`data-testid="adjust-warning-dismiss"`); clicking it hides the alert WITHOUT clearing the qty input or resetting the form. The qty input still reads `1500` post-dismiss. The submit button is NOT disabled. Per §7.8. |
| `adjust-warning-toast-text-and-retention.e2e.ts` (§4.2 failure-path flow) | Persona Mehdi: pick reason=receive; qty=1500; submit; dismiss warning; assert qty input still reads `1500` (or its Arabic-Indic rendering when locale=ar); assert row persisted. | The warning toast is dismissible, the qty is retained, and the save is not blocked. Per §7.8. |

### §9.7 P06-G07 -- Reversal-pair snapshot (LOW)

- **Source:** phase-06.md §7.15 reversal-pair payload
- **Target test section:** §3.3 / §8
- **Category:** Missing Snapshot

§3.2 declares `InventoryAdjustmentWithMetaSchema` as adding `is_reversal: z.boolean()` to the base adjustment schema (per §7.15), and §3.3 commits to hash-locked snapshots of the canonical push/pull payloads. There is currently no committed snapshot file for the `is_reversal: true` shape of the `WithMeta` row, so a renderer-side or serializer-side change that subtly altered the meta envelope (for example, adding `reversal_of_adjustment_id` without updating the schema) would not trip the contract harness. The fix is one new snapshot fixture and one DoD checkbox.

| Snapshot file | Asserts |
|-|-|
| `expected/sync/adjustment-row-with-meta-reversal.json.sha256` | Hash of the canonicalized JSON for an `InventoryAdjustmentWithMeta` row representing a voided-visit reversal: `{ id, item_id, visit_id, reason: 'consume_visit', delta: +2 (positive offset), is_reversal: true, ...standard adjustment fields }`. Committed alongside the existing phase-05 snapshots. Per §7.15 + §3.3 snapshot rules. DoD §8 grows one row: `[ ] expected/sync/adjustment-row-with-meta-reversal.json.sha256 (NEW for this phase -- reversal-pair WithMeta row)`. |
| P3 Mariam the Superadmin (reinforcement) | -- | -- | -- | Optional, exercises count_correction + `inventory_recompute_on_hand` debug path. |

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). These are residual exposures that Pass 1 did not surface -- they live here during Pass 3 verification and merge into their target §X.Y sections at test-authoring time. The §9 additions are NOT modified by Pass 2; Pass 2 only appends.

### §10.1 P06-G08 -- SyncPullService hook registration (CRITICAL)

- **Source:** phase-06.md §7.9 pull-time quantity recompute hook
- **Target test section:** §2.1 / §3.3
- **Category:** Missing Integration Test

§7.9 commits to a `SyncEngine::on_pull_applied_inventory` callback whose IMPLEMENTATION is exercised by §2.1 (pull batch with `quantity_on_hand=999` is overwritten by local recompute) and whose COVERAGE is gated by §1.3 at >= 95% lines. Neither row asserts that the hook is actually WIRED into the phase-01 `SyncPullService` startup. A regression that left the callback as dead code (defined, exported, never registered) would pass every §2.1 behavioural test because no `on_pull_applied_inventory` fires at all and the inherited LWW path silently writes the server value -- and the test would compare local vs local, not local vs hook-driven. This is CRITICAL because PRD §6.1.12 inv 1 declares local recompute as the source-of-truth.

| Scenario | Asserts |
|-|-|
| `sync_pull_service_registers_on_pull_applied_inventory_at_construction` | Instantiate the phase-01 `SyncPullService` via its public constructor (the same one `lib.rs` uses). Inspect its registered post-apply callbacks via the test-only `registered_post_apply_hooks()` accessor (or assert by behavioural proxy: stub the callback with a tracker and drain one pull batch containing `inventory_adjustments` and `inventory_items` -- the tracker MUST record exactly one call per batch with the affected `item_ids`). Per §7.9. Cross-ref §2.1 row `pull_applied_inventory_recomputes_quantity_on_hand_from_local_adjustments` which exercises the BODY of the hook; this row pins the REGISTRATION. |

### §10.2 P06-G09 -- `NotUserSelectable` variant in shared AppError enum (HIGH)

- **Source:** phase-06.md §7.6 + §3.2 error envelope row
- **Target test section:** §3.2
- **Category:** Missing Contract Test

§7.6 introduces `AdjustmentError::NotUserSelectable` as the typed Rust variant raised when a caller submits `reason='consume_visit'` (covered behaviourally by §9.2). The §3.2 fixed error-envelope row lists this variant alongside `Forbidden`, `QtyNonPositive`, and `CountCorrectionMustBeNonZero` and says "each MUST be in the `kind` enum" of `AppErrorSchema`, but the harness currently has no row that runs the inverse contract check for this specific variant: emit a `NotUserSelectable` from Rust, serialize, and Ajv-validate against the Zod enum. A regression that added the variant to Rust but forgot to add `not_user_selectable` to the TS `kind` enum would silently slip through the existing IPC-shape diff because Zod's `z.enum([...])` only rejects unknown literals at PARSE time, and no current test parses this literal.

| Scenario | Asserts |
|-|-|
| `app_error_envelope_includes_not_user_selectable_kind_literal` | Construct `AdjustmentError::NotUserSelectable` in Rust; serialize via the shared `Serialize` impl; assert the resulting JSON is `{ kind: "not_user_selectable", message: <non-empty string> }`. Parse the same JSON through `AppErrorSchema` on the TS side; assert it succeeds (Zod's `z.enum` would reject if the literal is absent). Per §7.6 + §3.2 fixed error-envelope row. Add a parallel row in the §3.2 table covering the three other new variants (`forbidden`, `qty_non_positive`, `count_correction_must_be_non_zero`) for completeness. |

### §10.3 P06-G10 -- Server `acceptPush` rejects caller-supplied `consume_visit` without `visit_id` (HIGH)

- **Source:** phase-06.md §7.6 server defence-in-depth + §7.1 CHECK
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§9.2 covers the IPC-layer rejection of a caller-supplied `consume_visit` reason; §7.6's last sentence extends the same posture to the SERVER (`acceptPush` "rejects `count_correction` rows whose authoring user is not a superadmin"), and the per-reason CHECK in §7.1 / §7.14 expects `consume_visit` rows to always carry a real `visit_id`. The server-side mirror of §9.2 is missing: a push containing `{ reason: 'consume_visit', visit_id: null, delta: +2 }` (or with a `visit_id` that does not resolve to a real, locked visit) must be rejected at the route layer BEFORE it reaches the CHECK, because the CHECK only validates the column predicate -- it cannot validate referential integrity to `visits`. Without this row, a malicious client could ship fake consume rows that look like visit consumption to downstream reports.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_rejects_consume_visit_without_real_visit_id` | Push `{ reason: 'consume_visit', visit_id: null, delta: +1, ... }`. Response is 422 (TypeBox `additionalProperties: false` + nullable check) or 400 with `kind: 'not_user_selectable'`. No row persisted; `inventory_items.quantityOnHand` unchanged. Repeat with `visit_id: '00000000-0000-0000-0000-000000000000'` (non-existent UUID) -- response is 404 / 422 with `kind: 'visit_not_found'`. Repeat with a `visit_id` referencing a `draft` (not `locked`) visit -- rejected per phase-05 §7.30 lock-state contract. Per §7.6 server defence-in-depth + §7.14 CHECK. Mirror of §9.2 at the server layer. |

### §10.4 P06-G11 -- Audit-first ordering on adjustment `create` row (HIGH)

- **Source:** phase-06.md §7.11 audit-first ordering step 3.1
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§9.1 locks the audit payload shape for the `inventory_items` UPDATE row (step 3.2 of §7.11). §7.11 also mandates a SEPARATE audit row in step 3.1 -- a `create` action on the `inventory_adjustments` entity itself, written BEFORE step 3.2 and BEFORE the actual `INSERT inventory_adjustments` row in step 3.3. The existing §2.1 row asserts the inventory_items update audit appears first vs business+outbox, but no test asserts the inventory_adjustments `create` audit row exists at all, nor that it precedes the items-update audit row in `audit_log.created_at` order. A writer that collapsed steps 3.1 and 3.2 into a single audit row (or skipped 3.1 entirely) would pass every existing test.

| Scenario | Asserts |
|-|-|
| `create_adjustment_writes_separate_audit_row_for_adjustment_create_first` | Submit `inventory_create_adjustment { reason: 'receive', qty: 5 }`. Query `audit_log` filtered to this tx; assert exactly TWO rows: (1) `action='create' entity='inventory_adjustments' entity_id=<new adjustment id>` with `delta` carrying the full new-row snapshot (`{ after: { id, item_id, reason, delta, ... } }`); (2) `action='update' entity='inventory_items' entity_id=<item id>` per §9.1. Assert `row1.created_at <= row2.created_at` AND `row1.id < row2.id` (monotonic insertion order). Per §7.11 steps 3.1 and 3.2. Run for all four reasons so every writer is locked, including the `consume_visit` path driven by the lock workflow. |

### §10.5 P06-G12 -- `<InventoryItemsTable>` search debounce 250ms (MEDIUM)

- **Source:** phase-06.md §7.5 filter row debounced 250ms, min 2 chars
- **Target test section:** §2.4
- **Category:** Missing Unit Test

§7.5 commits to the inventory list search input being "debounced 250ms, min 2 chars - reuses `src/lib/search.ts` from phase-03 §7.14". Existing §2.4 rows cover the 2-character minimum (queries shorter than 2 chars do not fire IPC) but no row asserts the debounce timing -- a regression that dropped the wrapper and called IPC on every keystroke would still pass the min-chars test but would saturate the IPC bus during typing. The fix is one component test using `vi.useFakeTimers()` to advance the clock and count IPC calls.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<InventoryItemsTable>` (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `inventory_search_debounces_at_250ms_and_coalesces_typing` | Render `<InventoryItemsTable>` with mocked `inventory_list_items` IPC. With `vi.useFakeTimers()`: type `"lid"` letter-by-letter at 50ms intervals (3 keystrokes, total 100ms). Advance timers to 249ms -- assert IPC NOT yet called (or called only once with no query). Advance to 250ms -- assert IPC called exactly once with `query: "lid"`. Type `"o"` (now 4 chars total); advance another 250ms; assert IPC called once more with `query: "lido"`. Total IPC calls across both bursts: exactly 2 (one per debounce window), not 4 (one per keystroke). Per §7.5. |

### §10.6 P06-G13 -- Soft-deleted reversal sibling excluded from `quantity_on_hand` SUM (MEDIUM)

- **Source:** phase-06.md §7.2 SUM with tombstone + §6.8 + §7.15 reversal pairs
- **Target test section:** §2.1 / §6.8

- **Category:** Missing Edge Coverage

§7.2 says the canonical recompute is `SELECT COALESCE(SUM(delta), 0) FROM inventory_adjustments WHERE item_id = ? AND deleted_at IS NULL`, and §6.8 covers generic tombstone exclusion. §7.15 introduces reversal pairs (a voided visit emits a +N row offsetting the original -N row). The unexplored edge: soft-deleting ONE SIDE of a reversal pair (e.g. an operator deletes the original consume row but the reversal row survives) must STILL produce a correct on-hand. The phase has no test that builds a reversal pair, soft-deletes the reversal sibling, and asserts `quantity_on_hand` recomputes correctly excluding only the tombstoned row -- without this, a regression that mis-joined the tombstone filter (e.g. on the reversal sibling's `reversal_of_adjustment_id` rather than its own `deleted_at`) would slip past §6.8's generic test.

| Scenario | Asserts |
|-|-|
| `recompute_on_hand_excludes_soft_deleted_reversal_sibling` | Seed item I with `quantity_on_hand=10`. Submit a `consume_visit` for I with delta=-3 (resulting `quantity_on_hand=7`). Void the visit (per phase-05 §7.24) which emits the reversal pair: a +3 row with `is_reversal=true, reversal_of_adjustment_id=<original id>`. Assert `quantity_on_hand=10` after reversal. Soft-delete the reversal row only (UPDATE `inventory_adjustments` SET `deleted_at=now()` WHERE `id=<reversal id>`). Trigger `inventory_recompute_on_hand` for I. Assert `quantity_on_hand=7` (the original -3 still counts; the +3 reversal is excluded). Then soft-delete the ORIGINAL row too; recompute; assert `quantity_on_hand=10` (both tombstoned, SUM is 0, plus the never-tombstoned baseline). Per §7.2 + §7.15. |

### §10.7 P06-G14 -- Sync-server presentation coverage gate scope (MEDIUM)

- **Source:** phase-06-test.md §1.3 row 8 + phase-06.md §2.3
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§1.3 declares a >= 85% lines gate for `sync-server/src/app/domains/inventory/presentation/**` with the parenthetical "no new routes -- only the push/pull handlers' inventory-adjustment branches". phase-06.md §2.3 confirms no new routes are added in this phase. With zero new presentation-layer code, the >= 85% gate is either unsatisfiable (no new lines to cover, c8 may report 100% trivially or 0% with NaN denominator) or it silently passes by inheriting phase-05's coverage. The gate must either be DROPPED for this phase or SCOPED to the specific push/pull branches that handle inventory adjustments (e.g. the `case 'inventory_adjustments':` arm of the push dispatcher).

| Scenario | Asserts |
|-|-|
| `sync_server_presentation_coverage_gate_scoped_to_inventory_branches_or_dropped` | The §1.3 row for `sync-server/src/app/domains/inventory/presentation/**` is REWRITTEN to either: (a) `Drop this row -- phase-06 adds no new routes; presentation gate inherited from phase-05` with a §8 sign-off note; OR (b) narrow the glob to `sync-server/src/app/domains/inventory/presentation/push-dispatch/inventory_adjustments.ts` (the specific branch) with the >= 85% threshold and a c8 invocation that targets that single file. The phase-06-test.md DoD §8 grows one row: `[ ] §1.3 row 8 (sync-server presentation) resolved: either dropped or scoped`. Per §1.3 + phase-06 §2.3 "What this phase does NOT touch" (no new routes). |

### §10.8 P06-G15 -- `last_adjusted_at` nullable round-trip (MEDIUM)

- **Source:** phase-06.md §3 Tauri response shape + §3.2 IPC contract
- **Target test section:** §2.1 / §2.2
- **Category:** Missing Integration Test

§3.2 declares `InventoryItemWithStatusSchema` with `last_adjusted_at: z.string().datetime().nullable()`. For an item with ZERO live (non-tombstoned) adjustments, the Rust side must emit `null`, not an empty string, not the epoch, not `"0001-01-01T00:00:00Z"`. The current §2.1 / §2.2 tests assert the field exists on items with adjustments but no test exercises the empty-history case, so a regression that defaulted the field (`unwrap_or(DateTime::default())`) would pass schema validation (the default is a valid datetime string) and silently break the "newly seeded item with no history" UI signal.

| Scenario | Asserts |
|-|-|
| `list_items_returns_null_last_adjusted_at_for_items_with_zero_live_adjustments` | Seed item I with `quantity_on_hand=0` and ZERO adjustment rows. Invoke `inventory_list_items`. Assert the row for I has `last_adjusted_at: null` (JSON null, not the string `"null"`, not omitted, not epoch). TS-side parse through `InventoryItemWithStatusSchema`; assert `result.last_adjusted_at === null`. Seed a second item J with one ACTIVE adjustment plus one tombstoned (`deleted_at IS NOT NULL`) adjustment. The tombstoned one is older; assert `last_adjusted_at` for J reflects the ACTIVE adjustment's timestamp, not the tombstoned one. Per §3 Tauri response shape + §3.2 nullable contract. |

### §10.9 P06-G16 -- Active/Inactive item opacity convention (LOW)

- **Source:** phase-06.md §7.5 active toggle + design-system.md §1.3 ink-4
- **Target test section:** §2.4
- **Category:** Missing Unit Test

§7.5 declares an `Active only | All` toggle and the design system says inactive rows are rendered "muted style (`opacity: 0.6`)" so operators can see them when `All` is selected without confusing them for active items. The current §2.4 tests cover the toggle behaviour (the `is_active=false` rows appear when `All` is chosen) but no row asserts the visual convention -- only the §5.1 manual step describes it. A regression that dropped the opacity rule or applied it inversely (muting active rows) would only be caught by a human eye review. The fix is one component test asserting the `data-inactive="true"` row carries `opacity: 0.6` (computed style or class assertion).

| Hook / Component | Test | Asserts |
|-|-|-|
| `<InventoryItemsTable>` (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `inactive_items_render_with_muted_opacity_when_all_filter_selected` | Render `<InventoryItemsTable>` seeded with two items: active item A and `is_active=false` item B. Toggle filter to `All`. Locate B's row by `data-testid="inventory-row-<b-id>"`; assert `getComputedStyle(row).opacity === '0.6'` (or assert the row carries the class declared by the design-system token -- whichever the implementation chose, per `.claude/rules/design-system.md` §1.3 `--ink-4`). A's row computed opacity is `1`. Toggle back to `Active only`; B's row is removed from the DOM (not just hidden). Per §7.5 + design-system §1.3. |

### §10.10 P06-G17 -- Sanity-cap warning accessibility wiring (LOW)

- **Source:** phase-06.md §7.8 + §9.6 sanity-cap warning UX
- **Target test section:** §4.1 / §4.2
- **Category:** Missing E2E Scenario

§9.6 already adds a component test (`sanity_cap_warning_renders_with_dismissible_alert_and_retains_qty`) and an E2E (`adjust-warning-toast-text-and-retention.e2e.ts`) for the warning's text and dismissibility. Neither asserts the WAI-ARIA wiring: the inline alert must carry `role="alert"` (the §9.6 component test asserts the JSX attribute via testid, but does NOT assert the rendered ARIA wiring fires the screen-reader live-region announcement). Phase-06.md §7.8 implies the affordance is for the operator at the till -- which in a clinical setting includes operators using assistive tech. A regression that swapped `role="alert"` for `role="status"` or removed `aria-live` would silently degrade accessibility without breaking any current test.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `adjust-warning-alert-role-announces-to-screen-reader.e2e.ts` | Mehdi (`receptionist`) | 1) Log in; navigate to `/inventory/adjust`. 2) Pick reason=receive. 3) Subscribe to the WebdriverIO accessibility-events stream (via the chromedriver `accessibility` domain) BEFORE the warning fires. 4) Type qty=1500; submit. | The accessibility event log records exactly one `live-region update` with role=`alert` and content matching the i18n key `inventory.adjust.warning.unusually_large` (en) or its Arabic mirror (when `LOCALE=ar`). The element rendered in the DOM has `role="alert"`, `aria-live="assertive"` (or implicit via `role="alert"`), and `aria-atomic="true"`. The dismiss button has `aria-label` matching `common.dismiss`. Per §7.8 + WCAG 2.2 SC 4.1.3 (status messages). Cross-ref §9.6 for the text-content assertions which this row does NOT duplicate -- it adds ONLY the accessibility wiring. |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 4 Phase-06 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P06-G18 through P06-G21). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these rows close the server-side parity gaps and the negative-on-hand E2E path the build spec explicitly calls out.

### §11.1 P06-G18 -- Server acceptPush writes TWO audit rows in audit-first order (HIGH)

- **Source:** phase-06.md §7.11 step 3.1 + "Server `acceptPush` follows the same order" + §7.3.
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§10.4 (P06-G11) pinned the LOCAL audit-first invariant; the server-side parallel is missing. A server that emits only one audit row (or writes it AFTER the upsert) silently slips past existing §2.3.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `server_acceptPush_writes_create_audit_then_update_audit_then_upserts_in_one_tx` | Push an `inventory_adjustment` payload. On commit: query `audit_log` ordered by `at ASC`; assert the two audit rows for THIS push are: (1) `action='create', entity='inventory_adjustments', entity_id=<adj_id>` and (2) `action='update', entity='inventory_items', entity_id=<item_id>`. Both rows' `at` values are <= the `inventory_adjustments.created_at` AND `inventory_items.updated_at` (proves audit-first ordering on server). Inject an upsert failure mid-tx; assert NEITHER audit row persists (atomicity). Per §7.11 + §7.3. |

### §11.2 P06-G19 -- Server Postgres partial indexes mirror (HIGH)

- **Source:** phase-06.md §7.10 -- "Mirror on server with raw SQL CREATE INDEX statements via Prisma raw migration".
- **Target test section:** §2.3 / §6.8
- **Category:** Missing Setup

The §9.4 sibling (P06-G04) covers the CHECK migration only; the server-side partial indexes have no coverage.

| Scenario | Asserts |
|-|-|
| `inventory_items_partial_indexes_mirror_lands_on_postgres` | After `prisma migrate deploy`: `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'InventoryItem'`. Assert the result contains `inventory_items_low_stock` (partial WHERE `quantity_on_hand <= low_stock_threshold`) and `inventory_items_negative` (partial WHERE `quantity_on_hand < 0`). Run `EXPLAIN ANALYZE SELECT * FROM "InventoryItem" WHERE quantity_on_hand <= low_stock_threshold AND deleted_at IS NULL LIMIT 50` -- assert `Index Scan using inventory_items_low_stock`. Mirror EXPLAIN for the negative-stock query. Re-apply the migration (idempotency replay); assert no error. Per §7.10 last line. |

### §11.3 P06-G20 -- Negative on-hand NEG pill via lock over-consumption (MEDIUM)

- **Source:** phase-06.md §6 verification step 8 -- "Negative on-hand: simulate over-consumption via lock; assert UI surfaces NEG pill but does not block".
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§1.1 covers the math; §2.4 covers the pill rendering. No E2E covers the full lock-driven over-consumption path.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `over-consumption-via-lock-surfaces-neg-pill-without-blocking.e2e.ts` | Mehdi (`receptionist`) -> superadmin verification | 1) Seed an inventory item with `quantity_on_hand=5`. 2) Create a draft visit consuming 8 units of this item (via consumption map). 3) Lock the visit. 4) Verify the lock workflow SUCCEEDED (no blocker). 5) Navigate to `/inventory`; locate the item row. 6) Open `/inventory/<item_id>` overview. | (a) Lock succeeded (no `LockBlocker::InsufficientStock` -- soft warning only); (b) `<StockStatusPill>` on the list row reads `NEG` with `--crimson` color; (c) `<ItemOverview>` mirrors the NEG status with the on-hand value `-3` rendered with tabular numerals; (d) attempting a follow-up `inventory::create_adjustment { delta: -1 }` is NOT blocked (the warning fires but the operation succeeds); (e) the pill updates live after the follow-up adjustment. Per §6 verification step 8. |

### §11.4 P06-G21 -- inventory.columns.pending_sync i18n key assertion (MEDIUM)

- **Source:** phase-06.md §7.12 -- explicit i18n key `inventory.columns.pending_sync`.
- **Target test section:** §2.4
- **Category:** Missing Unit Test

A regression renaming the key or hardcoding the column header would slip past §6.2's generic grep.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<InventoryItemsTable>` (`describe.each([['ltr'],['rtl']])`) | `pending_sync_column_header_uses_inventory_columns_pending_sync_i18n_key` | Render the table with `<I18nextProvider>` instrumented to record every `t()` lookup. Locate the column header cell for the Pending-sync column (via `data-testid="col-header-pending-sync"`). Assert the recorded lookup list contains exactly `inventory.columns.pending_sync` (NOT `inventory.columns.dirty`, NOT a hardcoded string). Verify the rendered header text matches the resolved value in both `en` and `ar` locale files. Per §7.12. |

---

## §12 Gap Analysis Pass 4 Additions

This row encodes the single Phase-06 gap surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P06-G22). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11.

### §12.1 P06-G22 -- Server-side audit delta payload shape parity (HIGH)

- **Source:** phase-06.md §7.11 step 3.2 + §7.3 + "Server `acceptPush` follows the same order" -- server audit row carries `delta: { before, after, reason }` payload shape matching the local writer (§9.1 P06-G01).
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§11.1 (P06-G18) pinned the audit-first ordering and action/entity/entity_id on the server side; §9.1 (P06-G01) pinned the `delta` JSON shape on the local side. The server-side `delta` payload shape is the unverified leaf.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `server_acceptPush_audit_delta_payload_carries_before_after_reason_shape` | Push an `inventory_adjustment` payload. Query the server `audit_log` row written for the `inventory_items` update. Assert `delta` JSON has EXACTLY three keys at the top level: `before` (the on-hand value pre-update), `after` (post-update), `reason` (one of `receive`/`writeoff`/`count_correction`/`consume_visit`). Both numeric values are integers; `reason` is the documented variant. Parametrize across all 4 reasons; assert all 4 land with the documented shape. A server that wrote `delta: {}` or `delta: { qty: 5 }` passes existing P06-G18 ordering tests but fails this. Per §7.3 + §7.11. |

