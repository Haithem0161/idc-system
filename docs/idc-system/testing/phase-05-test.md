# Phase 05: Reception & Visit Lock -- Test Plan

**Proves:** A receptionist can create a draft visit per check type, autocomplete a patient (FTS5), pick a doctor / subtype / dye / report, watch the running total update from the same `money_math` the Rust lock workflow uses, and -- when an operator is clocked in and qualified -- lock the visit in a single SQLite transaction that snapshots money + names, appends `consume_visit` inventory adjustments, recomputes on-hand counts, writes audit rows audit-first, renders an A5 PDF + thermal receipt, and enqueues outbox rows. Superadmin can void a locked visit with offsetting adjustments. Three new entities (`patients`, `visits`, `inventory_adjustments`) flow through `/sync/push` with the declared per-entity policies (`last-write-wins`, `manual`, `additive-only`).

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, audit-first `with_audit`, outbox, envelope versioning), Phase 02 test (auth + roles, `<RequireRole>`, settings cache), Phase 03 test (catalog FTS, effective-price resolver, operator specialties, pricing-changed event), Phase 04 test (operator shifts -- visit lock reads `has_open_for_operator` and qualified operators must be clocked in).
**Test Data:**
- Factories: `src-tauri/tests/support/factories.rs::{make_patient, make_visit_draft, make_visit_locked, make_visit_voided, make_consume_adjustment, make_doctor_pricing, make_consumption_map}` (extended in this phase); `src/test-utils/factories.ts::{makePatient, makeVisitDraft, makeVisitLocked, makeInventoryAdjustment, makeMoneyInputs}`; `sync-server/test/support/factories.ts::{makePatient, makeVisitPushPayload, makeInventoryAdjustmentPushPayload}`.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- already contains 200 patients (FTS populated), 30 visits in mixed states, full catalog + consumption maps, 2 closed shifts. The phase-05 plan consumes the fixture for scale/persona runs; it does NOT mutate the fixture schema.
- Synthetic scale fixture: `fixtures/scale/12-months.sql` (referenced by P5 Year-End Audit) -- 12 months of synthetic visits for scale drills; owned by `performance-soak.md` but loaded by §6.6 perf assertions here.
**Tool prerequisites:**
- Rust: `cargo`, `cargo-llvm-cov` (installed when phase-04-test executes), `wiremock` (NEW for offline-toggled sync server scenarios in §6.3 -- `cargo add --dev wiremock`).
- Frontend: `vitest` + RTL + jsdom + `@vitest/coverage-v8` (installed in phase-04-test), `msw@2` (NEW: for IPC mocking parity with the production transport in component tests -- `pnpm add -D msw@2`).
- E2E: `webdriverio` + `tauri-driver` (installed in phase-04-test). Multi-instance via `MULTI_DEVICE=true` per `personas.md`.
- Contract: `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser` (installed in phase-04-test).
- PDF / thermal snapshot helpers: `pdfium-render` or `pdf-extract` (NEW: text-layer extraction for the A5 PDF hash comparison per `.claude/rules/testing.md` §10 -- `cargo add --dev pdf-extract`). Thermal text is byte-exact, no helper needed.
- Sync server: `node --test` + `c8` already present.

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`.
- 3xN conflict matrix exhaustively -- the `manual` cell for `visits`, `last-write-wins` for `patients`, `additive-only` for `inventory_adjustments` are tested here; the cross-product against other entities is in `sync-conflicts.md`.
- Page-by-page i18n / RTL snapshots for `/reception/*` -- this plan asserts core invariants per `.claude/rules/design-system.md` §12; the full visual page-by-page sweep is in `i18n-rtl.md`.
- 8-hour soak + 12-month scale runs aggregated -- owned by `performance-soak.md`; §6.6 here references the perf SLOs the soak harness aggregates.

**Cross-phase commands:**
- `shifts_lines_run_today` -- registered in phase-04 `lib.rs` (per `lib.rs` line 105) but **logically owned by phase-05 per phase-04 §7.7**; tested here in §2.2 as a first-class command. Phase-04 test marked this as a `(cross-ref)` row pointing to this plan.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`money_math` (`src-tauri/src/domains/visits/money_math.rs`)** -- the single most important pure module in the phase. The frontend's running total uses a TS port of the same algorithm; the Rust port is canonical.

| Module | Test | Asserts |
|-|-|-|
| `money_math::compute` | `flat_pricing_check_with_no_subtype_no_doctor` | Base `price_iqd` from `check_types.price_iqd`; doctor cut 0; operator cut = `operator.base_cut_per_check_iqd`; `internal_pct = 100`; total = price (no dye, no report). |
| `money_math::compute` | `subtype_price_overrides_check_when_has_subtypes` | `check_types.has_subtypes=1` + `check_subtypes.price_iqd=X` -> `price_snapshot = X`. |
| `money_math::compute` | `doctor_override_replaces_internal_pct` | `doctor_check_pricing` row present -> `doctor_cut = pricing.cut_value` (with `cut_kind=flat` or `percentage`); `internal_pct = NULL`; total unchanged. |
| `money_math::compute` | `house_doctor_keeps_internal_pct_100` | `doctor_id = NULL` -> `internal_pct = 100`; `doctor_cut = 0`. |
| `money_math::compute` | `dye_cost_added_only_when_dye_true_and_supported` | `dye=true` + `dye_supported=1` -> `dye_cost = settings.dye_cost_iqd`; `dye=true` + `dye_supported=0` -> `Err(MoneyError::DyeNotSupported)`; `dye=false` -> `dye_cost = 0`. |
| `money_math::compute` | `report_cost_added_only_when_report_true_and_supported` | Same matrix for report cost. |
| `money_math::compute` | `total_equals_sum_of_price_and_dye_and_report` | Per phase-05 §7.2: `total_amount_iqd = price + dye_cost + report_cost` -- assertion ranges over a property-test of 50 random `(check_type, dye, report)` combos. |
| `money_math::compute` | `operator_cut_uses_operator_base_when_check_specific_override_absent` | When `operators.base_cut_per_check_iqd=X`, no per-check override -> `operator_cut = X`. |
| `money_math::compute` | `pct_kind_doctor_pricing_rounds_consistently` | `cut_kind=percentage`, `cut_value=25` (i.e. 25%) of `price=1_000_037` -> deterministic integer-rounded result; assert no float drift across 100 runs. |
| `money_math::compute` | `rejects_negative_inputs` | Negative `price_iqd`, negative `cut_value`, negative `dye_cost_iqd` -> `Err(MoneyError::Negative)` with the offending field name. |
| `money_math::compute` | `frontend_ts_port_matches_rust_for_canonical_inputs` | Read 30 canonical `(inputs, expected)` rows from `test-data/money_math/canonical.json`; assert each row produces the same `VisitSnapshots`. Same fixture is consumed by the TS port in §1.2. |

**`operator_eligibility` (`src-tauri/src/domains/visits/operator_eligibility.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `operator_eligibility::qualified` | `empty_when_no_open_shifts` | No shifts open -> returns `[]`. |
| `operator_eligibility::qualified` | `requires_specialty_intersection_with_check_type` | Operator A open + specialty matches; Operator B open + no matching specialty -> returns `[A]`. |
| `operator_eligibility::qualified` | `excludes_soft_deleted_operators` | Open shift but operator `deleted_at IS NOT NULL` -> excluded. |
| `operator_eligibility::qualified` | `excludes_inactive_operators` | `is_active=0` -> excluded. |
| `operator_eligibility::qualified` | `dye_only_specialty_filter` | `operator_specialties.on_dye_only=1` -> included only when `visit.dye=1`. |
| `operator_eligibility::qualified` | `tenant_scoped` | Operator from another `entity_id` -> excluded. |

**`Visit` entity (`src-tauri/src/domains/visits/domain/entities/visit.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `Visit::create_draft` | `produces_draft_with_uuid_v7_and_version_0_dirty_1` | Defaults. |
| `Visit::create_draft` | `requires_subtype_when_check_has_subtypes` | `has_subtypes=1` + `check_subtype_id=None` -> `Err(VisitError::SubtypeRequired)`. |
| `Visit::create_draft` | `rejects_subtype_when_check_lacks_subtypes` | `has_subtypes=0` + `check_subtype_id=Some(_)` -> `Err(VisitError::SubtypeNotApplicable)`. |
| `Visit::create_draft` | `rejects_dye_when_unsupported` | `check_types.dye_supported=0` + `dye=true` -> `Err`. |
| `Visit::create_draft` | `rejects_report_when_unsupported` | Mirror. |
| `Visit::edit_draft` | `accepts_field_patches_only_when_draft` | Status != Draft -> `Err(VisitError::IllegalTransition { from: Locked, to: Draft })`. |
| `Visit::edit_draft` | `bumps_version_and_updated_at_only_when_fields_changed` | No-op patch leaves `version` unchanged. |
| `Visit::lock` | `produces_locked_with_all_seven_snapshots_non_null` | Per phase-05 §7.1 CHECK invariants. |
| `Visit::lock` | `preserves_created_at` | Per §7.39: `created_at` is invariant across lock. |
| `Visit::lock` | `name_snapshots_required_when_locking` | Per §7.17 + §7.53 CHECK extension: `patient_name_snapshot`, `check_type_name_ar_snapshot`, `operator_name_snapshot` all non-null; `doctor_name_snapshot` iff `doctor_id IS NOT NULL`; same for subtype. |
| `Visit::lock` | `rejects_when_already_locked` | Status=Locked input -> `Err(VisitError::IllegalTransition { from: Locked, to: Locked })`. |
| `Visit::void` | `rejects_when_not_locked` | Per §7.19. Draft -> `Err(VoidError::NotLocked { current: Draft })`. Voided -> `Err(VoidError::NotLocked { current: Voided })`. |
| `Visit::void` | `preserves_created_at_and_locked_at` | Per §7.39. |
| `Visit::void` | `rejects_reason_below_5_chars` | Per §7.14: `reason.trim().chars().count() < 5` -> `Err(VoidError::ReasonTooShort)`. UTF-8 grapheme count, not byte count. |
| `Visit::assert_transition` | `legal_set_matches_phase_05_§7_32` | Exactly 3 legal transitions: `(Draft,Locked)`, `(Draft,Draft)` self-edit, `(Locked,Voided)`. All other pairs return `Err(IllegalTransition)`. |

**`Patient` entity**

| Module | Test | Asserts |
|-|-|-|
| `Patient::try_new` | `rejects_empty_or_whitespace_only_after_trim` | Per §7.9: `Patient::try_new("")` and `Patient::try_new("   ")` -> `Err(PatientError::NameEmpty)`. |
| `Patient::try_new` | `trims_leading_and_trailing_whitespace` | `" Layla "` -> `name == "Layla"`. |
| `Patient::try_new` | `accepts_arabic_and_mixed_scripts` | `"Layla هاشم"` accepted; bidi marks preserved byte-for-byte. |

**`InventoryAdjustment` entity**

| Module | Test | Asserts |
|-|-|-|
| `InventoryAdjustment::new_consume` | `requires_visit_id_when_reason_consume_visit` | Per §1 CHECK constraint mirror. |
| `InventoryAdjustment::new_consume` | `delta_sign_matches_consume_negative` | Consume rows have `delta < 0`. |
| `InventoryAdjustment::new_offset` | `delta_sign_is_positive_and_matches_visit_id_of_origin` | Void offset writer (§7.15). |
| `InventoryAdjustment` | `is_append_only_at_domain_layer` | No `edit` method exists; mutators are constructors only. |

**`VisitService` (`src-tauri/src/domains/visits/service/visit_service.rs`)** (pure helpers only -- I/O goes to §2.1)

| Module | Test | Asserts |
|-|-|-|
| `VisitService::compute_void_offsets` | `produces_one_offset_per_consume_row_with_flipped_sign` | Given 3 consume rows with deltas `[-2, -5, -1]`, returns 3 offset rows with deltas `[+2, +5, +1]`, same `item_id`, same `visit_id`. |
| `VisitService::compute_void_offsets` | `ignores_already_offset_pairs` | If a consume row already has a matching offset (idempotency on a partial-failed prior void), no duplicate offset is emitted. |
| `VisitService::map_lock_blocker` | `every_LockBlocker_variant_maps_to_a_unique_i18n_key` | Per §7.38: 7 variants -> 7 distinct `errors:operator.ineligible.*` / `errors:visit.*` keys. |

### §1.2 TS pure functions / value objects (Vitest, no IPC, no React)

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/patient.ts` | `PatientSchema_parses_minimal_row` | Round-trip. |
| `src/lib/schemas/patient.ts` | `PatientCreateSchema_rejects_empty_name_after_trim` | Per §7.9. |
| `src/lib/schemas/visit.ts` | `VisitSchema_status_locked_requires_all_snapshots_non_null` | Per §7.1: parsing a `status=locked` row with any snapshot field `null` fails on that field's path. |
| `src/lib/schemas/visit.ts` | `VisitSchema_internal_pct_iff_doctor_id_null` | Per §7.1 iff: `doctor_id` null -> `internal_pct` non-null; `doctor_id` non-null -> `internal_pct` null. |
| `src/lib/schemas/visit.ts` | `VisitDraftSchema_rejects_void_or_lock_only_fields` | A draft payload carrying `locked_at` / `voided_at` -> Zod error. |
| `src/lib/schemas/visit.ts` | `VisitLockInputSchema_rejects_empty_operator_id` | `operator_id` required + UUID. |
| `src/lib/schemas/visit.ts` | `VisitVoidInputSchema_rejects_reason_below_5_grapheme_chars` | Per §7.14: Unicode grapheme count, not `.length`. |
| `src/lib/schemas/visit.ts` | `VisitSchema_total_equals_sum_invariant_via_refine` | Per §7.2: `total_amount_iqd_snapshot = price + dye + report` enforced by `.refine`. |
| `src/lib/money-math.ts` (TS port) | `matches_rust_canonical_for_30_inputs` | Reads `test-data/money_math/canonical.json` (same file the Rust test reads, §1.1). Every row's TS port output equals the row's `expected` block bit-for-bit. This is the parity gate: drift in either port breaks both tests. |
| `src/lib/money-math.ts` | `pct_kind_rounds_identically_to_rust` | Same 100-run determinism test as the Rust version. Asserts `Math.floor(price * pct / 100)` (or whichever algorithm is canonical) matches Rust integer rounding. |
| `src/features/visits/format.ts` | `format_visit_total_uses_arabic_digits_when_setting_true` | `arabic_numerals=true` + `1234` -> `"١٢٣٤"` IQD label; `arabic_numerals=false` -> `"1234"`. |
| `src/features/visits/queries.ts` | `visit_query_keys_segment_by_check_type_id` | `['visits','byCheck', '<uuid>', 'today']` distinct from `['visits','byCheck', '<other-uuid>', 'today']`. |
| `src/stores/draft-visit-store.ts` | `persists_and_restores_per_workspace_slug_and_draftId` | Stub `localStorage`; write a draft; reload; read back. |
| `src/stores/draft-visit-store.ts` | `discards_keyed_entry_when_visit_id_no_longer_draft` | Lifecycle hook clears the entry after `visits::lock` or `visits::discard` settles. |
| `src/features/visits/lock-button.ts` (helper) | `disables_when_lock_dryrun_returns_non_empty_blockers` | Pure logic; given a `LockBlocker[]`, return `{ disabled: true, key: blockers[0].i18nKey }`. |
| `src/features/receipts/format-thermal-text.ts` | `wraps_to_settings_thermal_width_32_or_48` | 32-col + 48-col fixtures produce expected byte sequences. Hash-stable across runs. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/visits/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::visits::domain` |
| `src-tauri/src/domains/visits/service/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::visits::service` |
| `src-tauri/src/domains/visits/money_math.rs` | >= 95% lines (this is the highest-risk pure module in the phase) | `cargo llvm-cov --lib --fail-under-lines 95 -- domains::visits::money_math` |
| `src-tauri/src/domains/visits/operator_eligibility.rs` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::visits::operator_eligibility` |
| `src-tauri/src/domains/visits/infrastructure/**` | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::visits::infrastructure` |
| `src-tauri/src/domains/patients/domain/**` + `domains/patients/service/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::patients` |
| `src-tauri/src/domains/patients/infrastructure/**` | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::patients::infrastructure` |
| `src-tauri/src/domains/receipts/**` | >= 85% lines (receipts is mostly templating; the lock-time emission path is critical) | `cargo llvm-cov --lib --fail-under-lines 85 -- domains::receipts` |
| `src/features/visits/**`, `src/features/receipts/**`, `src/lib/schemas/{visit,patient,inventory}.ts`, `src/lib/money-math.ts`, `src/stores/draft-visit-store.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/visits/**,src/features/receipts/**,src/lib/money-math.ts,src/lib/schemas/{visit,patient,inventory}.ts,src/stores/draft-visit-store.ts"` |
| `src/pages/reception/**`, `src/components/reception/**`, `src/components/receipts/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/reception/**,src/components/reception/**,src/components/receipts/**"` |
| `sync-server/src/app/domains/{patients,visits,inventory}/domain/**` | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/sync/conflict/visit*.ts` (the manual-policy implementation for visits) | >= 95% lines (conflict-policy code is critical; the `additive-only` and `LWW` paths are simpler) | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/visits_phase05.rs` (extend; do not duplicate the 7 existing tests).
- Auxiliary file: `src-tauri/tests/patients_phase05.rs` (NEW -- patients warrant their own integration file for FTS5 + LWW push semantics).
- Auxiliary file: `src-tauri/tests/inventory_adjustments_phase05.rs` (NEW -- the immutability trigger and additive policy deserve isolated coverage).

Existing scenarios at HEAD (do not duplicate):
- `create_draft_and_lock_produces_receipt_and_consumption`
- `lock_rejected_when_no_qualified_operator_on_shift`
- `void_offsets_inventory_and_marks_visit_voided`
- `discard_locked_visit_is_rejected`
- `discard_draft_soft_deletes_and_emits_audit`
- `inventory_adjustments_trigger_blocks_business_update`
- `patients_search_returns_matches_by_fts_prefix`

**New scenarios in `visits_phase05.rs`:**

| Scenario | Asserts |
|-|-|
| `lock_writes_audit_first_then_business_then_outbox` | Order assertion: inspect WAL frames or instrument the writer; the `audit_log` INSERT for the visit-lock action precedes the `visits` UPDATE. Mirrors phase-04 audit-first verification but scaled to lock's multi-row fan-out (1 visit + N adjustments + N item recomputes, each preceded by its own audit row). |
| `lock_rolls_back_entire_transaction_when_receipt_render_fails` | Force `ReceiptGenerator::render` to return `Err` (test feature flag `force-receipt-failure`). Expect: no `visits` row update (status remains `draft`), no `inventory_adjustments` rows, no audit rows, no outbox rows. Per §7.10 step 6.5 + §7.16. |
| `lock_rolls_back_when_disk_write_fails_after_db_writes_succeed` | Per §7.16 atomic-rename strategy: write to a tmp dir, fail the rename (chmod read-only target), expect tx rollback + tmp file cleaned up. |
| `lock_re_validates_operator_eligibility_inside_transaction` | Per §7.12 TOCTOU: pre-tx eligibility check passes; before the lock's in-tx re-check, force-close the operator's shift via a direct UPDATE; expect `LockError::OperatorBecameIneligible`; assert no partial write. |
| `lock_validates_patient_name_non_empty_at_lock_time` | Per §7.13: a draft whose `patient.name` arrived from sync as whitespace -> `Err(LockError::PatientNameEmpty)`. |
| `lock_writes_seven_name_snapshots_including_subtype_and_doctor_when_present` | After lock, all 7 name-snapshot columns populated; reprint reads only from snapshots, not joins. Per §7.17. |
| `lock_keeps_internal_pct_null_when_doctor_id_set_and_100_when_house` | Per §7.1 iff invariant. |
| `lock_emits_lock_start_and_lock_end_metrics_events` | Per §7.54: `metrics_events` table contains both rows with the same `correlation_id`; `lock_end.duration_ms` is the delta. On forced-failure path, `lock_end.blocked=true` and `reason` carries the error code. |
| `lock_emits_visit_locked_tauri_event` | Test rig subscribes to `visits:locked` (per §7.50); event fires after commit, payload contains `visit_id`, `operator_id`. |
| `update_draft_rejects_field_patch_on_locked_visit` | Per §7.32: `(Locked, Draft)` blocked at service layer with `IllegalTransition`. |
| `void_writes_offsetting_consume_rows_with_correct_by_user_id` | Per §7.15: every offset row carries `by_user_id = void_actor_id`. |
| `void_rejects_re_void_with_typed_error` | Per §7.19: second void on the same visit -> `Err(VoidError::NotLocked { current: Voided })`. |
| `void_only_offsets_existing_consume_rows_idempotently` | Per `compute_void_offsets`: if a prior void failed mid-flight leaving 2 of 3 offsets in place, the next void only emits the missing 3rd. |
| `discard_rejects_locked_or_voided` | Already covered for locked; add a voided-row scenario for parity. |
| `list_today_by_check_returns_only_today_and_excludes_drafts_when_filter_locked` | The `visits_check_type` index drives the query; assert via `EXPLAIN QUERY PLAN`. |
| `list_drafts_by_check_returns_drafts_regardless_of_date` | Per §7.7. Draft from yesterday must appear. Uses `visits_drafts` index. |
| `list_workspace_paginates_by_created_at_id_cursor` | Per §7.21 + §7.44: seed 75 rows; first page `limit=50` returns 50 + `next_cursor`; second page returns 25 + `next_cursor=None`; total stable across runs. |
| `list_workspace_applies_all_four_filters_in_combination` | Subtype + doctor + status + date range -> exact row set match. |
| `pricing_resolve_returns_freshly_computed_snapshot_without_mutating_visit` | Per §7.43: call `pricing::resolve`; assert returned `VisitSnapshots`; assert `visits.version` unchanged; assert no audit row written. |
| `lock_dryrun_returns_all_blockers_for_invalid_draft` | Per §7.38: draft with `patient.name=""`, no open shift, missing subtype -> returns 3 `LockBlocker` variants. |
| `lock_dryrun_returns_empty_when_draft_lockable` | Happy path -> `vec![]`. |
| `patients_recent_index_used_in_search` | `EXPLAIN QUERY PLAN` for `patients::search` mentions `patients_recent` index. Per §7.35. |
| `migration_creates_three_tables_and_all_indexes` | `005_patients_visits_adjustments.sql` is idempotent on a fresh DB AND on a DB seeded with `clinical-day.sql`. All declared indexes from §1 + §7.5 + §7.35 + §7.41 present. |
| `inventory_adjustments_no_update_trigger_blocks_business_updates` | Already covered; assert additionally that updates to `version`, `dirty`, `last_synced_at`, `origin_device_id` are still allowed (per §7.33 sync-metadata carve-out). |
| `inventory_adjustments_no_update_trigger_blocks_soft_delete` | Setting `deleted_at` to non-null -> `RAISE(ABORT)`. Per §7.33. |

**New scenarios in `patients_phase05.rs`:**

| Scenario | Asserts |
|-|-|
| `patients_search_filters_to_last_30_days_by_default` | Per §7.35: a patient with last visit 31 days ago is excluded; with `since_days=60` it returns. |
| `patients_search_fts5_handles_arabic_query` | `"لي"` returns Layla; `"Lay"` also returns Layla; results stable order. |
| `patients_search_returns_empty_for_match_operator_injection` | Input `"Layla MATCH 'foo'"` is treated as a literal FTS query, not as MATCH syntax. Per §6.7. |
| `patients_search_excludes_soft_deleted` | Soft-delete a patient; search returns 0 rows. |
| `patients_soft_delete_rejects_when_referenced_by_non_deleted_visits` | Per §7.34: returns `Err(PatientError::ReferencedByVisits)`. |
| `patients_soft_delete_allows_when_only_soft_deleted_visits_reference_it` | The check considers `deleted_at IS NULL` only. |
| `patients_fts_triggers_keep_index_in_sync_on_insert_update_delete` | After each of the 3 trigger paths, an FTS5 query returns the expected current state. |
| `patients_fts_index_handles_rename` | Rename via `update`; old name no longer matches; new name matches. |

**New scenarios in `inventory_adjustments_phase05.rs`:**

| Scenario | Asserts |
|-|-|
| `adjustment_insert_with_reason_consume_requires_visit_id` | CHECK constraint trips when `reason='consume_visit' AND visit_id IS NULL`. |
| `adjustments_chrono_index_used_for_history_scans` | `EXPLAIN QUERY PLAN` for `list_consume_for_visit` and item-history scans mentions `inventory_adjustments_chrono` (§7.41). |
| `adjustment_outbox_op_payload_includes_visit_id_when_consume` | Per §7.18 + §7.36. |

### §2.2 Tauri IPC handler tests

One test per command, happy + at least one error path.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `patients_search` | `returns_top_n_patients_by_fts_match_with_since_days_default_30` | `rejects_query_below_min_length_1` -> `Validation`. |
| `patients_create` | `returns_serialized_patient_with_uuid_v7` | `rejects_empty_name` -> serialized `AppError::Validation` with field path. |
| `patients_get` | `returns_full_patient_row` | `returns_not_found_for_unknown_id` -> `AppError::NotFound`. |
| `patients_update` | `applies_partial_name_patch_and_bumps_version` | `rejects_update_on_soft_deleted_patient` -> `Validation`. |
| `visits_checks_grid` | `returns_one_card_per_active_check_type_with_today_count` | `returns_not_authenticated_when_no_session` -> `NotAuthenticated`. |
| `visits_list_today_by_check` | `returns_only_today_in_local_tz` | `rejects_malformed_check_type_id` -> `Validation`. |
| `visits_list_drafts_by_check` | `returns_drafts_regardless_of_date` | `returns_empty_when_check_type_has_no_drafts` -> `Ok(vec![])`. |
| `visits_list_workspace` | `paginates_and_filters` (single test covering one filtered+cursored slice) | `rejects_invalid_cursor_encoding` -> `Validation`. |
| `visits_get` | `returns_visit_with_joined_refs_including_name_snapshots` | `returns_not_found` -> `NotFound`. |
| `visits_create_draft` | `creates_draft_with_running_total_computable_offline` | `rejects_draft_with_unsupported_dye` -> `Validation`. |
| `visits_update_draft` | `applies_subtype_change_and_recomputes_running_total` | `rejects_update_when_status_locked` -> `IllegalTransition`. |
| `visits_discard` | `returns_unit_and_marks_visit_deleted` | `rejects_discard_when_status_locked` -> per §7.31. |
| `visits_qualified_operators` | `returns_only_clocked_in_with_matching_specialty` | `returns_empty_when_no_open_shifts` -> `Ok(vec![])`. |
| `visits_lock` | `returns_LockResult_with_visit_and_receipt_paths` | `rejects_lock_when_operator_no_longer_eligible_at_lock_time` -> `LockError::OperatorIneligible` typed enum. |
| `visits_lock_dryrun` | `returns_empty_when_lockable` | `returns_all_blockers_for_invalid_draft` -> `Ok(vec![NoQualifiedOperator,...,SubtypeMissing])`. |
| `visits_void` | `void_returns_serialized_visit_with_status_voided_and_offsets_visible_in_list_consume` | `rejects_void_by_receptionist` -> `Validation`. |
| `visits_pricing_resolve` | `returns_fresh_snapshots_without_mutating_visit` | `rejects_pricing_resolve_on_voided_visit` -> per §7.43 read-only contract: voided visits are not recomputable (`Validation`). |
| `shifts_lines_run_today` (cross-phase, **owned here per §7.25**) | `returns_count_of_today_locked_visits_for_operator` | `returns_zero_when_operator_has_no_visits_today` -> `Ok(0)`. |
| `receipts_reprint` | `re_renders_and_writes_pdf_and_thermal_paths` | `rejects_reprint_on_draft_visit` -> `Validation`. |
| `receipts_print_pdf` (per §7.23 -- if not yet implemented, this row is a forward-declaration) | `opens_print_dialog_via_tauri_plugin_shell` | `returns_no_printer_configured_when_settings_blank` -> `Validation` with i18n key. |
| `receipts_print_thermal` (per §7.23) | `sends_thermal_bytes_to_configured_printer` | `rejects_print_when_thermal_file_missing` -> `NotFound`. |
| `settings_list_printers` (per §7.23) | `returns_at_least_one_printer_on_test_host_with_lpstat` | `returns_empty_when_shell_command_absent` -> `Ok(vec![])`. |
| (Error envelope -- fixed row enforced by template) | Every command's error path serializes as `{ kind, message }` per phase-04 IPC pattern. | -- |

### §2.3 Sync server route handlers

DB: real Prisma test DB; per-test teardown.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` (Patient -- LWW) | `lww_higher_updated_at_wins` | Two pushes 1s apart -> later wins; row reflects later state; older push returns `status: "skipped"`. |
| `POST /sync/push` (Patient -- LWW) | `lww_tiebreak_on_equal_updated_at_uses_origin_device_id_lex` | Per §7.40: lex-smaller wins. |
| `POST /sync/push` (Patient) | `accepts_inline_created_patient_from_visit_flow` | Per §7.18: a `patients::create` op enqueued by `<NewVisitForm>` is accepted as a normal LWW row; the subsequent `visits::create_draft` op landing in the same batch resolves FK correctly. |
| `POST /sync/push` (Patient) | `soft_delete_rejected_when_visits_still_reference` | Per §7.34 server-side mirror: `409 PATIENT_REFERENCED` (or 422; pick one and match the i18n key registry). |
| `POST /sync/push` (Visit -- manual) | `manual_conflict_returns_409_with_local_and_server_envelopes` | Per §3.Server `ConflictVisitResponseSchema`. Body shape: `{ success: false, error: { code: 'VISIT_CONFLICT', details: { local: VisitResponse, server: VisitResponse } } }`. |
| `POST /sync/push` (Visit) | `manual_conflict_creates_ConflictParked_row` | After 409, the server has a `conflict_parked` row referencing both versions. (The resolver UI is phase-08.) |
| `POST /sync/push` (Visit) | `accept_push_validates_status_conditional_typebox` | Per §7.6: a `status=locked` payload with `total_amount_iqd_snapshot` missing -> `422` on that field. |
| `POST /sync/push` (Visit) | `accept_push_validates_total_equals_sum` | Per §7.2: a payload where total != price + dye + report -> `422` with code `TOTAL_INVARIANT_VIOLATION`. |
| `POST /sync/push` (Visit) | `accept_push_validates_internal_pct_iff_doctor_null` | Per §7.1: violation -> `422 IFF_VIOLATION`. |
| `POST /sync/push` (Visit) | `accept_push_re_validates_subtype_dye_report_invariants` | Per §7.3: server re-loads referenced `CheckType` + `CheckSubtype` and rejects bad combos. |
| `POST /sync/push` (Visit) | `accept_push_assert_transition_blocks_locked_to_draft_push_from_peer` | Per §7.32: `(Locked, Draft)` push -> `409 ILLEGAL_VISIT_TRANSITION`. |
| `POST /sync/push` (Visit) | `accept_push_discard_on_locked_returns_409_illegal_transition` | Per §7.31 reconciled with §7.53(2): `409 ILLEGAL_VISIT_TRANSITION { from: 'locked', to: 'discarded' }`, NOT `422 visit_discard_not_draft`. |
| `POST /sync/push` (Visit) | `accept_push_returns_idempotent_response_on_op_id_replay` | `ProcessedOp` cache hit returns identical response (including the original 409 if applicable). |
| `POST /sync/push` (InventoryAdjustment) | `additive_insert_succeeds_and_outbox_records_clean` | Two pushes from two devices with distinct `id` -> both rows persist. |
| `POST /sync/push` (InventoryAdjustment) | `replay_same_op_id_returns_cached_response_not_duplicate` | Per §7.36. |
| `POST /sync/push` (InventoryAdjustment) | `rewrite_of_existing_row_by_peer_returns_409_ADDITIVE_VIOLATION` | Per §7.36: server uses `create`, not `upsert`; an `id`-collision without `op_id` cache hit returns `409`. |
| `POST /sync/push` (InventoryAdjustment) | `consume_visit_requires_non_null_visit_id` | Per §1 CHECK mirror at TypeBox layer. |
| `GET /sync/pull` (Patient) | `pull_returns_LWW_resolved_row_with_pulled_at_set` | Per §7.52. |
| `GET /sync/pull` (Visit) | `pull_returns_seven_name_snapshot_columns` | Per §7.17 + §7.52. |
| `GET /sync/pull` (Visit) | `pull_excludes_other_tenants_visits` | Tenant guard. |
| `GET /sync/pull` (InventoryAdjustment) | `pull_orders_by_created_at_origin_device_id_id_for_total_stability` | Per §7.41. |

### §2.4 React Query mutation / query flows (mocked IPC via `msw@2` worker)

Mocked IPC; assert cache invalidation, optimistic update, rollback on error.

**RTL invariant (mandatory):** every component test that renders DOM MUST use `describe.each([['ltr'],['rtl']])`. Asserting only LTR is incomplete per `.claude/rules/testing.md` §14.

| Hook / Component | Test | Asserts |
|-|-|-|
| `usePatientSearch` | `debounces_300ms_and_invokes_ipc_once_per_settled_query` | Type "Lay", "Layl", "Layla" within 100ms -> 1 IPC call, not 3. |
| `usePatientSearch` | `returns_empty_array_when_no_match_and_does_not_throw` | -- |
| `useVisitCreate` | `optimistically_inserts_into_today_by_check_cache` | Pre-IPC: cache contains the optimistic row. On IPC reject: rolled back. On resolve: replaced with server row. |
| `useVisitUpdate` | `invalidates_byCheck_detail_and_workspace_keys_on_settle` | -- |
| `useVisitDiscard` | `removes_row_from_byCheck_cache_on_success` | -- |
| `useVisitLock` | `invalidates_inventory_and_audit_and_byCheck_keys_on_success` | The lock fan-out has the broadest cache impact. Per §7.50: `visits:locked` event also invalidates `['shifts','lines_run', operator_id]`. |
| `useVisitVoid` | `invalidates_inventory_audit_byCheck_detail_keys` | -- |
| `useVisitPricingResolve` | `does_not_invalidate_caches_read_only_query` | -- |
| `useReceiptReprint` | `surfaces_typed_error_when_visit_is_draft` | -- |
| `useQualifiedOperators` | `enabled_only_when_check_type_id_truthy` | -- |
| `<NewVisitForm>` | `renders_skeleton_loading_empty_error_states_per_phase_04_dod_convention` | All 4 states present per `.claude/rules/frontend.md` mandate. |
| `<NewVisitForm>` | `running_total_matches_lock_result_total_for_same_inputs` | Mock `visits::lock` to record its computed total; assert UI's running total equals the recorded total exactly. The parity test. |
| `<NewVisitForm>` | `recalculate_button_calls_pricing_resolve_and_updates_total` | Per §7.28. |
| `<NewVisitForm>` | `pricing_changed_banner_renders_on_event` | Per §7.27: dispatch a `catalog:pricing_changed` Tauri event; banner appears. |
| `<NewVisitForm>` | `settings_changed_banner_renders_on_event` | Per §7.42. |
| `<NewVisitForm>` | `lock_button_disabled_while_lock_dryrun_returns_blockers` | Per §7.38. |
| `<NewVisitForm>` | `lock_button_disabled_state_announced_to_screen_readers_via_aria_describedby` | Per §7.48 + §7.47. |
| `<NewVisitForm>` | `ctrl_enter_triggers_lock_when_validation_passes` | Per §7.47 keyboard contract. |
| `<OperatorPickerDialog>` | `does_not_open_when_qualified_set_empty_and_surfaces_lock_blocked_toast` | Per §7.48. |
| `<OperatorPickerDialog>` | `arrow_key_navigation_and_enter_select` | Per §7.47. |
| `<VoidModal>` | `focus_trap_and_escape_to_cancel_and_autofocus_textarea` | Per §7.47. |
| `<VoidModal>` | `submit_disabled_until_reason_at_least_5_graphemes` | UI mirrors §7.14. |
| `<DiscardConfirm>` | `focus_trap_and_escape_to_cancel` | Per §7.47. |
| `<WorkspaceVisitsTable>` | `renders_12_columns_in_correct_order_and_mirrors_in_rtl` | Per §7.21 column list. |
| `<WorkspaceVisitsTable>` | `pending_sync_dirty_dot_renders_only_when_row_dirty_1` | Per §7.29. |
| `<DirtyDot>` | `tooltip_text_matches_i18n_key_in_both_locales` | Per §7.29. |
| `<ChecksGridCard>` | `renders_up_to_3_subtype_chips_plus_overflow_when_has_subtypes` | Per §7.20. |
| `<VisitDetail>` (and tabs) | `readonly_mode_hides_actions_and_keeps_print_buttons` | Per §7.24. |
| `<LockValidationErrors>` | `lists_each_LockBlocker_with_distinct_i18n_key_and_focusable_links` | Per §7.38 + §7.47. |
| `<ReceiptPreview>` | `renders_pdf_embed_and_thermal_text_side_by_side` | Per §7.22. |
| `<PatientAutocomplete>` | `creates_new_patient_via_ipc_when_no_selection_made_on_first_save` | Per §4 frontend step 1. |
| `<DoctorAutocomplete>` | `empty_input_is_treated_as_house_and_disables_doctor_cut_preview` | -- |
| `<SettingsForm>` (cross-phase touch) | `thermal_printer_combobox_consumes_settings_list_printers_output` | Per §7.23. |

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Phase 05 adds NO new server routes -- traffic flows through `/sync/push` and `/sync/pull` (declared in phase-01). Phase-05 introduces 3 new entity-shape contributions to those routes' schemas.

Harness: `sync-server/test/contract/visits-contract.test.ts` (NEW), `patients-contract.test.ts` (NEW), `inventory-adjustments-contract.test.ts` (NEW). Each fetches `/documentation/json`, dereferences with `@apidevtools/json-schema-ref-parser`, compiles the relevant subschemas with Ajv 8 + `ajv-formats`.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (request) | `PatientPushSchema` | `fixtures/payloads/patient-push-insert.json`, `...-update.json`, `...-soft-delete.json`. |
| `POST /sync/push` (request) | `VisitPushSchema` (composite per §7.6 with `DraftFields` / `LockedFields` / `VoidedFields` variants) | `fixtures/payloads/visit-push-draft.json`, `visit-push-locked.json`, `visit-push-voided.json`. Each MUST validate. |
| `POST /sync/push` (request, negative) | `VisitPushSchema` | `visit-push-locked-missing-total.json` MUST fail Ajv with required-field error on `total_amount_iqd_snapshot`. `visit-push-iff-violation.json` MUST fail the custom keyword `internalPctIffDoctor`. `visit-push-total-sum-violation.json` MUST fail the custom keyword `totalEqualsSum`. |
| `POST /sync/push` (request) | `InventoryAdjustmentPushSchema` | `fixtures/payloads/adjustment-push-consume.json`, `adjustment-push-receive.json`, `adjustment-push-writeoff.json`. |
| `POST /sync/push` (response) | `SyncPushResponseSchema` (per-op `results[]`) | Captured live; assert `status in ['applied','skipped','conflict','rejected']`. Visit-conflict response MUST include `details` matching `ConflictVisitResponseSchema`. |
| `GET /sync/pull` (response) | `VisitResponseSchema` (with all 7 name-snapshot columns + `pulled_at` per §7.52) | Captured live for seeded tenant. Each `entity: 'visits'` row MUST validate. |
| `GET /sync/pull` (response) | `PatientResponseSchema` (with `pulled_at`) | Captured live; validates. |
| `GET /sync/pull` (response) | `InventoryAdjustmentResponseSchema` (with `pulled_at`) | Captured live; validates. |

### §3.2 IPC shape contract

The §3.2 contract for phase-05 IPCs. Last row is FIXED -- the shared `AppError` envelope.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `patients_search` | `Vec<Patient>` | `z.array(PatientSchema)` |
| `patients_create` | `Patient` | `PatientSchema` |
| `patients_get` | `Patient` | `PatientSchema` |
| `patients_update` | `Patient` | `PatientSchema` |
| `visits_checks_grid` | `Vec<ChecksGridCard>` | (NEW) `ChecksGridCardSchema = z.object({ check_type_id, slug, name_en, name_ar, today_count, sample_subtypes: z.array(z.string()).max(3), more_count: z.number() })` per §7.20 |
| `visits_list_today_by_check` | `Vec<VisitSummary>` | (NEW) `VisitSummarySchema` (12 columns from §7.21) |
| `visits_list_drafts_by_check` | `Vec<VisitSummary>` | `VisitSummarySchema` |
| `visits_list_workspace` | `{ rows: Vec<VisitSummary>, next_cursor: Option<String> }` | `z.object({ rows: z.array(VisitSummarySchema), next_cursor: z.string().nullable() })` |
| `visits_get` | `VisitWithJoinedRefs` | `VisitWithJoinedRefsSchema` (extends `VisitSchema` with `patient`, `doctor`, `operator`, `check_type`, `check_subtype` joined refs + the 7 name snapshots) |
| `visits_create_draft` | `Visit` | `VisitSchema` (status=draft branch) |
| `visits_update_draft` | `Visit` | `VisitSchema` |
| `visits_discard` | `()` | `z.void()` |
| `visits_qualified_operators` | `Vec<Operator>` | `z.array(OperatorSchema)` (from phase-03) |
| `visits_lock` | `LockResult { visit, artifacts }` | (NEW) `LockResultSchema = z.object({ visit: VisitWithJoinedRefsSchema, artifacts: ReceiptArtifactsSchema })` |
| `visits_lock_dryrun` | `Vec<LockBlocker>` (tagged union) | (NEW) `LockBlockerSchema` -- discriminated union with 7 variants per §7.38 |
| `visits_void` | `VisitWithJoinedRefs` | `VisitWithJoinedRefsSchema` |
| `visits_pricing_resolve` | `VisitSnapshots` | (NEW) `VisitSnapshotsSchema` per §7.43 |
| `shifts_lines_run_today` | `u32` | `z.number().int().nonnegative()` |
| `receipts_reprint` | `ReceiptArtifacts { pdf_path, thermal_path }` | (NEW) `ReceiptArtifactsSchema = z.object({ pdf_path: z.string(), thermal_path: z.string() })` |
| `receipts_print_pdf` | `()` | `z.void()` |
| `receipts_print_thermal` | `()` | `z.void()` |
| `settings_list_printers` | `Vec<PrinterInfo>` | (NEW) `PrinterInfoSchema = z.object({ name: z.string(), is_default: z.boolean() })` |
| (Error envelope -- fixed) | `AppError` serialized via `Serialize` impl | `AppErrorSchema = z.object({ kind: z.enum([...]), message: z.string() })` -- shared schema referenced by every command's error path. New variants this phase introduces: `LockError`, `VoidError`, `VisitError`, `PatientError` -- each MUST be in the `kind` enum. |

The harness MUST also assert the inverse: every Zod-declared field appears in the Rust JSON. Fields added on either side without updating the other fail the contract.

### §3.3 Sync envelope contract

- **Push payload conforms.** `OperatorShiftPushPayload`-style structs (`PatientPushPayload`, `VisitPushPayload`, `InventoryAdjustmentPushPayload`) serialized via Rust `serde` -> validate against the matching TypeBox schemas on the server. Per §7.6 + §7.52.
- **Pull payload conforms.** Server's response shapes for each of the 3 new entities -> validate against mirrored client schemas (Zod for in-IPC types).
- **Conflict-resolution policy registry agrees.** Assert the engine's policy registry returns:
  - `('patients', 'last-write-wins')` with the `origin_device_id` lex tiebreak per §7.40.
  - `('visits', 'manual')` with 409 envelope shape per `ConflictVisitResponseSchema`. Per §7.40 algorithm.
  - `('inventory_adjustments', 'additive-only')` with the immutability rule per §7.36 (server uses `create`, not `upsert`).
- **Envelope version.** All 3 new entities ride `envelope_version: 1`; stub at `999` rejected.
- **Snapshot files** (per `.claude/rules/testing.md` §10):
  - `expected/sync/patient-push-canonical.json.sha256`
  - `expected/sync/visit-push-locked-canonical.json.sha256`
  - `expected/sync/visit-push-voided-canonical.json.sha256`
  - `expected/sync/visit-pull-row-canonical.json.sha256`
  - `expected/sync/inventory-adjustment-push-canonical.json.sha256`
- **Receipt golden snapshots** (per §10 receipts rules):
  - `expected/receipts/a5-locked-visit-en.pdf` -- hash of extracted text layer + hash of rendered page-1 bitmap at 150 dpi (via `pdf-extract` + `pdfium-render`).
  - `expected/receipts/a5-locked-visit-ar.pdf` -- Arabic + RTL render.
  - `expected/receipts/thermal-locked-visit-en-32col.txt` -- byte-exact hash.
  - `expected/receipts/thermal-locked-visit-ar-48col.txt` -- byte-exact hash with Arabic-Indic digits per §7.46.

---

## §4 E2E Tests (Pyramid Layer 4)

All `data-testid` selectors. Specs live under `e2e/specs/visits/` and `e2e/specs/receipts/`.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `new-visit-create-draft.e2e.ts` | Mehdi (receptionist) | 1) Log in. 2) Navigate to `/reception`. 3) Click a check card. 4) Click `[+ New visit]`. 5) Type partial patient name; pick from autocomplete. 6) Pick subtype; toggle dye; pick doctor. 7) Assert running total recomputes live. 8) Save draft. | Draft visible in workspace list with `dirty=1` indicator. Outbox contains 1 op. After server reachable, op drains; indicator clears. |
| `lock-and-print.e2e.ts` | Mehdi | 1) Open a draft with all required fields. 2) Ensure an operator with matching specialty is clocked in. 3) Click `Lock & print`. 4) Operator picker opens; pick. 5) Assert PDF print dialog opens (mocked in test rig). 6) Assert navigation to `/reception/visits/:id`. | `visits` row status=`locked`, all 7 snapshots populated, all 7 name-snapshots populated. `inventory_adjustments` rows present for each mapped item. `audit_log` rows in audit-first order. PDF file exists at `$APPDATA/.../receipts/<YYYY>/<MM>/<visit-id>.pdf`. Thermal text exists. `visits:locked` Tauri event observed. |
| `lock-blocked-by-dryrun.e2e.ts` | Mehdi | 1) Open a draft missing the subtype. 2) Assert `<LockValidationErrors>` lists `SubtypeMissing`. 3) Assert Lock button disabled with `aria-describedby` pointing at the errors. 4) Pick the subtype. 5) Assert button re-enables. | Per §7.38. |
| `void-locked-visit.e2e.ts` | Mariam (superadmin) | 1) Open a locked visit. 2) Click Void. 3) Type a 6-char reason. 4) Submit. 5) Assert status flips to `voided`, offset rows appear, inventory recomputes. | Audit log shows: 1 `void` row on the visit + N `create` rows on offsets + N `update` rows on items. Outbox has matching entries. |
| `reprint-from-receipts-tab.e2e.ts` | Mehdi | 1) Open a locked visit's Receipts tab. 2) Click Reprint. 3) Assert print dialog reopens for the existing PDF (no re-render). | Re-render only happens if files missing -- this test deletes the PDF first to exercise the re-render branch in a second variant. |
| `patient-fts-search-en-and-ar.e2e.ts` | Mehdi | Search "Lay" (en); "لي" (ar). | Both return Layla; results stable. |
| `house-doctor-no-cut.e2e.ts` | Mehdi | Create draft, leave doctor empty (house). | Running total: `internal_pct=100`, `doctor_cut=0`. After lock: snapshots match. |
| `recalculate-on-pricing-changed.e2e.ts` | Multi-session | 1) Admin edits a doctor's `cut_value`. 2) Receptionist with an open draft sees `<PricingChangedBanner>`. 3) Click Recalculate. 4) Total reflects new cut. | Per §7.27 + §7.49. |
| `accountant-readonly-drilldown.e2e.ts` | Asma (accountant) | Navigate to `/accounting/visits/<locked-id>`. | `<VisitDetail>` renders in `mode=readonly` per §7.24: no Void button, no Discard, Reprint still visible. |
| `breadcrumb-resolves-localized-name.e2e.ts` | Mehdi | Navigate the reception subtree. | Breadcrumbs show localized check name, then `New visit` or patient name snapshot per §7.56. |
| `reception-route-role-guard.e2e.ts` | Asma | Attempt `/reception/checks/<slug>/new`. | Redirected by `<RequireRole>` per §7.58. |

### §4.2 Failure-path flows

- **`offline-lock-and-drain.e2e.ts`** -- Set `--offline` on tauri-driver before lock; assert lock succeeds locally (UI confirms, receipts file written, outbox grows); reconnect; assert all ops drain; assert server has the visit + every adjustment + every audit row in order.
- **`receipt-render-failure-aborts-lock.e2e.ts`** -- Inject `force-receipt-failure` feature flag; click Lock; assert the lock fails with a clear error toast; assert the draft is unchanged (status still `draft`); assert no audit/adjustment rows written. Per §7.10 step 6.5 + §7.16.
- **`disk-write-failure-aborts-lock.e2e.ts`** -- Set receipts dir to read-only; click Lock; assert atomic rollback with tmp files cleaned up. Per §7.16.
- **`operator-clocks-out-mid-lock.e2e.ts`** -- Race: receptionist clicks Lock; just before the in-tx eligibility check, a second session clocks the operator out. Assert `LockError::OperatorBecameIneligible` toast surfaces with i18n key `errors:operator.ineligible.operator_became_ineligible`. Per §7.12.
- **`server-returns-409-on-visit-push.e2e.ts`** -- Device A locks a visit; Device B (offline at same time) edits the same visit's `note` then reconnects. B's push returns 409 with `local + server` envelope. Assert B's UI surfaces a "conflict parked" indicator that points the user to phase-08's resolver (which doesn't exist yet -- assert the placeholder error message is correct).
- **`patient-soft-delete-blocked-by-visits.e2e.ts`** -- Per §7.34: admin tries to soft-delete a patient with a locked visit. Assert toast `errors:patient.referenced` with linked visits count.
- **`pricing-changed-banner-survives-route-change.e2e.ts`** -- Open a draft; trigger `catalog:pricing_changed`; banner appears; navigate away and back; assert banner state persists in the draft store.
- **`token-expiry-mid-lock-flow.e2e.ts`** -- Force JWT expiry between `lock_dryrun` and `lock` calls; assert one refresh + retry; assert lock eventually succeeds; assert no duplicate audit rows.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-patient-lww.e2e.ts` | Device A edits Layla's name "Layla H."; Device B edits "Layla M.". Both reconnect 1s apart. | Server keeps Device A's version (later `updated_at`). Both devices converge to "Layla M." after pull. Wait, fix: Device A's later wins. Verify the rule actually maps to the test's later device. (Re-derive at test-author time.) Per §7.40. |
| `two-device-patient-lww-tiebreak.e2e.ts` | Identical `updated_at` (clock-skew rig). | Lex-smaller `origin_device_id` wins. |
| `two-device-visit-manual-conflict.e2e.ts` | Both devices offline. Device A edits a draft's subtype. Device B locks the same draft. Both reconnect. | A's push (an update to a now-locked row from peer's view) returns 409 with `local=A_draft, server=B_locked` envelope. ConflictParked row on server. UI on Device A surfaces conflict indicator. |
| `two-device-additive-inventory.e2e.ts` | Both devices lock distinct visits at the same time, each consuming the same inventory item. Both reconnect. | Both consume rows persist on server. Item `quantity_on_hand` consistent after pull on both devices (recomputed). Per §7.36. |
| `three-device-lock-race.e2e.ts` | Three devices online; each tries to lock a different draft for the same operator simultaneously. | Exactly one lock succeeds per visit; eligibility re-check inside tx blocks the others when applicable. No partial state. Audit log shows three `lock_start` events but only one `lock_end` per visit. |
| `two-device-name-snapshot-isolation.e2e.ts` | Device A locks a visit. Device B renames the patient (LWW). Reprint on Device A still shows the original `patient_name_snapshot`. | Per §7.17. Snapshot isolation -- the whole point. |
| `multi-device-visits-locked-event-invalidates-shifts-lines-run-cache.e2e.ts` | Device A locks a visit; Device B (same operator on its on-shift table) | Per §7.50: Device B's `<ShiftHistoryToday>` lines-run cell for that operator increments after pull. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: A5 PDF in en + ar.** Render a locked visit's receipt with `arabic_numerals=true`, locale `ar`; print to PDF; visually inspect: header on right, totals column on left, digits in Arabic-Indic shape; signature box positioned correctly; QR not yet present (Horizon-1). Repeat with `en` locale. Snapshot hash matches `expected/receipts/a5-*.pdf`.
- **Visual: Thermal text in en + ar.** Render to a real ESC/POS printer at 32 cols and 48 cols. Assert wrap behavior, alignment, no clipped characters. The byte hash matches `expected/receipts/thermal-*.txt`.
- **Keyboard-only NewVisit flow.** Per §7.47: navigate the entire `<NewVisitForm>` with keyboard only; assert focus order; assert `Ctrl/Cmd+Enter` triggers Lock; assert Escape cancels the Operator Picker; assert focus returns to the Lock button after picker close.
- **Screen reader announcements.** With NVDA / VoiceOver enabled, assert: form labels announced; running total announced as currency; LockBlocker bullets announced with their full message; modal open/close announces dialog state.
- **Printer enumeration on each OS.** Run `settings_list_printers` on Linux (CUPS / lpstat), macOS (lpstat), Windows (wmic). Assert at least one printer name returns. Pick one as the thermal printer; persist; verify it survives app restart. Per §7.23 + §7.45.
- **Receipt RTL print on hardware.** With an actual thermal printer in Arabic mode, lock a visit and print. Visually confirm RTL rendering matches the test snapshot. Per §7.30.

### §5.2 Cross-references to `personas.md`

- `personas.md` -> **P2 Mehdi the Receptionist** -> steps 3-7 (25 visits, FTS search, lock with dye, inventory consumption, lunch-offline period, reconnect-drain). This is the canonical persona for phase-05.
- `personas.md` -> **P4 Two-Device Conflict** -> steps 5-10 (concurrent visit creation, manual policy, conflict parking). Reinforcement.
- `personas.md` -> **P5 Year-End Audit** -> steps 2-5 (12-month report aggregates over visits). Read-only path.
- `personas.md` -> **P3 Mariam the Superadmin** -> step 5 (creating doctors with pricing) -- tangential; pricing changes drive `<PricingChangedBanner>` here.

**Canonical: P2.** P2 MUST pass for §8 DoD to flip to `complete`.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone
- **Asia/Baghdad fixed offset.** `visits::list_today_by_check` uses local-midnight boundaries from `Asia/Baghdad` (UTC+03:00 fixed; no DST). Test: a visit `locked_at = 23:59:30 +03:00` falls into the correct local day. Asserted in `list_today_by_check_uses_baghdad_local_midnight`.
- **Day-boundary on lock.** A draft created yesterday and locked at `00:02 local today` appears in today's `list_today_by_check`, not yesterday's. The query uses `locked_at`, not `created_at`. Asserted in `list_today_uses_locked_at_not_created_at`.
- **Clock skew vs server.** Lock locally at `12:00:00`; server stamps push at `12:00:02`; client pull reflects server's `updated_at` as canonical for ordering. Asserted in `lock_pullback_uses_server_updated_at`.
- **Receipt timestamp.** A5 PDF and thermal both display `locked_at` in Asia/Baghdad local time. The thermal-text snapshot test pins the format `YYYY-MM-DD HH:MM` in Baghdad; the i18n `ar` variant shows the same in Arabic numerals if the setting is on. Per §7.46.
- **DST defensive.** Same as phase-04: a CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `visits/`, `patients/`, `receipts/` modules; only `chrono::FixedOffset::east_opt(3 * 3600)`.

### §6.2 i18n & RTL
- **en/ar swap on every new route.** Snapshot `/reception`, `/reception/checks/:slug`, `/reception/checks/:slug/new`, `/reception/visits/:id` in both locales. Cross-cutting full sweep in `i18n-rtl.md`; this plan asserts that every visible string comes from `reception.*` or `errors:visit.*` / `errors:operator.*` i18n keys -- no string literals in JSX.
- **Arabic-Indic numerals on every numeric column.** `<WorkspaceVisitsTable>` totals, `<RunningTotalSummary>`, `<VisitDetailDetailsTab>` snapshot block, receipt PDF + thermal. Per §7.46. Asserted in component tests + receipt snapshot tests.
- **RTL layout invariants.** Eyebrow rule mirrored, numeric columns right-aligned (which under RTL aligns to page edge), `<DirtyDot>` leads the row in both directions, pill dots lead their label.
- **Mixed-direction patient names.** A patient named `"Layla هاشم"` round-trips byte-for-byte through search + lock + receipt. Asserted in `patient_name_mixed_script_byte_stable`.
- **Receipt RTL renders.** `expected/receipts/a5-locked-visit-ar.pdf` snapshot pins clinic-name + headers on the right edge, totals column on left. `expected/receipts/thermal-locked-visit-ar-48col.txt` pins RTL alignment in monospace 48-col grid.

### §6.3 Offline & Network
- **Full offline lock.** `offline-lock-and-drain.e2e.ts` (§4.2). Lock works fully offline; receipts file written; outbox grows.
- **Intermittent connection during multi-op push.** A locked visit fans out to ~5-10 outbox ops (visit + adjustments + items + audit rows). Drop the connection after op 3 of 7; assert the engine resumes from op 4, not op 1; assert no duplicate writes server-side.
- **Token expiry mid-sync.** `token-expiry-mid-lock-flow.e2e.ts` (§4.2). One 401 -> refresh + retry once; second 401 -> pause pushes, surface `session_expired`.
- **Server 5xx during push.** Per phase-04 pattern: WireMock 3x503 then 200; assert `outbox.attempts` advances; assert eventual drain.
- **Partial-batch push.** Push 50 ops including a visit-lock fan-out (7 ops). One adjustment op violates a server invariant. Assert: visit + other 6 adjustments + audit rows all applied; the invalid adjustment row is `rejected` with a reason; the rest of the batch is unaffected. Server-side test in §2.3.
- **Server-pause during receipt-render-fail rollback.** If sync server is down while the lock fails locally, the outbox state is correctly left empty (no orphan ops to drain after the rollback).

### §6.4 Concurrency & Conflicts
- **2-device same visit (`manual` policy).** `two-device-visit-manual-conflict.e2e.ts` (§4.3). 409 with `local + server` envelope; ConflictParked row.
- **2-device same patient (`last-write-wins`).** `two-device-patient-lww.e2e.ts` (§4.3). Later `updated_at` wins; tiebreak on `origin_device_id` lex (`two-device-patient-lww-tiebreak.e2e.ts`).
- **2-device same item (`additive-only` adjustments).** `two-device-additive-inventory.e2e.ts` (§4.3). Both consume rows survive; quantity_on_hand recomputed consistently.
- **3-device chain on patient LWW.** Devices A, B, C all rename the same patient offline; reconnect in random order; deterministic convergence on the highest `updated_at` (lex tiebreak on equals).
- **Operator clocks out during lock (TOCTOU).** `operator-clocks-out-mid-lock.e2e.ts` (§4.2). Per §7.12.
- **Conflict resolver round-trip.** N/A in this phase -- the resolver UI lives in phase-08. Phase-05 verifies that the `ConflictParked` row exists and the envelope shape is correct; the actual `parked -> resolve -> audit row` round-trip is in `phase-08-test.md`.

### §6.5 Crash & Recovery
- **SIGKILL mid-lock transaction.** Spawn a child process; instrument the lock writer with a feature-gated `panic!` between (a) audit-row insert and (b) visits-row update. Kill the process at that point. Reopen; assert: no audit row, no visits update, no adjustments, no outbox rows, no receipt files. Tx atomicity holds end-to-end. Test: `crash_between_audit_and_business_writes_leaves_no_partial_state`.
- **SIGKILL after receipt files written but before commit.** Use the §7.16 atomic-rename strategy -- the tmp files exist but the rename has not happened. On restart, the receipts dir is clean (no orphan tmps from the prior crash; the boot routine sweeps tmp dirs older than 5 min). Asserted in `crash_after_receipt_write_before_commit_cleans_tmp_files`.
- **SQLite WAL after crash.** Kill during a multi-op fan-out write; reopen with `journal_mode=WAL` + `busy_timeout=5000`; assert WAL replay succeeds and no `visits` row is left in an invalid CHECK state. Phase-05 invariant: a half-committed lock is impossible because the CHECK constraint blocks any `status=locked` row without all snapshots.
- **Disk full on inventory recompute.** Mid-lock, disk fills during the item recompute step. Assert `AppError::Db`, full tx rollback, no half-written rows, receipt tmp files cleaned. Test (Rust integration with tmpfs sized just below the migration footprint + 1 fan-out): `disk_full_mid_lock_rolls_back_atomically` (gated `--ignored`).
- **Crash between metrics-events writes.** Per §7.54: `lock_start` written, crash, no `lock_end`. On boot, a sweeper marks the orphan as `lock_end { blocked: true, reason: "crash" }` after a 5-min timeout. (Or, if the design says these are separate non-transactional writes -- a single orphan `lock_start` is acceptable and the metrics consumer treats it as a failure. Pick one; phase-08 §7.16 soak harness gates the choice.)
- **Crash after partial offset writes during void.** Per `compute_void_offsets` idempotency: if 2 of 3 offsets wrote then crashed, the next void invocation emits only the missing offset. Asserted in `void_resumes_after_partial_crash`.

### §6.6 Scale & Performance
- **10k visits in `clinical-day.sql` scaled.** `list_workspace` over a single check type with 10k visits, filtered to today (~50 rows): < 30 ms p99. `EXPLAIN QUERY PLAN` shows `visits_check_type` + `visits_status_date` index usage. Asserted in `perf_list_workspace_at_10k`.
- **1k+ patients FTS.** `patients::search("Lay", limit=10, since_days=30)` on 1k+ patient fixture: < 50 ms p99 per `.claude/rules/testing.md` §9. The `patients_recent` index drives the recency filter. Asserted in `perf_patient_fts_at_1k`.
- **Outbox drain throughput.** Lock 100 visits offline -> 7+ ops per lock -> 700+ op backlog. Drain at >= 50 ops/sec. Asserted in `perf_outbox_drain_visit_fan_out`.
- **Lock transaction.** End-to-end lock (validate + audit + business + adjustments + recompute + receipt write + outbox + commit): < 200 ms p99 (default `.claude/rules/testing.md` §9), with a tighter project target of < 100 ms p99 for a typical 2-adjustment lock. Per §7 below. Asserted in `perf_lock_typical_case_under_100ms`.
- **12-month aggregate.** Per persona P5 step 2: `/accounting/visits` over 12 months on 8000+ visits: < 4 s p95 (scaled from §9's 90-day SLO of < 1 s). Owned by `performance-soak.md`, gated here only as a forward-reference.

### §6.7 Security & Permissions
- **Role bypass: receptionist tries `visits_void`.** Per §2.2 error-path test. `Validation` returned; no mutation.
- **Role bypass: accountant tries `visits_lock`.** UI hides the button; IPC layer also rejects (`<RequireRole>` wraps `/reception/*` and `visits_lock` checks role server-side). Asserted in `visits_lock_rejects_accountant_role`.
- **JWT tamper: alter `role` claim from `receptionist` to `superadmin` and replay against `/sync/push` for a visit-void op.** Server rejects with 401 (signature invalid) -- the server NEVER trusts the claim shape. Cross-cutting in `security.md`.
- **FTS5 injection: `patients::search`.** Input `"Layla MATCH 'foo'"` or `"x' OR 1=1 --"` is treated as a literal FTS query, never as MATCH syntax or SQL. Asserted in `patients_search_returns_empty_for_match_operator_injection` (§2.1). Same for `doctors_fts` and any other FTS5 surface this phase touches.
- **Soft-delete bypass.** Soft-delete a visit (via `visits::discard`); then call `visits::get`, `visits::list_workspace`, `visits::list_today_by_check`, `visits::list_drafts_by_check` -- assert ALL exclude the row. Raw `SELECT * FROM visits WHERE id = ?` returns the row (it's a tombstone, not a hard delete). Asserted in `soft_delete_visit_hides_from_reads_but_persists`.
- **Server-side: receptionist token cannot push a visit-void.** Authenticated as receptionist; push a `status=voided` payload from the device; server's `accept_push` re-checks role (the JWT carries `role=receptionist`) and rejects 403. Per `auth.md` + `sync-server.md`.
- **Inventory adjustment forgery.** A peer pushes an `inventory_adjustments` row with `reason='consume_visit'` but `visit_id` pointing at a peer's locked visit (cross-tenant). Server rejects 403 (tenant guard) AND 409 if same-tenant but `id` conflicts. Per §7.36.
- **Refresh-token replay.** N/A -- owned by `security.md` cross-cutting plan.

### §6.8 Data Integrity
- **Migration replay forward.** `005_patients_visits_adjustments.sql` is idempotent on fresh DB AND on a DB seeded through `clinical-day.sql`. All `CREATE * IF NOT EXISTS` + the FTS5 triggers + the `inventory_adjustments_no_update` trigger are re-creatable. Asserted in `migration_005_idempotent_on_populated_db`.
- **Migration replay against populated DB.** Pre-load phase-01..04 data + a snapshot of an in-flight 005 install; replay 005; assert no constraint violations; assert FTS5 content is consistent with the underlying table.
- **FK enforcement.** Insert a `visits` row with `patient_id`, `check_type_id`, `receptionist_user_id` non-existent -> FK violation. Same matrix for `inventory_adjustments.{item_id, visit_id, by_user_id}`.
- **Soft-delete cascade rules.** Soft-deleting a patient when no live visits reference it -> succeeds; with a live visit -> rejected per §7.34. Soft-deleting a check_type with live drafts -> rejected (phase-03's rule, re-asserted here). Soft-deleting a doctor with live drafts -> draft falls back to house? Or rejected? (Asserts the rule chosen in phase-03; cross-checked here.)
- **`sync_version` monotonicity.** Every mutation to `visits` increments `version` by exactly 1: create_draft (0), update_draft (1, 2, ...), lock (+1), void (+1). The `lock` fan-out increments `inventory_items.version` for each affected item by 1. Asserted in `version_increments_monotonically_across_lock_fan_out`.
- **CHECK constraint enforcement.** Try to INSERT a `visits` row with `status='locked'` and a snapshot column NULL -> SQLite rejects. Same for §7.1 iff and §7.2 total invariants and §7.53 name-snapshot CHECK extension. Asserted in `visits_check_constraint_blocks_invalid_states` (one test per invariant variant).
- **Append-only trigger.** Attempting `UPDATE inventory_adjustments SET delta = -10 WHERE id = ?` -> RAISE(ABORT). Sync-metadata-only update is allowed. Per §7.33.
- **FTS5 consistency.** After 1000 random insert/update/delete operations on `patients`, the FTS5 index returns the same row set as a direct `SELECT name FROM patients WHERE deleted_at IS NULL`.
- **Name-snapshot invariant.** Post-lock, the receipt re-render reads only from snapshot columns. Manually corrupt the underlying `check_types.name_ar` after lock; reprint; assert the receipt still shows the original snapshotted name. Per §7.17.

---

## §7 Performance SLOs (this phase's surfaces)

Default SLOs in `.claude/rules/testing.md` §9 apply unless overridden. The `Default?` column declares whether the row uses the §9 default or a phase-specific override.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `visits::list_today_by_check` over 50 rows | < 30 ms p99 | yes | `perf_list_today_by_check_50_rows` | Default list-query SLO; index-driven. |
| Tauri (SQLite) | `visits::list_workspace` over 10k visits, filtered + paginated | < 50 ms p99 | no (tighter than §9 default for typical list since 50ms is the project target for a paginated workspace tab) | `perf_list_workspace_at_10k` | Workspace tab is interactive; under 50ms feels instant. |
| Tauri (SQLite) | `patients::search("Lay", since_days=30)` at 1k+ patients | < 50 ms p99 | yes | `perf_patient_fts_at_1k` | §9 FTS5 default. |
| Tauri (SQLite) | `visits::lock` typical case (1 visit + 2 adjustments + 2 items + audit fan-out + receipt write + commit) | < 100 ms p99 | no (project target tighter than §9's 200 ms lock SLO to ensure interactivity) | `perf_lock_typical_case_under_100ms` | PRD §1.3 lock p95 < 30s is the user-experience ceiling; project target is two orders of magnitude tighter. |
| Tauri (SQLite) | `visits::lock` worst case (10 adjustments + 10 item recomputes) | < 200 ms p99 | yes | `perf_lock_worst_case_under_200ms` | Matches §9 default; tests the fan-out's worst case. |
| Tauri (SQLite) | `visits::void` typical case (3 offsets + recomputes) | < 100 ms p99 | no | `perf_void_typical_case` | Void is structurally similar to lock minus receipt; tighter than §9. |
| Tauri (SQLite) | `visits::pricing_resolve` (read-only recompute) | < 20 ms p99 | no | `perf_pricing_resolve_under_20ms` | Read-only; no I/O; should be quick. |
| Tauri (SQLite) | `visits::lock_dryrun` (read-only validation set) | < 30 ms p99 | no | `perf_lock_dryrun_under_30ms` | Debounced 300ms on field changes; must be far faster than the debounce to feel responsive. |
| Tauri (Receipts) | A5 PDF render typical visit | < 500 ms p99 | -- (no §9 default) | `perf_a5_pdf_render_typical` | Receipts must feel snappy; users wait for it. Counts toward lock end-to-end. |
| Tauri (Receipts) | Thermal text render | < 50 ms p99 | -- | `perf_thermal_render` | Text only; trivial. |
| Tauri (IPC) | `visits::lock` full round-trip (Tauri serialize + Rust + commit + deserialize) | < 200 ms p99 | yes | `perf_lock_ipc_round_trip` | Matches §9 lock transaction SLO. |
| Sync engine | Drain a 700-op visit-fan-out backlog | >= 50 ops/sec | yes | `perf_outbox_drain_visit_fan_out` | §9 default. |
| Sync engine | Push a single visit-lock fan-out batch | < 2 s p95 | no (tighter than §9 push round-trip × 1 because batched) | `perf_push_visit_fan_out_batch` | Locks are user-visible; the sync indicator should clear quickly. |
| Sync server (Postgres) | `/sync/push` for a 50-op mixed batch (Visit + Adjustment + Patient) | < 200 ms p95 | yes | `perf_push_handler_50_op_mixed_batch` | §9 default. |
| Sync server (Postgres) | `/sync/pull` for a 100-row visits page including 7 name-snapshot cols + `pulled_at` | < 200 ms p95 | yes | `perf_pull_handler_100_visits` | §9 default. |
| Frontend | `<NewVisitForm>` cold paint (no cache) | < 400 ms | -- | `perf_new_visit_form_cold_paint` | One IPC + render; form has more state than `<ShiftsPage>`. |
| Frontend | `<NewVisitForm>` running-total recompute on field change | < 30 ms | -- | `perf_running_total_recompute` | Synchronous TS port of `money_math`; must feel instant. |
| Frontend | `<WorkspaceVisitsTable>` first paint with 50 rows | < 200 ms | -- | `perf_workspace_table_50_rows_cold_paint` | Includes the `<DirtyDot>` per row. |
| Reports (cross-phase forward-ref) | 12-month visit aggregate (P5 step 2) | < 4 s p95 | no (scaled from §9's 90-day < 1 s SLO) | `perf_12_month_visits_aggregate` | Owned by `performance-soak.md`; cross-referenced here. |

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI (`cargo test -p app_lib --lib` + `vitest run --project unit`).
- [ ] All §2 integration tests green in CI:
  - `cargo test --test visits_phase05 --test patients_phase05 --test inventory_adjustments_phase05`
  - IPC handler tests for all 22 commands listed in §2.2 (including the cross-phase-owned `shifts_lines_run_today`).
  - `pnpm --filter sync-server test -- sync/visits sync/patients sync/inventory-adjustments`
  - `vitest run --project integration`
- [ ] All §3 contract tests green in CI (`pnpm test:contract`). All snapshot files committed.
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- visits/ receipts/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P2 Mehdi the Receptionist** runs end-to-end and passes (record date / runner in row below).
- [ ] §6 all eight edge categories addressed (no empty subsections).
- [ ] §7 SLOs met for every row; override rows have a recorded rationale in the test source.
- [ ] Coverage gates met per §1.3:
  - [ ] `domains::visits::domain` >= 90%
  - [ ] `domains::visits::service` >= 90%
  - [ ] `domains::visits::money_math` >= 95%
  - [ ] `domains::visits::operator_eligibility` >= 90%
  - [ ] `domains::visits::infrastructure` >= 75%
  - [ ] `domains::patients::{domain,service}` >= 90%
  - [ ] `domains::patients::infrastructure` >= 75%
  - [ ] `domains::receipts` >= 85%
  - [ ] Frontend `src/features/visits/**`, `src/features/receipts/**`, `src/lib/money-math.ts`, `src/lib/schemas/{visit,patient,inventory}.ts`, `src/stores/draft-visit-store.ts` >= 90%
  - [ ] Frontend `src/pages/reception/**`, `src/components/reception/**`, `src/components/receipts/**` >= 60%
  - [ ] Sync server `domains/{patients,visits,inventory}/domain/**` >= 90%
  - [ ] Sync server conflict policy code (`sync/conflict/visit*.ts`) >= 95%
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed:
  - `expected/sync/patient-push-canonical.json.sha256`
  - `expected/sync/visit-push-locked-canonical.json.sha256`
  - `expected/sync/visit-push-voided-canonical.json.sha256`
  - `expected/sync/visit-pull-row-canonical.json.sha256`
  - `expected/sync/inventory-adjustment-push-canonical.json.sha256`
  - `expected/receipts/a5-locked-visit-en.pdf` (+ text-layer + bitmap hashes)
  - `expected/receipts/a5-locked-visit-ar.pdf` (+ text-layer + bitmap hashes)
  - `expected/receipts/thermal-locked-visit-en-32col.txt`
  - `expected/receipts/thermal-locked-visit-ar-48col.txt`
- [ ] `testing-status.md` row updated (Unit / Integration / Contract / E2E / Manual counts, Coverage %, Started / Completed dates, Open Defects).
- [ ] Lint, typecheck, build all green (`pnpm lint && pnpm build && cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd ../sync-server && pnpm lint && pnpm typecheck && pnpm test`).

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P2 Mehdi the Receptionist** | -- | -- | -- | -- |
| P4 Two-Device Conflict (reinforcement) | -- | -- | -- | Optional, exercises `visits` manual policy across two devices. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P05-G01 -- Void audit-first ordering (CRITICAL)

- **Source:** phase-05.md §7.11
- **Target test section:** §2.1 (`visits_phase05.rs`)
- **Category:** Missing Integration Test

`lock_writes_audit_first_then_business_then_outbox` covers the lock fan-out, but no scenario asserts that `visits::void` writes its audit rows BEFORE the `visits` UPDATE, the offsetting `inventory_adjustments` inserts, and the `inventory_items` recomputes. §7.11 requires audit-first ordering for every audit-emitting service. Until void carries the same assertion, a regression that flips void's order would land silently.

| Scenario | Asserts |
|-|-|
| `void_writes_audit_first_then_business_then_outbox` | Order assertion: instrument the void writer / inspect WAL frames; the `audit_log` INSERT for the visit-void action precedes the `visits` UPDATE (status -> voided), each offsetting `inventory_adjustments` INSERT, and each `inventory_items` recompute. Same audit-first invariant as the lock-side test, scaled to the void fan-out (1 visit + N offsets + N item recomputes, each preceded by its own audit row). Outbox rows enqueue last. |

### §9.2 P05-G02 -- Receipt print metrics events (HIGH)

- **Source:** phase-05.md §7.54
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.54 declares `receipt_print_ok` and `receipt_print_fail` events on the `metrics_events` table for every `ReceiptGenerator::render_pdf` / `render_thermal` invocation. §2.1 currently asserts `lock_start` / `lock_end` metrics but not the receipt print events. Without coverage, a future receipt-renderer rewrite could drop instrumentation undetected.

| Scenario | Asserts |
|-|-|
| `receipt_render_pdf_writes_print_ok_metric_on_success` | After `ReceiptGenerator::render_pdf` returns `Ok`, exactly one row exists in `metrics_events` with `name='receipt_print_ok'`, `correlation_id` matching the lock's correlation, and `format='pdf'`. |
| `receipt_render_thermal_writes_print_ok_metric_on_success` | Same as above with `format='thermal'`. |
| `receipt_render_pdf_writes_print_fail_metric_on_error` | Force `render_pdf` to `Err` via the `force-receipt-failure` feature flag; exactly one row exists with `name='receipt_print_fail'`, `format='pdf'`, and `reason` carrying the error code. Tx rollback still applies (per §7.16); the metrics row lives outside the tx. |
| `receipt_render_thermal_writes_print_fail_metric_on_error` | Mirror for thermal. |

### §9.3 P05-G03 -- Inline-patient outbox fan-out (HIGH)

- **Source:** phase-05.md §7.18
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.18 + lock step 6.7 require a single lock to enqueue outbox rows for every entity touched, including an inline-created `patients` row when `<NewVisitForm>` saves the visit with `patient.create()` followed by `visit.create_draft()` then `visit.lock`. No current scenario asserts that the patient outbox row co-lands with the visit, inventory-adjustment, and audit outbox entries from one lock transaction.

| Scenario | Asserts |
|-|-|
| `lock_with_inline_created_patient_enqueues_patient_visit_adjustments_audit_outbox_rows_in_one_tx` | Setup: create a draft visit whose `patient_id` references a brand-new `patients` row created in the same session (no prior outbox flush). Lock the visit. Assert `outbox` contains, after commit: 1 op for `patients` (create), 1 op for `visits` (lock), N ops for `inventory_adjustments` (consume), N audit ops, and that all share the same `correlation_id`. None enqueued before commit; all enqueued atomically. |

### §9.4 P05-G04 -- LockError tagged-union contract (HIGH)

- **Source:** phase-05.md §7.26
- **Target test section:** §3.2 / §3.3
- **Category:** Missing Contract Test

§7.26 defines `LockError` as a serde-tagged enum surfaced to the frontend and the i18n key registry. §3.2 currently covers `LockBlocker` (the read-only validation set) but not `LockError` (the lock-time runtime failure variants). Without a contract row, drift between the Rust variant set, the TS Zod schema, and the i18n keys can ship undetected.

| IPC command / surface | Rust struct | TS schema |
|-|-|-|
| `visits_lock` (error path) | `LockError` -- serde-tagged enum with variants `OperatorIneligible`, `OperatorBecameIneligible`, `PatientNameEmpty`, `ReceiptRenderFailed`, `DiskWriteFailed`, `AlreadyLocked`, `SubtypeRequired` (and any additional variants declared at implementation time per §7.26) | (NEW) `LockErrorSchema` -- discriminated union mirroring the Rust variants, each variant's `i18nKey` matching the server-side i18n key registry entry under `errors:visit.lock.*` and `errors:operator.ineligible.*`. Harness MUST diff: every Rust variant has a TS variant; every TS variant has an i18n key; every i18n key exists in both `en.json` and `ar.json`. |

### §9.5 P05-G05 -- Numerals module direct unit test (HIGH)

- **Source:** phase-05.md §7.46
- **Target test section:** §1.1
- **Category:** Missing Unit Test

The TS port has a direct unit test for digit-format conversion (§1.2 `format_visit_total_uses_arabic_digits_when_setting_true`), but the Rust `numerals` module is only exercised end-to-end via receipt-snapshot tests. A direct unit test for the lookup map and the locale gate is required for parity with the TS side and for the 90% line-coverage gate on `domains::visits::*` pure modules.

**`numerals` (`src-tauri/src/domains/visits/numerals.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `numerals::to_arabic_indic` | `maps_ascii_digits_0_through_9_to_arabic_indic_codepoints` | `'0'..='9'` -> `'\u{0660}'..='\u{0669}'` byte-for-byte across all 10 entries. |
| `numerals::to_arabic_indic` | `preserves_non_digit_characters` | `"IQD 1,234"` -> `"IQD ١,٢٣٤"`; comma, space, letters unchanged. |
| `numerals::to_arabic_indic` | `is_idempotent_on_already_arabic_indic_input` | `"١٢٣"` -> `"١٢٣"` (no double-mapping). |
| `numerals::format_with_locale` | `returns_ascii_when_arabic_numerals_setting_false` | `format_with_locale(1234, &settings { arabic_numerals: false, .. })` -> `"1234"`. |
| `numerals::format_with_locale` | `returns_arabic_indic_when_arabic_numerals_setting_true` | Same input with `arabic_numerals: true` -> `"١٢٣٤"`. Gate is read from settings, not hardcoded. |
| `numerals::format_with_locale` | `does_not_inspect_locale_only_setting_flag` | `locale='en'` + `arabic_numerals: true` -> Arabic-Indic. The setting is the sole gate per §7.46. |

### §9.6 P05-G06 -- Audit action enum and pruner ownership (MEDIUM)

- **Source:** phase-05.md §7.37
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.37 extends the audit action enum to include `lock` and `void` and declares an ownership invariant for the pruner. No current scenario asserts that the enum accepts the new actions, rejects unknown actions, or that the pruner respects the ownership invariant.

| Scenario | Asserts |
|-|-|
| `audit_log_accepts_lock_and_void_actions` | Insert `audit_log` rows with `action IN ('lock', 'void')` via the audit writer; both succeed; CHECK constraint passes. |
| `audit_log_rejects_unknown_action` | Insert `audit_log` row with `action='locked'` (typo) or `action='destroy'` -> SQLite CHECK violation. |
| `audit_pruner_preserves_lock_and_void_rows_within_retention_window` | Per §7.37 ownership invariant: pruner sweep over a fixture with `lock`/`void` rows aged inside the retention window leaves them intact; aged-out rows pruned; ownership keys (`actor_user_id`, `entity_id`, `entity_type`) preserved on retained rows. |

### §9.7 P05-G07 -- Client-side double-click Lock idempotency (MEDIUM)

- **Source:** phase-05.md §4 step 6.7 + §7.10
- **Target test section:** §6.5
- **Category:** Missing Edge Coverage

§6.5 currently covers SIGKILL, disk-full, and partial-write recovery, but no scenario asserts client-side idempotency when a receptionist double-clicks the Lock button and two near-simultaneous `visits::lock` IPC calls fire against the same draft. Server-side `op_id` replay (§2.3) covers the network path; the local-only race is not yet covered.

- **Double-click Lock on the same draft.** Two `visits::lock` IPC invocations fire within 50 ms against a single `draft_id` from the same session. Assert: exactly one transaction commits (status=`locked`, snapshots populated, adjustments + audit + outbox rows emitted once); the second invocation returns `LockError::AlreadyLocked` (per §7.26 variant) with the same `visit_id` payload and writes no additional rows. The UI side complements this with a button-disabled-while-pending invariant (per §7.10), asserted in component tests. Rust test: `lock_double_invocation_on_same_draft_commits_once_and_second_returns_already_locked`.

### §9.8 P05-G08 -- Server-side raw-SQL migration ordering (MEDIUM)

- **Source:** phase-05.md §7.51
- **Target test section:** §2.3 / §6.8
- **Category:** Missing Integration Test

§7.51 mandates that the server-side `inventory_adjustments_no_update_pg` trigger ship as a raw-SQL migration in a `005_*` file (matching the local SQLite migration number) and that `prisma migrate status` returns clean on a fresh + on a populated DB. No current scenario asserts the file's existence, its ordering, or the clean migrate-status invariant.

| Route / migration | Test | Asserts |
|-|-|-|
| (Migration file presence) | `phase05_server_migration_005_file_exists_with_raw_sql_trigger` | `sync-server/prisma/migrations/005_*` directory exists; `migration.sql` content includes `CREATE TRIGGER inventory_adjustments_no_update_pg` (or `CREATE OR REPLACE FUNCTION` per Postgres pattern) blocking non-sync-metadata updates on `inventory_adjustments`. |
| `prisma migrate status` | `phase05_prisma_migrate_status_returns_clean_on_fresh_db` | After applying all migrations through 005 on a fresh test DB, `pnpm prisma migrate status` exits 0 with "Database schema is up to date". |
| `prisma migrate status` | `phase05_prisma_migrate_status_returns_clean_on_populated_db` | Seed `clinical-day.sql`-equivalent rows on the server; re-run `prisma migrate status`; exits 0; no drift. |
| Trigger behaviour | `inventory_adjustments_pg_trigger_blocks_business_update` (§2.3) | UPDATE on `delta`, `item_id`, `visit_id`, `reason`, `by_user_id` -> trigger raises exception. UPDATE on `version`, `dirty`, `last_synced_at`, `origin_device_id` -> succeeds (sync-metadata carve-out). |

### §9.9 P05-G09 -- Shell capability scoping (MEDIUM)

- **Source:** phase-05.md §7.45
- **Target test section:** §3 / §6.7
- **Category:** Missing Persona / Manual Step

§7.45 restricts `shell:allow-execute` in `capabilities/main.json` to exactly two commands -- `lpstat -p` (Linux/macOS) and `wmic printer get name` (Windows) -- for the printer enumeration path. No current contract or security test validates the scope. A broader allowlist would expand the attack surface of the desktop binary.

- **Capability lint: shell:allow-execute scope.** Static-analysis test (Rust integration or pre-push CI step) reads `src-tauri/capabilities/main.json`, locates the `shell:allow-execute` permission entries, and asserts the allowlist contains EXACTLY two entries: `{ name: "lpstat", args: ["-p"] }` and `{ name: "wmic", args: ["printer", "get", "name"] }`. Any extra entry fails the test. Cross-references the manual step under §5.1 "Printer enumeration on each OS" -- the human run verifies that the scoped allowlist still functions on Linux / macOS / Windows. Rust test: `capability_shell_allow_execute_is_scoped_to_two_commands_only`.

### §9.10 P05-G10 -- PRD UI affordance component tests (MEDIUM)

- **Source:** phase-05.md §7.57
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.57 enumerates five PRD-mandated UI affordances -- `<ChecksGridHeader>`, `<WorkspaceHeader>`, `<NewVisitHeader>`, `<NewVisitActionsBar>`, `<VoidButton>` -- that have no component-level tests. §2.4 covers the form, modal, and table surfaces but skips these five header / action affordances. Without coverage, layout / role-gating regressions ship silently.

**RTL invariant applies (both `dir=ltr` and `dir=rtl`):**

| Hook / Component | Test | Asserts |
|-|-|-|
| `<ChecksGridHeader>` | `renders_eyebrow_rule_and_today_count_with_arabic_indic_digits_when_setting_true` | Eyebrow rule on the leading edge; today count uses `numerals::format_with_locale`; aria label localized via `reception.checks.header.*` keys. |
| `<WorkspaceHeader>` | `renders_filter_pills_in_paper_2_tray_with_active_pill_lifted_to_surface` | Per `.claude/rules/design-system.md` §5.6 filter-pills convention; active pill has hair-line shadow; non-active pills sit flat. |
| `<NewVisitHeader>` | `renders_check_type_name_snapshot_and_back_link_with_breadcrumb_i18n_key` | Header pulls `check_type.name_ar` / `name_en` per locale; back link points to `/reception/checks/:slug`; breadcrumb key `breadcrumbs.reception.new_visit`. |
| `<NewVisitActionsBar>` | `renders_save_draft_lock_and_discard_buttons_with_role_gated_visibility` | Save draft + Lock visible for receptionist; Discard visible only when draft has an `id`; all three carry `data-testid` per `.claude/rules/testing.md` §14. |
| `<NewVisitActionsBar>` | `lock_button_is_btn_primary_save_draft_is_btn_ink_discard_is_btn_ghost` | Per design-system §9 button variants; one primary, one ink, one ghost. |
| `<VoidButton>` | `hidden_for_receptionist_visible_for_superadmin` | Per §7.24 + §7.58 role gate. RTL mirroring asserted alongside. |
| `<VoidButton>` | `renders_as_btn_danger_with_crimson_outline_not_solid` | Per design-system §9 `btn-danger` variant; destructive-but-reversible affordance. |

### §9.11 P05-G11 -- Sync-server routes coverage gate (LOW)

- **Source:** phase-05.md §2 server routes
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§1.3 declares coverage gates for the sync-server domain layer (>= 90%) and conflict-policy code (>= 95%), but not for the sync-server routes layer. `.claude/rules/testing.md` §8 sets the default routes threshold at >= 85%. Without a phase-05 row, the gate is unenforced for the new visit / patient / inventory-adjustment route surfaces introduced via `/sync/push` and `/sync/pull` schema extensions.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `sync-server/src/app/sync/push/**`, `sync-server/src/app/sync/pull/**` (route handlers touching `patients`, `visits`, `inventory_adjustments`) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov --include="src/app/sync/push/**,src/app/sync/pull/**"` |
| P5 Year-End Audit step 2 (reinforcement) | -- | -- | -- | Optional, exercises 12-month aggregate -- gates `performance-soak.md` more than this plan. |

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). The format mirrors §9: `Target test section` names the existing §X.Y subsection that should absorb the new test rows during authoring. When Pass 3 re-runs, every gap below must show as covered.

### §10.1 P05-G12 -- `/inventory/*` route role guard (HIGH)

- **Source:** phase-05.md §7.58 + phase-06 §7.13 symmetric gate
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.58 declares the `/reception/*` outlet wrapped in `<RequireRole roles={['receptionist','superadmin']}>` and notes the `/inventory/*` wrapper is symmetric, owned by phase-06 §7.13 (`<RequireRole roles={['accountant','superadmin']}>`). The Pass 1 §9 additions covered `/reception/*` role gating via the `reception-route-role-guard` E2E, but no scenario asserts the symmetric `/inventory/*` gate at the phase-05 surface (the receptionist persona that owns `/reception/*` must NOT see `/inventory/*`). Without coverage, a wrapper regression that opened `/inventory/*` to receptionists would ship silently and only fail in phase-06's plan, after release.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `inventory-route-role-guard-rejects-receptionist.e2e.ts` | Mehdi (`receptionist`) | 1) Log in. 2) Type `/inventory` into the address bar. 3) Press Enter. 4) Repeat for `/inventory/items`, `/inventory/items/<seeded-id>`, `/inventory/adjust`. | Each navigation redirects to `/no-access` (the `<RequireRole>` fallback). Body renders the `no_access.title` i18n key copy in the active locale; no `/inventory/*` UI ever paints. `<UserMenu>` does NOT list an Inventory link for this persona. Per §7.58 symmetric clause + phase-06 §7.13. |

### §10.2 P05-G13 -- visits manual-policy 4-step algorithm coverage (HIGH)

- **Source:** phase-05.md §7.40 `visits` manual semantics
- **Target test section:** §3.3 / §2.3
- **Category:** Missing Contract Test

§7.40 specifies the four-step server-side `accept_push` algorithm for `visits`: (1) ProcessedOp.has -> cached, (2) absent existing -> INSERT, (3) lower-version push -> park conflict, (3') equal-version + snapshot diff -> park conflict, (4) higher-version push -> accept. §3.3 currently covers the `visits` policy declaration row but not the algorithm's four branches as a single covered matrix. Without a matrix test, a regression that flipped step 3's "lower OR equal+diff" condition to "lower only" would silently accept conflicting equal-version pushes.

| Scenario | Asserts |
|-|-|
| `accept_push_visit_absent_inserts_new_row` | Push a Visit whose id does not exist server-side -> 200 + row persisted with the pushed snapshots, `version` from payload, `pulledAt` null. No conflict. |
| `accept_push_visit_lower_version_parks_conflict` | Seed server Visit at `version=3`. Push Visit with same id and `version=2` -> 409 ConflictParked; row in `parked_conflicts` carries `kind='visit_version_lower'`; server row unchanged. |
| `accept_push_visit_equal_version_with_snapshot_diff_parks_conflict` | Seed server Visit at `version=3` with `patient_name_snapshot='A'`. Push same id, `version=3`, `patient_name_snapshot='B'` -> 409 ConflictParked; `kind='visit_snapshot_diff'`; server row unchanged. |
| `accept_push_visit_higher_version_accepts_latest_legal_transition` | Seed server Visit `status=draft`, `version=3`. Push same id, `status=locked`, `version=4`, full lock snapshot block -> 200; row updated; `lockedAt` populated. |
| `accept_push_visit_equal_version_identical_snapshots_is_idempotent_noop` | Seed server Visit. Push same id, same `version`, byte-identical snapshots -> 200; ProcessedOp recorded; no UPDATE issued (assert via `pg_stat_user_tables` or audit-log absence). |

### §10.3 P05-G14 -- Sync-apply per-entity validator hook for visits (HIGH)

- **Source:** phase-05.md §7.3 cross-table invariants 2-5
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.3 mandates that `SyncEngine::apply_pull` invokes a per-entity validator hook for `visits` that re-checks invariants 2-5 (subtype required iff parent `has_subtypes = 1`; `dye = 1` requires `dye_supported = 1`; `report = 1` requires `report_supported = 1`) and rejects malicious or buggy pulls. Existing §2.1 covers the lock-time validator and the server-side `accept_push` re-check, but no integration test asserts the local pull path: a tampered server-emitted row that violates an invariant must NOT land in local SQLite. Without coverage, a compromised server or man-in-the-middle could inject invalid visits and bypass the local invariant set.

| Scenario | Asserts |
|-|-|
| `apply_pull_rejects_visit_with_subtype_when_parent_has_subtypes_zero` | Seed a `check_types` row with `has_subtypes=0`. Push a pulled Visit referencing this check and a non-null `check_subtype_id` through the `SyncEngine::apply_pull` validator hook. Result: row REJECTED; no `visits` INSERT; an entry written to the apply-pull error log with `kind='invariant_violation'`, `invariant='subtype_disallowed'`. |
| `apply_pull_rejects_visit_with_dye_one_when_check_dye_supported_zero` | Seed `check_types` with `dye_supported=0`. Push a pulled Visit with `dye=1` -> rejected with `invariant='dye_not_supported'`. |
| `apply_pull_rejects_visit_with_report_one_when_check_report_supported_zero` | Seed `check_types` with `report_supported=0`. Push a pulled Visit with `report=1` -> rejected with `invariant='report_not_supported'`. |
| `apply_pull_rejects_visit_missing_required_subtype` | Seed `check_types` with `has_subtypes=1`. Push a pulled Visit with `check_subtype_id=NULL` -> rejected with `invariant='subtype_required'`. |
| `apply_pull_accepts_visit_satisfying_all_invariants` | Positive control: a pulled Visit consistent with its `check_type` row applies cleanly; row INSERT-ed; no error-log entry. |

### §10.4 P05-G15 -- Server-side name-snapshot CHECK enforcement on push (HIGH)

- **Source:** phase-05.md §7.17 + §7.52 name-snapshot columns
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.17 + §7.52 add name-snapshot columns to both local SQLite (with the §7.53 CHECK constraint enforcing non-null on `status='locked'`) and to the server-side Prisma `Visit` model. The existing §2.1/§6.8 tests cover the local SQLite CHECK by raw-INSERT, but no test asserts that the server's `accept_push` rejects a `status='locked'` push whose name-snapshot columns are null. A malicious or buggy client could bypass the local CHECK (e.g. by skipping the CHECK migration) and rely on the server's defence-in-depth; without that defence, the server-side reprint would render empty names.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_rejects_locked_visit_with_null_patient_name_snapshot` | Push a Visit `{ status: 'locked', version: 4, patient_name_snapshot: null, doctor_name_snapshot: 'Dr X', operator_name_snapshot: 'Op Y', check_type_name_ar_snapshot: '...' }` -> 422 ValidationError; payload error path `patient_name_snapshot`; row not persisted. Per §7.17 + §7.53. |
| `POST /sync/push` | `push_rejects_locked_visit_with_null_operator_name_snapshot` | Mirror for `operator_name_snapshot=null` -> 422; row not persisted. |
| `POST /sync/push` | `push_rejects_locked_visit_with_null_check_type_name_ar_snapshot` | Mirror for `check_type_name_ar_snapshot=null` -> 422; row not persisted. |
| `POST /sync/push` | `push_rejects_locked_visit_with_doctor_id_set_but_doctor_name_snapshot_null` | Push `{ status:'locked', doctor_id: <uuid>, doctor_name_snapshot: null }` -> 422 (`doctor_name_snapshot` must be non-null when `doctor_id` is non-null per §7.53 iff-clause). |
| `POST /sync/push` | `push_accepts_locked_visit_with_all_required_snapshots_populated` | Positive control: a `status='locked'` Visit with every required snapshot non-null persists with 200. |

### §10.5 P05-G16 -- `<DraftStaleBanner>` shared base component (HIGH)

- **Source:** phase-05.md §7.42 `<SettingsChangedBanner>` shared `<DraftStaleBanner>` base
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.42 explicitly says `<PricingChangedBanner>` (§7.27) and `<SettingsChangedBanner>` (§7.42) "both share a `<DraftStaleBanner>` base." The §9 Pass 1 §9.10 row covers the affordance test for each banner individually, but no test asserts the structural promise that both banners render through the SAME base component. Without coverage, a refactor that diverged the two banners would silently stack two banners on top of each other when both events fired (pricing edit + settings edit on the same draft), creating a confusing UX where dismissing one leaves the other.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DraftStaleBanner>` | `pricing_and_settings_banners_render_through_same_base_component` | Render `<NewVisitForm>` with mocked IPC. Fire `catalog:pricing_changed` -> assert exactly ONE `<DraftStaleBanner>` element in the DOM (queried by `data-testid="draft-stale-banner"`); inspect its `data-variant` -> `'pricing'`. Fire `settings:changed` (without dismissing the first) -> still exactly ONE `<DraftStaleBanner>` element; `data-variant` updates to `'settings'` (or aggregates; see next row). NEVER two stacked banner roots. |
| `<DraftStaleBanner>` | `dismissing_banner_clears_until_next_event_arrival` | Render with a `pricing` event active; click dismiss; assert the banner unmounts. Fire a fresh `catalog:pricing_changed` event -> banner remounts with `data-variant='pricing'`. Dismiss state is event-scoped, not session-scoped. Mirrors §10.10 below. |
| `<PricingChangedBanner>` and `<SettingsChangedBanner>` | `both_components_render_through_DraftStaleBanner_root` | Render each banner standalone with its respective props; assert each contains a child element matching `data-testid="draft-stale-banner"`. The two wrappers exist for typed i18n keys but defer the chrome to the shared base. |

### §10.6 P05-G17 -- `useLinesRunToday` cache TTL and `visits:locked` invalidation (HIGH)

- **Source:** phase-05.md §7.50 lines-run handshake
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.50 spells out the wiring: `<ShiftHistoryToday>` consumes `shifts::lines_run_today(operator_id)` per row via React Query keys `['shifts','lines_run', operator_id]`, with a 30-second cache TTL, invalidated explicitly on `visits:locked` event. §2.4 covers some mutation flows but no test asserts (a) the declared TTL value, (b) the event listener invalidates the matching key, or (c) un-related operator rows are NOT invalidated. Without coverage, a regression that dropped the listener would freeze the lines-run column on stale data until the next refetch interval.

| Hook / Component | Test | Asserts |
|-|-|-|
| `useLinesRunToday` | `query_is_configured_with_30_second_stale_time` | Inspect the React Query observer for `['shifts','lines_run', operatorId]`; `staleTime === 30_000` (ms). Source-of-truth assertion; not a behaviour test. |
| `useLinesRunToday` | `visits_locked_event_invalidates_only_matching_operator_key` | Mount `<ShiftHistoryToday>` with two operator rows (op-A, op-B). Both queries settle. Dispatch a `visits:locked` Tauri event with payload `{ operator_id: 'op-A' }`. Assert: query for `['shifts','lines_run','op-A']` re-fetches (mocked IPC call count increments); query for `['shifts','lines_run','op-B']` does NOT re-fetch (call count unchanged). Per §7.50 explicit invalidation path. |
| `useLinesRunToday` | `cache_serves_within_ttl_window_without_event` | Mount `<ShiftHistoryToday>`; let queries settle. Unmount + remount within 30s WITHOUT firing `visits:locked`. Assert no additional IPC call (cache hit). Re-mount after 30s+ -> refetch. |

### §10.7 P05-G18 -- `numerals` reads live `settings_cache` (HIGH)

- **Source:** phase-05.md §7.46 Rust receipt arabic-numerals formatter
- **Target test section:** §1.1 / §2.1
- **Category:** Missing Integration Test

§7.46 says `numerals::format_iqd` / `format_int` read `settings.arabic_numerals` "from the `settings_cache`". Pass 1 §9.5 covers the lookup map and the locale gate via direct unit tests that pass an in-memory settings struct, but no test asserts the receipt-render path actually consults the LIVE settings cache (not a stale closure captured at app boot). Without coverage, a regression that snapshotted the setting at startup would render Western digits on every receipt printed after a setting toggle until the app restarted.

| Scenario | Asserts |
|-|-|
| `receipt_render_uses_settings_cache_value_when_arabic_numerals_toggled_mid_session` | Boot the app with `arabic_numerals=false`. Lock a visit V1; assert its persisted A5 PDF text layer renders the total with ASCII digits. Toggle the setting via `settings::update { arabic_numerals: true }`, which updates the `settings_cache`. Lock a second visit V2 (no app restart). Assert V2's receipt renders Arabic-Indic digits. Crucially: `ReceiptGenerator::render_pdf` re-reads the cache for each render; the value is NOT closed over at construction time. Per §7.46. |
| `receipt_render_uses_settings_cache_value_when_arabic_numerals_toggled_back_off_mid_session` | Same fixture with the toggle going `true -> false` between two locks; second receipt back to ASCII. Confirms the gate is symmetric and live. |

### §10.8 P05-G19 -- Platform-appropriate printer-enumeration command (HIGH)

- **Source:** phase-05.md §7.45 + §7.23
- **Target test section:** §1.1 / §6.7
- **Category:** Missing Unit Test

§7.45 says the printer enumeration "implementation chooses the platform-appropriate command at runtime" (`lpstat` on Linux/macOS, `wmic` on Windows). Pass 1 §9.9 covers the capability allowlist scope, but no unit test asserts the dispatch function picks the correct binary per `cfg!(target_os)`. Without coverage, a regression that hard-coded `lpstat` everywhere would silently break printer enumeration on Windows installs.

| Module | Test | Asserts |
|-|-|-|
| `settings::printer_enum::platform_command` | `selects_lpstat_with_dash_p_on_linux` | With a stubbed `cfg!(target_os = "linux")` (or invoking the function compiled-in on Linux runners), the returned `(cmd, args)` tuple is exactly `("lpstat", &["-p"])`. |
| `settings::printer_enum::platform_command` | `selects_lpstat_with_dash_p_on_macos` | Same as above on `target_os = "macos"`. |
| `settings::printer_enum::platform_command` | `selects_wmic_printer_get_name_on_windows` | On `target_os = "windows"`, returns `("wmic", &["printer", "get", "name"])`. |
| `settings::printer_enum::parse_output` | `parses_lpstat_output_into_vec_of_printer_info` | Feed canonical `lpstat -p` output (`printer Brother_QL740 is idle.  enabled since ...`); returns `[PrinterInfo { name: "Brother_QL740", is_default: false }, ...]`. |
| `settings::printer_enum::parse_output` | `parses_wmic_output_into_vec_of_printer_info` | Feed canonical `wmic printer get name` output (CRLF-separated header + names); returns the same `Vec<PrinterInfo>` shape. |

### §10.9 P05-G20 -- Boot sweeper 5-minute age threshold (MEDIUM)

- **Source:** phase-05.md §7.16 receipt-temp-dir cleanup
- **Target test section:** §6.5
- **Category:** Missing Edge Coverage

§7.16 declares that lock writes use a temp directory with atomic rename and a boot sweeper to clean up orphans. The Pass 1 §9 row covers the cleanup-on-crash invariant, but no test asserts the explicit 5-minute age threshold that distinguishes "in-flight lock from a still-alive process" from "orphan from a prior crash". Without coverage, a regression that swept aggressively (0-second threshold) would delete a concurrent lock's in-flight artifacts.

| Scenario | Asserts |
|-|-|
| `boot_sweeper_skips_temp_files_younger_than_5_minutes` | Seed `$APPDATA/.../receipts/.tmp/` with a file `mtime = now - 4m 59s`. Boot the app; let the sweeper run. Assert: file STILL EXISTS post-sweep (under the 5-min threshold). Per §7.16. |
| `boot_sweeper_removes_temp_files_older_than_5_minutes` | Seed a tmp file with `mtime = now - 5m 1s`. Boot. Assert: file REMOVED; sweep recorded one cleanup in the boot log. |
| `boot_sweeper_boundary_at_exactly_5_minutes_is_inclusive_or_exclusive_per_spec` | Seed a tmp file with `mtime = now - 5m 0s`. Behaviour matches the boundary declared in §7.16 (the implementation MUST be deterministic at the boundary; the test pins whichever side the spec picks and prevents drift). |

### §10.10 P05-G21 -- Banner dismiss state resets on next event (MEDIUM)

- **Source:** phase-05.md §7.27 `<PricingChangedBanner>` "Dismissable until the next event"
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.27 ends with "Dismissable until the next event" -- the banner's hidden state must reset when a fresh `catalog:pricing_changed` event arrives. Pass 1 §9.10 covers the banner's initial render but not the post-dismiss re-render on a second event. Without coverage, a regression that persisted the dismiss state across events would suppress the banner for the rest of the session, hiding stale-pricing warnings.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<PricingChangedBanner>` | `dismiss_then_next_pricing_event_remounts_banner` | Render `<NewVisitForm>` with an active draft. Fire `catalog:pricing_changed` -> banner visible. Click dismiss -> banner hidden. Fire a fresh `catalog:pricing_changed` -> banner visible again (NEW dismissable instance). Two consecutive events without dismiss between -> still one visible banner (no double-mount). Per §7.27. |
| `<SettingsChangedBanner>` | `dismiss_then_next_settings_event_remounts_banner` | Mirror for `<SettingsChangedBanner>` driven by `settings:changed`. Per §7.42 (which states "identical mechanism"). |

### §10.11 P05-G22 -- `thermal_printer_name=null` prompts on first print (MEDIUM)

- **Source:** phase-05.md §7.23 + §4 receipt routing
- **Target test section:** §2.2 / §4
- **Category:** Missing Integration Test

§7.23 declares the `thermal_printer_name` setting defaults to `null`, with "null -> user prompt at first print". No current test exercises the first-print path: when a receptionist locks a visit and `thermal_printer_name IS NULL`, the IPC must NOT silently send bytes to a default printer; it must surface a picker dialog backed by `settings::list_printers`. Without coverage, a regression that fell back to the OS default would print to the wrong device.

| Scenario | Asserts |
|-|-|
| `receipts_print_thermal_with_null_setting_invokes_picker_and_persists_choice` | Seed settings with `thermal_printer_name=null`. Invoke `receipts::print_thermal { visit_id }`. Assert: no bytes sent to any printer; the IPC returns a `PrinterPickRequired` typed variant (or emits a `receipts:printer_pick_required` event the frontend consumes); the frontend test then simulates the user picking "Brother_QL740" through `<PrinterPickerDialog>`; `settings::update { thermal_printer_name: 'Brother_QL740' }` is called; the second `receipts::print_thermal` call sends bytes to that printer. Per §7.23. |
| `receipts_print_thermal_with_empty_string_uses_os_default_printer` | Seed settings with `thermal_printer_name=""`. Invoke `receipts::print_thermal`; assert bytes routed to the OS default printer (per §7.45 "empty string means OS default"); no picker. |
| `receipts_print_thermal_with_set_value_routes_to_named_printer` | Seed `thermal_printer_name="Star_TSP100"`; invoke; assert bytes routed to "Star_TSP100"; no picker. |

### §10.12 P05-G23 -- `<ChecksGridCard>` subtype order by FTS recent-usage (MEDIUM)

- **Source:** phase-05.md §7.20 sample subtype list ordering
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.20 specifies the card renders "up to 3 subtype names by FTS recent-usage order + '+N more' overflow." Existing §2.4 covers the overflow count (the `+N more` text) but no test asserts the ordering -- a regression that fell back to alphabetical sort would still produce three names and the same overflow text, hiding the bug. Recent-usage ordering is the operator-facing affordance: the subtypes they used today should surface first.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<ChecksGridCard>` | `samples_first_three_subtypes_by_fts_recent_usage_descending` | Seed a `check_types` row with 5 subtypes (S1..S5). Seed `visits` such that recent-usage rank is `S3 > S1 > S5 > S2 > S4` (locked visits today/yesterday). Render the card; assert the rendered sample list reads `S3, S1, S5` (in that order) followed by `+2 more`. NOT alphabetical (`S1, S2, S3`). Per §7.20 + the FTS recency hook. |
| `<ChecksGridCard>` | `samples_fewer_than_three_when_only_two_subtypes_used_recently` | When only S1 and S3 have recent usage, render `S3, S1` with NO `+N more` overflow (overflow only when total subtypes > 3, regardless of usage). |
| `<ChecksGridCard>` | `samples_no_list_when_check_has_subtypes_zero` | Seed `check_types` with `has_subtypes=0`; render the card -> no sample list element renders. Per §7.20 "when `has_subtypes = 1`" gate. |

### §10.13 P05-G24 -- `visits::list_workspace` cursor encoding round-trip (MEDIUM)

- **Source:** phase-05.md §7.21 + §7.44 cursor `(created_at, id)`
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.21 + §7.44 specify cursor pagination "encodes (created_at, id)" with `ORDER BY created_at DESC, id DESC`. Existing §2.1 tests check pagination output (50 rows + a next cursor token), but no test asserts the cursor is a stable base64-encoded `(created_at, id)` pair with a clean decoded round-trip. Without coverage, a switch to opaque server-stamped cursors (or a JSON cursor that includes mutable fields) would silently break deep-paginating across sessions.

| Scenario | Asserts |
|-|-|
| `list_workspace_cursor_decodes_to_stable_created_at_and_id_pair` | Seed 60 visits with deterministic `(created_at, id)` values. Page 1: call `visits::list_workspace { limit: 50 }`; capture `next_cursor`. Assert: base64-decode the cursor yields a 2-tuple matching the 50th row's `(created_at, id)` (or 51st depending on the off-by-one convention -- pin whichever the implementation uses). |
| `list_workspace_cursor_round_trip_returns_next_page_starting_from_51st_row` | Use the captured cursor; call again with `cursor=<token>, limit: 50`; assert the first row of page 2 is row 51 (zero-indexed: row 50) and the last row is row 60; `next_cursor=None`. |
| `list_workspace_cursor_is_stable_across_concurrent_inserts` | Capture page-1 cursor. Insert a new visit at `created_at = NOW()` (which sorts FIRST since DESC). Re-page using the captured cursor; assert the page-2 rows are still rows 51..60 of the original ordering (the cursor encodes a fixed `(created_at, id)` boundary, not an offset). |

### §10.14 P05-G25 -- `visits_drafts` partial index is used (MEDIUM)

- **Source:** phase-05.md §7.5 local index for draft listing
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.5 adds `CREATE INDEX visits_drafts ON visits(entity_id, check_type_id, created_at DESC) WHERE status='draft' AND deleted_at IS NULL`. Existing §2.1 tests check the IPC's row output but never run `EXPLAIN QUERY PLAN` to confirm the partial index is actually hit. Without coverage, a query rewrite that dropped the partial-index WHERE-clause from the SQL would silently full-scan, only failing at scale.

| Scenario | Asserts |
|-|-|
| `list_drafts_by_check_uses_visits_drafts_partial_index` | Seed 1k visits across multiple checks and statuses. Invoke `visits::list_drafts_by_check { check_type_id }`. Capture the prepared statement and run `EXPLAIN QUERY PLAN` against the same SQL. Assert the plan output contains `SEARCH visits USING INDEX visits_drafts` (or SQLite's equivalent phrasing for partial-index usage); does NOT contain `SCAN visits`. Per §7.5 + §7.7. |
| `list_workspace_with_status_draft_filter_uses_visits_drafts_index` | Same `EXPLAIN QUERY PLAN` assertion for `visits::list_workspace` with `filters.statuses = ['draft']`. Per §7.21 + §7.5 partial-index alignment. |

### §10.15 P05-G26 -- Server Prisma `Visit` `@@index` rows exist (MEDIUM)

- **Source:** phase-05.md §7.4 + §3.3 / §6.6
- **Target test section:** §3.3 / §6.6
- **Category:** Missing Contract Test

§7.4 declares three required `@@index` rows on the server Prisma `Visit` model: `@@index([entityId, status, lockedAt])`, `@@index([entityId, patientId])`, `@@index([entityId, checkTypeId, status])`. No current test introspects the live database to confirm all three indexes actually exist. Without coverage, a future schema edit that dropped or renamed one of them would silently full-scan server-side reports at production scale.

| Scenario | Asserts |
|-|-|
| `phase05_server_visit_model_has_three_required_indexes` | Connect a Prisma test DB to a populated schema after all migrations through 005 apply. Query `pg_indexes` (or run `\d "Visit"` equivalent) for the `Visit` table. Assert the index set contains AT LEAST: one index on `(entity_id, status, locked_at)`, one on `(entity_id, patient_id)`, and one on `(entity_id, check_type_id, status)` -- order, partial-ness, and name match what Prisma generates from the `@@index` rows in §7.4. |
| `phase05_report_query_uses_entity_status_lockedat_index` | Run `EXPLAIN ANALYZE` on the canonical reports query (`SELECT * FROM "Visit" WHERE entityId=? AND status='locked' AND lockedAt BETWEEN ? AND ?`); assert the plan uses an index scan on the matching index, not a sequential scan. Per §7.4 rationale. |

### §10.16 P05-G27 -- `created_at` immutability across full void flow (LOW)

- **Source:** phase-05.md §7.39 visit `created_at` immutability
- **Target test section:** §6.8
- **Category:** Missing Integration Test

§7.39 says `created_at` is immutable across `lock` and `void` (and across sync round-trips). A pure unit test on the entity guard exists, but no integration scenario snapshots `created_at` before/after a full DB-backed void flow that includes the audit row, the offsetting adjustments, and the outbox enqueue. Without coverage, a writer that "touched" `created_at` during the void update (for example, via an ORM `setUpdatedAt` hook misapplied to `created_at`) would silently shift the value.

| Scenario | Asserts |
|-|-|
| `void_full_flow_preserves_created_at_byte_for_byte` | Seed a locked visit with `created_at = '2026-05-13T08:14:22.123Z'`. Capture the raw bytes of the `created_at` column (e.g. via a `SELECT created_at, CAST(created_at AS BLOB)` if SQLite, or its serialization). Run the full void flow: `visits::void { visit_id, reason: 'wrong patient was used' }`. Re-capture `created_at`. Assert byte-for-byte equality. The `updated_at` MUST have advanced (sanity-check the test isn't passing on a no-op). |
| `void_full_flow_preserves_created_at_after_sync_round_trip` | Same as above but after pushing the void and pulling it back; the round-tripped row's `created_at` still matches the original. Catches any server-side rewrite of the field. |

### §10.17 P05-G28 -- Document Center deferral verified (LOW)

- **Source:** phase-05.md §7.55 Document Center deferral
- **Target test section:** §5.1
- **Category:** Manual Step

§7.55 says "No Document Center upload -- receipts are local-only artifacts under `$APPDATA/idc-system/receipts/...`. Horizon-1 introduces the centralized receipt archive." No manual or contract step verifies this negative invariant: that locking a visit produces NO outgoing HTTP request to any Document Center endpoint or upload queue. A regression that silently uploaded receipts to a stubbed Horizon-1 endpoint would breach the local-only commitment without any test catching it.

| Script step (added to §5.1) | Manual action | Pass criteria |
|-|-|-|
| "Document Center stays out of scope" | 1) Start the desktop binary with the sync server reachable. 2) Open DevTools network panel (or run with `RUST_LOG=tauri_plugin_http=trace`). 3) Lock a visit through the receptionist flow. 4) Inspect the captured network traffic for the 60s window covering the lock. | NO request fires to any path containing `/documents`, `/document-center`, `/uploads`, `/receipts/archive`, or any S3/storage-gateway URL. The only outbound HTTP traffic is the normal `/sync/push` for the visit and its dependents. Per §7.55. The captured trace is attached to the persona run report. |

### §10.18 P05-G29 -- `metrics_events` retention cross-reference (LOW)

- **Source:** phase-05.md §7.54 + phase-08 §7.21 retention policy
- **Target test section:** §6.5 / §8
- **Category:** Missing Coverage Gate

§7.54 says `metrics_events` emissions from phase-05 (`lock_start`, `lock_end`, `receipt_print_ok`, `receipt_print_fail`) "share the WAL pool, and use the same retention as phase-08 §7.21." No phase-05 test or DoD check asserts the retention/pruner cross-reference is actually wired -- that the phase-08 pruner sees and prunes the phase-05 event rows under the declared policy. Without coverage, phase-05 emissions could accumulate indefinitely until phase-08 lands and someone notices.

| DoD additions to §8 | Asserts |
|-|-|
| `[ ] §7.54 metrics emissions covered by phase-08 §7.21 pruner` | A cross-phase test or fixture in the phase-05 plan loads the `clinical-day.sql` fixture, runs N lock + receipt-print cycles emitting `lock_start`/`lock_end`/`receipt_print_*` rows, advances the wall clock past the phase-08 §7.21 retention threshold, runs the phase-08 pruner (imported as a library call from phase-05's test harness; the pruner module is implemented in phase-08 but its API contract is testable from anywhere), and asserts the phase-05 event rows are pruned. The DoD checkbox stays unchecked until phase-08 lands; the test row is added now so the gate is visible in the testing-status table. |
| `metrics_events_phase05_rows_share_retention_with_phase08_emissions` (Rust integration, deferred to land alongside phase-08 §7.16 soak harness) | Same fixture; assert pruner removes phase-05 rows AND phase-08 rows in one sweep with identical age thresholds; no phase-05-specific retention carve-out. Per §7.54 explicit cross-reference. |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 12 Phase-05 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P05-G30 through P05-G41). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P05-G30 -- Server visit-push audit-first ordering (HIGH)

- **Source:** phase-05.md §4 Sync Server Visit push acceptance step 4 -- "Insert audit row in same Prisma transaction".
- **Target test section:** §2.3
- **Category:** Missing Integration Test

Rust-side audit-first (§2.1 `lock_writes_audit_first_then_business_then_outbox`) is asserted but the Prisma-side equivalent on `/sync/push` is silent.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `server_visit_push_writes_audit_row_in_same_tx_before_upsert` | Push a `visits` payload with a state transition (e.g. draft -> locked). On success: query `audit_log` AND `visits` from the same Prisma client; assert the audit row exists with `action='lock'`, `entity='visits'`, `entity_id` matching the push id, `at` <= the visit row's `updated_at` (ordering proof). Inject an upsert failure mid-tx (e.g. CHECK violation on a name-snapshot column); assert neither the audit row NOR the visits update persist (atomicity proof). Per §4 server step 4. |

### §11.2 P05-G31 -- TENANT_MODELS additions (patients, visits, inventory_adjustments) (HIGH)

- **Source:** phase-05.md §5 Infrastructure -- "Add `patients`, `visits`, `inventory_adjustments` to TENANT_MODELS".
- **Target test section:** §3.3
- **Category:** Missing Coverage Gate

A regression dropping one from the registry silently breaks tenant isolation on push/pull for that entity.

| Scenario | Asserts |
|-|-|
| `tenant_models_registry_contains_phase05_additions` | Import `TENANT_MODELS` from `sync-server/src/app/sync/tenant-models.ts`. Assert the array contains: `'patients'`, `'visits'`, `'inventory_adjustments'`. For each name, run a cross-tenant push attempt (JWT with `entityId='A'` POSTing a body carrying `entity_id='B'`) and assert the row lands under tenant A (the registry enforces JWT-injection per P03-G32 pattern). |

### §11.3 P05-G32 -- LockBlocker 7-variant exhaustive lock_dryrun coverage (HIGH)

- **Source:** phase-05.md §7.38 -- `LockBlocker` enum with 7 variants.
- **Target test section:** §2.1
- **Category:** Missing Edge Coverage

`lock_dryrun_returns_all_blockers_for_invalid_draft` exercises 3 of 7. The other 4 paths are untested through `visits::lock_dryrun`.

| Scenario | Asserts |
|-|-|
| `lock_dryrun_surfaces_dye_not_supported_variant` | Seed a draft with `dye_required=true` for a check_type with `dye_supported=0`. Call `visits::lock_dryrun`. Assert returned blockers contain `LockBlocker::DyeNotSupported { check_type_id }`. |
| `lock_dryrun_surfaces_report_not_supported_variant` | Mirror with `report_required=true` against a check_type with `report_supported=0`. Expect `LockBlocker::ReportNotSupported`. |
| `lock_dryrun_surfaces_no_shift_for_check_type_variant` | Seed a draft with operator O on check_type CT where no `operator_shifts` row covers (O, CT) today. Call lock_dryrun. Expect `LockBlocker::NoShiftForCheckType { operator_id, check_type_id }`. |
| `lock_dryrun_surfaces_draft_stale_variant` | Seed a draft whose `updated_at` < the latest `catalog:pricing_changed` or `settings:changed` event. Call lock_dryrun. Expect `LockBlocker::DraftStale { last_event_at }`. |

### §11.4 P05-G33 -- visits:locked event emitted ONLY after commit (MEDIUM)

- **Source:** phase-05.md §7.50 -- "emitted by VisitService::lock after commit".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§9.x covers the happy-path emission; no test asserts the event is NOT fired on rollback.

| Scenario | Asserts |
|-|-|
| `visit_locked_event_not_emitted_when_lock_tx_rolls_back` | Subscribe to `visits:locked` events. Seed a draft; force a mid-tx failure (e.g. drop `audit_log` table after step 1, or inject a disk-write failure in `ReceiptGenerator::render_pdf`). Call `visits::lock`; expect Err. Assert the event subscription received ZERO `visits:locked` events. The event MUST emit AFTER commit, not before -- a regression that emitted early would mis-fire on rolled-back locks. Per §7.50. |

### §11.5 P05-G34 -- (Draft, Voided) MustLockFirst illegal transition (MEDIUM)

- **Source:** phase-05.md §7.32 -- illegal transition `(Draft, Voided)` returns `Visit::MustLockFirst`.
- **Target test section:** §2.1
- **Category:** Missing Edge Coverage

`(Locked, Draft)`, `(Locked, Locked)`, re-void `(Voided, Voided)` covered; `(Draft, Voided)` is not.

| Scenario | Asserts |
|-|-|
| `void_rejects_draft_with_must_lock_first` | Seed a draft visit (status='draft'). Call `visits::void { id, reason: 'cancellation' }`. Assert returned error matches `Visit::MustLockFirst`. Verify the visits row unchanged: `status='draft'`, `version` unchanged, no audit row written, no outbox row. Per §7.32 illegal transition matrix. |

### §11.6 P05-G35 -- NewVisitForm conditional create vs update dispatch (MEDIUM)

- **Source:** phase-05.md §4 Frontend NewVisitForm step 6 -- "dispatches visits::create_draft OR visits::update_draft".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<NewVisitForm>` | `save_draft_dispatches_create_for_new_draft_and_update_for_existing` | Mount `<NewVisitForm>` with `draft={ id: undefined, ... }`. User fills fields; clicks `[Save draft]`. Assert IPC mock receives `visits_create_draft { payload }` (NOT update). Mount again with `draft={ id: 'visit-123', ... }` (existing draft). Click Save. Assert IPC receives `visits_update_draft { id: 'visit-123', patch }` (NOT create). The branch decision is at the form layer, NOT inside `useVisitCreate`/`useVisitUpdate`. Per §4 frontend step 6. |

### §11.7 P05-G36 -- DyeReportToggles disabled state + tooltip (MEDIUM)

- **Source:** phase-05.md §3 Frontend + §4 step 4 -- "Dye / Report toggles: disabled with tooltip if not supported by check type".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DyeReportToggles>` (`describe.each([['ltr'],['rtl']])`) | `dye_toggle_disabled_with_tooltip_when_check_type_dye_supported_false` | Render with `checkType={ dye_supported: 0, report_supported: 1 }`. Assert the dye toggle has `aria-disabled='true'` and is non-interactive; hovering surfaces a tooltip with text resolved from i18n key `reception.toggle.unsupported.dye`. Report toggle remains enabled. Mirror for `report_supported=0`. Both directions tested per testing.md §14 RTL anti-pattern. |

### §11.8 P05-G37 -- SubtypeRadioList conditional render per has_subtypes (MEDIUM)

- **Source:** phase-05.md §4 Frontend NewVisitForm step 2 + §3 `<SubtypeRadioList>` -- "rendered iff parent `check_types.has_subtypes = 1`".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<NewVisitForm>` | `subtype_radio_list_absent_when_check_type_has_subtypes_is_zero` | Render the form with `checkType={ has_subtypes: 0 }`. Assert `screen.queryByTestId('subtype-radio-list')` is `null` (element NOT rendered, not just empty). Layout flows without an empty section. Re-render with `has_subtypes=1`; assert the element IS present and lists subtypes filtered to non-deleted rows. Per §4 step 2. |

### §11.9 P05-G38 -- fs:scope capability for receipts directory (MEDIUM)

- **Source:** phase-05.md §5 Infrastructure -- `capabilities/default.json` adds `fs:scope: $APPDATA/idc-system/receipts/**`.
- **Target test section:** §6.7
- **Category:** Missing Capability Lint

§9.9 added `shell:allow-execute` scoping; no parallel `fs:scope` assertion.

| Scenario | Asserts |
|-|-|
| `capabilities_declare_only_receipts_fs_scope_added_in_phase05` | Parse `src-tauri/capabilities/default.json`. Assert the `fs` plugin permission set contains exactly ONE new scope added by phase-05: a `$APPDATA/idc-system/receipts/**` allowlist entry. Assert NO broader allowlist (`$APPDATA/**`, `$HOME/**`) appears anywhere. The receipts scope is read+write; no other fs paths are granted by this phase. Per §5 Infrastructure. |

### §11.10 P05-G39 -- Receipt path yyyy/mm partitioning + .txt extension (MEDIUM)

- **Source:** phase-05.md §6 verification step 8 + §7.55 -- receipt path `$APPDATA/idc-system/receipts/{yyyy}/{mm}/{visit_id}.{pdf,txt}`.
- **Target test section:** §2.1
- **Category:** Missing Edge Coverage

| Scenario | Asserts |
|-|-|
| `receipts_persist_under_yyyy_mm_partition_with_correct_extensions` | Lock two visits with `locked_at` values in different months (e.g. 2026-04-30 and 2026-05-01). Assert: (a) PDF for the April visit lives at `$APPDATA/idc-system/receipts/2026/04/<visit_id>.pdf`; (b) thermal text for the April visit at `$APPDATA/idc-system/receipts/2026/04/<visit_id>.txt` (NOT `.thermal` or `.esc-pos`); (c) May visit lands under `2026/05/`; (d) no flat-dir fallback. Per §7.55. |

### §11.11 P05-G40 -- useVisitAuditLog / useVisitReceipts read hooks (LOW)

- **Source:** phase-05.md §3 Frontend React Query hooks -- `useVisitAuditLog`, `useVisitReceipts`.
- **Target test section:** §2.4
- **Category:** Incomplete Coverage

| Hook | Test | Asserts |
|-|-|-|
| `useVisitAuditLog(visit_id)` | `caches_under_visits_audit_visitId_and_invalidates_on_lock_void` | Cache key `['visits','audit', <visit_id>]`. After `useVisitLock(visit_id).mutate()` resolves, the next read fetches fresh from IPC. Same for `useVisitVoid`. |
| `useVisitReceipts(visit_id)` | `caches_under_visits_receipts_visitId_and_invalidates_on_reprint` | Cache key `['visits','receipts', <visit_id>]`. After `useReceiptReprint(visit_id).mutate()` resolves, the cache entry is invalidated and the next read returns the updated receipts list. |

### §11.12 P05-G41 -- ChecksGridCard whole-surface clickable navigation (LOW)

- **Source:** phase-05.md §7.20 -- "Click anywhere on the card navigates to /reception/checks/:check-slug".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<ChecksGridCard>` (`describe.each([['ltr'],['rtl']])`) | `clicking_card_body_navigates_to_locale_resolved_slug` | Render the card with `checkType={ name_en: 'Complete Blood Count', name_ar: 'تعداد دم شامل' }`. Click on the card BODY (not the title, not a subtype chip -- a region with no inner button). Assert `router.location.pathname === '/reception/checks/complete-blood-count'` (en locale). Switch to ar locale (no `name_en` populated case): expected slug derived via `transliterate(name_ar)` per §3 routing block. Per §7.20. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 4 Phase-05 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P05-G42 through P05-G45). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P05-G42 -- Server audit-first for void push variant (HIGH)

- **Source:** phase-05.md §4 Sync Server Visit push acceptance step 4 -- "Insert audit row in same Prisma transaction" (parallel of §11.1's lock-variant coverage).
- **Target test section:** §2.3
- **Category:** Missing Integration Test

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `server_visit_push_void_writes_audit_row_in_same_tx_before_upsert` | Seed a locked visit V on the server. Push a `visits` payload transitioning V from `locked` -> `voided` (with `void_reason` populated and offsetting inventory_adjustments rows in the same envelope). On commit: query `audit_log` for this push; assert TWO rows exist with `action IN ('void', <consumption_reversal>)` AND that the `void` audit row's `at <=` the visit's `voided_at`. Inject an upsert failure mid-tx; assert NEITHER the audit row NOR the visit transition persist. Mirrors §11.1 P05-G30 for the void variant. |

### §12.2 P05-G43 -- visits_doctor / visits_operator partial-index assertions (MEDIUM)

- **Source:** phase-05.md §1 visits indexes -- `visits_doctor` (`WHERE deleted_at IS NULL AND doctor_id IS NOT NULL`) and `visits_operator` (`WHERE deleted_at IS NULL AND operator_id IS NOT NULL`).
- **Target test section:** §2.1
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `visits_doctor_and_visits_operator_partial_indexes_present_and_used` | Apply migration 005. `SELECT sql FROM sqlite_master WHERE type='index' AND name IN ('visits_doctor','visits_operator')`. Assert both DDLs include the documented WHERE predicates (`deleted_at IS NULL AND doctor_id IS NOT NULL`). Run representative query `EXPLAIN QUERY PLAN SELECT * FROM visits WHERE doctor_id = ? AND deleted_at IS NULL ORDER BY created_at DESC`; assert plan mentions `USING INDEX visits_doctor`. Mirror for `visits_operator`. Per §1. |

### §12.3 P05-G44 -- tauri-plugin-shell registration in lib.rs (MEDIUM)

- **Source:** phase-05.md §5 Infrastructure + §7.45 -- "register `tauri-plugin-shell` in `src-tauri/src/lib.rs`".
- **Target test section:** §2.1
- **Category:** Missing Setup

| Scenario | Asserts |
|-|-|
| `lib_rs_registers_tauri_plugin_shell_for_phase05` | Boot the Tauri test harness; introspect plugin registry for `shell`. Assert it is present and initialized. Static-analysis variant: grep `src-tauri/src/lib.rs` for `.plugin(tauri_plugin_shell::init())`. Either form closes the gap; without it, `settings::list_printers` / `receipts::print_pdf` would return runtime "plugin not found" errors that the §9.9 capability lint does not surface. Mirror of P01-G33. |

### §12.4 P05-G45 -- DB CHECK length(trim(void_reason)) >= 5 enforcement (MEDIUM)

- **Source:** phase-05.md §7.8 -- DB CHECK on `visits` requiring `length(trim(void_reason)) >= 5` when `status='voided'`.
- **Target test section:** §2.1 / §6.8
- **Category:** Missing Edge Coverage

| Scenario | Asserts |
|-|-|
| `visits_check_constraint_blocks_short_void_reason_at_db_layer` | Bypass service validators: issue raw `INSERT INTO visits (id, status, void_reason, voided_at, ...) VALUES ('v-1', 'voided', 'oops', <now>, ...)` against the SQLite connection. Assert `SQLITE_CONSTRAINT_CHECK` is returned and no row inserted. Repeat with `void_reason = '     '` (5 spaces) -- still rejected (length(trim(...)) < 5). Repeat with `'valid void reason'` -- succeeds. The CHECK is the last line of defence against a malformed sync-apply path. Per §7.8. |
