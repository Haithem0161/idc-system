# Phase 03: Catalog & Reference Data -- Test Plan

**Proves:** All 8 catalog entities (`check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`) ship with full CRUD via `<AdminShell>`, FTS5-backed doctor search (excludes soft-deleted via trigger filters per §7.33), the XOR invariant on `check_types.has_subtypes` is enforced at three layers (UI prompt + Rust entity + server `acceptPush`), `doctor_check_pricing` and `inventory_consumption_map` Postgres NULL-uniqueness gotchas are blocked by paired partial unique indexes per §7.20 + §7.21, the `inventory_items.quantity_on_hand` is informational over sync (overwritten by pull-time recompute per §7.25, consumed by phase-06), `catalog:pricing_changed` is emitted on every check-type/subtype/pricing mutation with the §7.35 payload schema so phase-05's draft banner can filter affected drafts, `effective_price(doctor_id, check_type_id, check_subtype_id)` resolves per the §7.26 contract that phase-05 lock consumes, `operator_service.soft_delete` cascades to `operator_specialties` rows (§7.22), every catalog mutation writes `with_audit` (§7.18), and `/admin/*` is wrapped in `<RequireRole roles={['superadmin']}>` (§7.36) so receptionists / accountants cannot reach admin URLs.

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, `with_audit`, outbox, audit-action enum, conflict-resolution policy registry, `<SyncPill>`, `<AppShell>`), Phase 02 test (auth + roles, `<RequireRole>`, `users` for actor on audit rows, `<RootRedirect>` for role-default routing, `formatIqd` / `formatInt` helpers from §7.12 / §7.30, the i18n `errors:*` namespace baseline).

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_check_type_flat, make_check_type_subtyped, make_check_subtype, make_doctor, make_doctor_pricing_pct, make_doctor_pricing_fixed, make_operator, make_operator_specialty, make_inventory_item, make_consumption_map_row}` (extended).
- Factories (TS): `src/test-utils/factories.ts::{makeCheckType, makeCheckSubtype, makeDoctor, makeDoctorPricing, makeOperator, makeOperatorSpecialty, makeInventoryItem, makeConsumptionMapRow}`.
- Factories (Sync server): mirrors of all 8 entities' push payloads.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- contains the full catalog (5 check types: 3 flat + 2 with subtypes; 8 doctors + their pricing rows; 6 operators with specialties; 12 inventory items + their consumption maps). Phase-03 plan consumes the fixture for persona runs; the schema rows are owned here.
- Synthetic scale fixture: 200 doctors + 500 inventory items (for FTS perf assertions).

**Tool prerequisites:**
- Inherited from phase-01 / phase-02 test execution: `cargo-llvm-cov`, `vitest` + `@testing-library/react` + `jsdom` + `@vitest/coverage-v8`, `webdriverio` + `tauri-driver`, `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser`, `wiremock`, `testcontainers`, `msw@2`, `argon2`, `jsonwebtoken`.
- None new -- phase-03 inherits the full toolchain. FTS5 is built into the bundled SQLite (no extra crate). Phase-03 IS the first phase to exercise FTS5 at scale; the test infrastructure for FTS5 query injection lands here and is reused by phase-05's patients FTS.

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`.
- 3xN conflict matrix exhaustively -- the `last-write-wins` cell for all 8 catalog entities is exercised here (one representative entity); the cross-product against `users` / `settings` / `visits` lives in `sync-conflicts.md`.
- Page-by-page i18n / RTL snapshots for `/admin/*` -- phase-03 asserts core invariants; the full visual page-by-page sweep is in `i18n-rtl.md`.
- 200-doctor / 500-item / 12-month scale runs aggregated -- owned by `performance-soak.md`.
- `inventory_items.quantity_on_hand` pull-time recompute IMPLEMENTATION -- the CONTRACT is declared here (§7.25) but the hook lands in phase-06 §7.9 and the integration test for the hook lives in `phase-06-test.md` §2.1. Phase-03 verifies only that the contract is documented and the pulled value is informational at this point.

**Cross-phase commands:** none. Phase-03 owns 27 IPC commands (CRUD across 8 entities + `doctor_pricing::upsert`/`soft_delete`, `operator_specialties::upsert`/`soft_delete`, `inventory_consumption::upsert`/`soft_delete`, `doctors::set_active` from §7.23, plus the `effective_price` IPC for phase-05's lock consumption). All registered + tested here.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`CheckType` entity (`src-tauri/src/domains/catalog/domain/entities/check_type.rs`)** -- the XOR invariant on `has_subtypes` is the most risk-bearing piece in the phase.

| Module | Test | Asserts |
|-|-|-|
| `CheckType::try_new_flat` | `produces_flat_with_base_price_and_has_subtypes_false` | Defaults; `has_subtypes = 0`; `base_price_iqd = Some(X)`. |
| `CheckType::try_new_flat` | `rejects_negative_base_price` | `base_price_iqd = -100` -> `Err(CheckTypeError::NegativePrice)`. |
| `CheckType::try_new_flat` | `rejects_empty_name_ar` | -> `Err`. |
| `CheckType::try_new_subtyped` | `produces_subtyped_with_base_price_none` | `has_subtypes = 1`, `base_price_iqd = None`. |
| `CheckType::try_new_subtyped` | `rejects_when_base_price_provided` | `Err(CheckTypeError::SubtypedRequiresNullPrice)`. |
| `CheckType::toggle_to_subtyped` | `clears_base_price_atomically` | Existing flat type -> subtyped: `has_subtypes` flips to 1; `base_price_iqd` flips to `None` in the same struct update; version bumped. |
| `CheckType::toggle_to_flat` | `errs_when_subtype_rows_exist` | Per §7.1: pass `has_subtypes_rows = true` -> `Err(CheckTypeError::SubtypesExist)`. |
| `CheckType::toggle_to_flat` | `succeeds_when_no_subtype_rows_exist` | Pass `has_subtypes_rows = false`, valid `price` -> `Ok`. `has_subtypes=0`, `base_price_iqd=Some(price)`. |

**`CheckSubtype` entity**

| Module | Test | Asserts |
|-|-|-|
| `CheckSubtype::try_new` | `requires_parent_has_subtypes_eq_1` | Per §7.2: caller-passes a `parent_has_subtypes: bool`; `false` -> `Err(CheckSubtypeError::ParentNotSubtyped)`. |
| `CheckSubtype::try_new` | `rejects_negative_price` | -- |

**`Doctor` entity**

| Module | Test | Asserts |
|-|-|-|
| `Doctor::try_new` | `rejects_empty_name_after_trim` | Per §7.4. |
| `Doctor::try_new` | `accepts_unicode_arabic_and_mixed_scripts` | `"د. Layla هاشم"` round-trips byte-stable. |
| `Doctor::set_active` | `bumps_version_and_writes_dirty_true` | Per §7.23. |

**`DoctorCheckPricing` entity + `effective_price` resolver**

| Module | Test | Asserts |
|-|-|-|
| `DoctorCheckPricing::try_new` | `pct_kind_requires_value_in_0_to_100` | `cut_kind=Pct`, `cut_value=150` -> `Err(PricingError::PctOutOfRange)`. |
| `DoctorCheckPricing::try_new` | `fixed_kind_rejects_negative_value` | -- |
| `DoctorCheckPricing::try_new` | `rejects_negative_price_override` | -- |
| `DoctorCheckPricing::try_new` | `requires_subtype_when_parent_has_subtypes` | Pass `parent_has_subtypes=true` + `check_subtype_id=None` -> `Err(PricingError::SubtypeRequired)`. Per §7.6. |
| `DoctorCheckPricing::try_new` | `rejects_subtype_when_parent_lacks_subtypes` | Pass `parent_has_subtypes=false` + `check_subtype_id=Some(_)` -> `Err(PricingError::SubtypeForbidden)`. |
| `effective_price::resolve` | `house_pricing_returns_subtype_or_basetype_price` | Per §7.26 step 1: `doctor_id = None` -> returns `subtype.price_iqd` if Some(subtype), else `check_type.base_price_iqd`. |
| `effective_price::resolve` | `doctor_pricing_override_wins_when_present` | Per §7.26 step 3: `price_override_iqd = Some(15000)` -> returns `15000`. |
| `effective_price::resolve` | `doctor_pricing_falls_back_to_subtype_or_basetype_when_no_override` | Per §7.26 step 4. |
| `effective_price::resolve` | `pure_function_does_not_mutate_state` | After resolution, no row's `version` changed; no audit row written. |

**`Operator` entity**

| Module | Test | Asserts |
|-|-|-|
| `Operator::try_new` | `rejects_negative_base_cut` | -- |
| `Operator::set_active` | `does_not_block_when_open_shifts_exist` | Per §7.24: `is_active=0` is allowed even with open shifts. Only `soft_delete` (per §7.22) checks open shifts. |
| `Operator::soft_delete` | `cascades_specialties_to_deleted` | Per §7.22: the pure helper returns `Vec<OperatorSpecialtyId>` to soft-delete. (The actual cascade SQL runs in the service layer; the helper computes the IDs.) |

**`OperatorSpecialty` entity** -- standard CRUD, minimal logic.

**`InventoryItem` entity**

| Module | Test | Asserts |
|-|-|-|
| `InventoryItem::try_new` | `rejects_empty_unit_after_trim` | Per §7.5: `unit = "  "` -> `Err`. |
| `InventoryItem::try_new` | `rejects_negative_low_stock_threshold` | -- |
| `InventoryItem::try_new` | `accepts_quantity_on_hand_eq_0_at_creation` | Default to 0; phase-06 adjustments mutate. |

**`InventoryConsumptionMap` entity**

| Module | Test | Asserts |
|-|-|-|
| `InventoryConsumptionMap::try_new` | `rejects_quantity_per_check_zero_or_negative` | Per §1 CHECK + §7.9 step 1: `quantity_per_check <= 0` -> `Err`. |
| `InventoryConsumptionMap::try_new` | `requires_subtype_when_parent_has_subtypes` | Per §7.9: parent `has_subtypes=1` + `check_subtype_id=None` -> `Err(ConsumptionMapError::SubtypeRequired)`. |
| `InventoryConsumptionMap::try_new` | `rejects_subtype_when_parent_lacks_subtypes` | Per §7.9: mirror -> `Err(ConsumptionMapError::SubtypeForbidden)`. |
| `InventoryConsumptionMap::try_new` | `rejects_on_dye_only_when_parent_dye_supported_zero` | Per §7.34: `parent.dye_supported=0` + `on_dye_only=1` -> `Err(ConsumptionMapError::DyeNotSupportedOnParent)`. |

**`CatalogPricingChangedEmitter` (`src-tauri/src/domains/catalog/service/pricing_changed_emitter.rs`)** -- per §7.27 + §7.35.

| Module | Test | Asserts |
|-|-|-|
| `PricingChangedEmitter::compute_payload` | `emits_check_type_kind_on_check_type_update` | Per §7.35 payload schema: `kind = 'check_type'`, `changed_entity_id`, `check_type_id`, `changed_at`. |
| `PricingChangedEmitter::compute_payload` | `emits_check_subtype_kind_on_subtype_mutation` | -- |
| `PricingChangedEmitter::compute_payload` | `emits_doctor_pricing_kind_on_pricing_upsert` | Per §7.27 + §7.35: includes `doctor_id`. |
| `PricingChangedEmitter::compute_payload` | `pure_function_does_not_emit_directly` | The function builds the payload; the caller emits via `app_handle.emit_all`. |

### §1.2 TS pure functions / value objects

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/check-type.ts::CheckTypeSchema` | `xor_refinement_rejects_both_subtypes_and_base_price` | `has_subtypes=1 && base_price_iqd!=null` -> ZodError. |
| `src/lib/schemas/check-type.ts::CheckTypeSchema` | `xor_refinement_rejects_neither` | `has_subtypes=0 && base_price_iqd==null` -> ZodError. |
| `src/lib/schemas/doctor.ts::DoctorPricingSchema` | `pct_value_in_0_to_100_or_error` | Refinement matches the Rust constraint. |
| `src/lib/schemas/inventory.ts::InventoryConsumptionMapSchema` | `quantity_per_check_must_be_positive` | -- |
| `src/lib/schemas/inventory.ts::InventoryConsumptionMapSchema` | `on_dye_only_only_when_parent_dye_supported` | (Refinement validated client-side via the parent's `dye_supported` flag injected through context.) |
| `src/lib/format/locale-name.ts::resolveLocaleName` | `returns_name_en_when_active_locale_en_and_non_null` | Per §7.16: `({name_ar:'X', name_en:'Y'}, 'en')` -> `'Y'`. |
| `src/lib/format/locale-name.ts::resolveLocaleName` | `falls_back_to_name_ar_when_name_en_null` | -> `'X'`. |
| `src/lib/format/locale-name.ts::resolveLocaleName` | `returns_name_ar_for_ar_locale_regardless_of_name_en` | -- |
| `src/lib/search.ts::shouldFireSearch` | `requires_min_2_chars_after_trim` | Per §7.14: `"l"` -> false; `" Li "` -> true (trims). |
| `src/lib/search.ts::debounceMs` | `is_250` | Per §7.14. |
| `src/lib/events/pricing.ts::PricingChangedPayloadSchema` | `parses_each_of_4_kinds` | `'check_type' | 'check_subtype' | 'doctor_pricing' | 'settings'` per §7.35. |
| `src/lib/events/pricing.ts::affectsActiveDraft` | `matches_when_check_type_and_subtype_intersect_draft` | Per §7.35: a payload with `kind='doctor_pricing', check_type_id=A, check_subtype_id=B, doctor_id=D` matches a draft with the same tuple. |
| `src/lib/events/pricing.ts::affectsActiveDraft` | `does_not_match_when_check_type_differs` | Different `check_type_id` -> `false`. The banner does not flag this draft. |
| `src/stores/admin-nav-store.ts` | `active_sub_page_persists_in_memory_only` | The store holds state in memory; never persists to localStorage. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/catalog/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::catalog::domain` |
| `src-tauri/src/domains/catalog/service/**` (effective_price resolver, PricingChangedEmitter, every entity service) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::catalog::service` |
| `src-tauri/src/domains/catalog/infrastructure/**` (sqlx repos for all 8 entities, FTS5 triggers) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::catalog::infrastructure` |
| `src/features/catalog/**`, `src/lib/schemas/{check-type,check-subtype,doctor,operator,inventory}.ts`, `src/lib/format/locale-name.ts`, `src/lib/search.ts`, `src/lib/events/pricing.ts`, `src/stores/admin-nav-store.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/catalog/**,src/lib/schemas/{check-type,check-subtype,doctor,operator,inventory}.ts,src/lib/format/locale-name.ts,src/lib/search.ts,src/lib/events/pricing.ts,src/stores/admin-nav-store.ts"` |
| `src/pages/admin/**` (check-types, doctors, operators, inventory), `src/components/admin/**` (admin-shell, has-subtypes-toggle, doctor-pricing-editor, operator-specialty-picker, consumption-map-editor, inventory-admin-table, check-type-form) | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/admin/**,src/components/admin/**"` |
| `sync-server/src/app/domains/catalog/domain/**` + `service/**` (all 8 entities + push acceptance with XOR + NULL-uniqueness + subtype-required checks) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/catalog/presentation/**` (`/sync/push` and `/sync/pull` catalog-branches) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/catalog_phase03.rs` (already exists at HEAD per testing-status.md baseline). Extend.
- Auxiliary files: split per the 8 entities if the file grows above ~1500 lines; default to one file.

**New scenarios in `catalog_phase03.rs`:**

| Scenario | Asserts |
|-|-|
| `check_type_create_flat_persists_and_audits` | Insert; assert one `audit_log` row `action='create'`, `entity='check_types'`. |
| `check_type_create_subtyped_persists_with_null_base_price` | -- |
| `check_type_toggle_has_subtypes_zero_to_one_clears_base_price_in_one_tx` | Per §7.1: toggle in one tx; assert `has_subtypes=1`, `base_price_iqd IS NULL`, one audit row `action='update'`. |
| `check_type_toggle_has_subtypes_one_to_zero_blocked_by_subtype_rows` | Seed 1 non-deleted subtype; toggle to flat -> `Err(CheckTypeError::SubtypesExist)`; CHECK preserves the row. |
| `check_type_toggle_has_subtypes_one_to_zero_succeeds_when_subtypes_soft_deleted` | Soft-delete all subtypes; toggle to flat with new price; succeeds. |
| `check_type_soft_delete_blocked_by_references` | Insert a `doctor_check_pricing` row referencing the type; attempt `check_types::soft_delete` -> `Err(CheckTypeError::Referenced)`. Per §4. |
| `check_subtype_create_requires_parent_has_subtypes_1` | Parent `has_subtypes=0` -> `Err(CheckSubtypeError::ParentNotSubtyped)`. Per §7.2. |
| `check_subtype_unique_per_parent_at_db_layer` | The DB uniqueness is implicit (no constraint); two subtypes with the same name under the same parent are allowed. The test pins this design decision. |
| `doctor_create_persists_and_indexes_in_fts5` | Insert; query `doctors_fts MATCH 'Layla'` returns the row. |
| `doctor_update_re_indexes_fts5` | Update name from "Layla" to "Layla H."; query for "Layla H." returns the row; query for "Layla" (prefix) still matches. |
| `doctor_soft_delete_removes_from_fts5` | Per §7.33: soft-delete; FTS query returns 0 rows. |
| `doctor_un_soft_delete_re_adds_to_fts5` | Restore `deleted_at = null`; FTS query returns the row. Per §7.33 trigger behavior. |
| `doctor_soft_delete_cascades_pricings_in_same_tx` | Insert doctor + 3 pricing rows; soft-delete doctor; assert all 4 rows have `deleted_at != null`; each emits an audit row `action='soft_delete'`. |
| `doctor_set_active_writes_update_audit_with_is_active_delta` | Per §7.23. |
| `doctor_check_pricing_upsert_paired_unique_constraint_no_subtype` | Per §7.20: insert two pricings with same `(doctor, check_type)` and `check_subtype_id = NULL`; second -> SQLite unique constraint violation (local IFNULL trick) AND server Postgres partial-index violation. |
| `doctor_check_pricing_upsert_paired_unique_constraint_with_subtype` | Two pricings with same `(doctor, check_type, check_subtype)` -> unique violation. Per §7.20. |
| `doctor_check_pricing_upsert_accepts_subtype_null_and_subtype_non_null_for_same_doctor_check` | `(doc, ct, NULL)` and `(doc, ct, sub-A)` both insert (they're distinct pairs). |
| `doctor_check_pricing_pct_value_in_0_to_100_at_db_layer` | Raw insert with `cut_kind='pct', cut_value=150` -> CHECK violation. Per §1. |
| `doctor_check_pricing_emits_catalog_pricing_changed_event_on_upsert` | Per §7.27: after commit, the event handler receives the payload; payload conforms to §7.35 schema. |
| `operator_create_persists_and_audits` | -- |
| `operator_set_active_does_not_block_open_shifts` | Per §7.24: forward-ref phase-04 -- with phase-04's `operator_shifts` table not yet created, the test asserts the SQL guard is not in the `set_active` code path. |
| `operator_soft_delete_cascades_specialties_in_one_tx` | Per §7.22: soft-delete operator + 3 specialties; all 4 rows updated; 4 audit rows; 4 outbox rows. |
| `operator_specialty_unique_per_operator_and_check_type` | Insert two `(operator, check_type)` -> second hits unique constraint. Per §1. |
| `inventory_item_create_persists_with_quantity_on_hand_default_0` | -- |
| `inventory_item_create_rejects_empty_unit` | Per §7.5: raw insert with `unit=' '` -> CHECK violation. |
| `inventory_item_soft_delete_blocked_by_consumption_map_reference` | Per §7.8: with one non-deleted consumption row -> `Err(InventoryItemError::ReferencedByConsumptionMap)`. |
| `inventory_item_active_index_used_in_active_filter` | Per §7.7: `EXPLAIN QUERY PLAN` for `SELECT * FROM inventory_items WHERE entity_id=? AND is_active=1 AND deleted_at IS NULL` mentions `inventory_items_active`. |
| `inventory_consumption_map_upsert_requires_subtype_when_parent_has_subtypes` | Per §7.9 step 2. |
| `inventory_consumption_map_upsert_rejects_subtype_when_parent_lacks_subtypes` | Per §7.9 step 3. |
| `inventory_consumption_map_upsert_rejects_on_dye_only_when_parent_dye_supported_zero` | Per §7.34: `parent.dye_supported=0` + `on_dye_only=1` -> `Err(ConsumptionMapError::DyeNotSupportedOnParent)`. |
| `inventory_consumption_map_paired_unique_constraint_no_subtype` | Per §7.21: same Postgres NULL gotcha; partial unique blocks duplicates. |
| `inventory_consumption_map_paired_unique_constraint_with_subtype` | -- |
| `inventory_consumption_map_emits_catalog_pricing_changed_event_with_settings_kind` | Per §7.27 + §7.35: setting changes (phase-02) also emit `kind='settings'`; phase-03 mutations emit `kind='check_type'|'check_subtype'|'doctor_pricing'`. |
| `effective_price_resolver_house_subtype_returns_subtype_price` | Per §7.26 step 1. |
| `effective_price_resolver_house_basetype_returns_check_type_base_price` | -- |
| `effective_price_resolver_doctor_override_wins` | Per §7.26 step 3: `price_override_iqd = Some(X)` -> X. |
| `effective_price_resolver_doctor_no_override_falls_back_to_subtype` | -- |
| `effective_price_resolver_does_not_mutate_state` | After resolution, no row version changed; no audit row written. Per §7.26 contract. |
| `effective_price_resolver_excludes_soft_deleted_pricing_rows` | A soft-deleted pricing row is treated as if it doesn't exist; falls back to subtype/basetype price. |
| `catalog_pull_time_inventory_items_quantity_on_hand_is_informational_at_phase_03` | Per §7.25: pull an `inventory_items` row with `quantity_on_hand=999`; without `inventory_adjustments` (phase-05 not yet wired), the pulled value is taken as-is. Phase-06 introduces the recompute hook that overwrites. |
| `migration_003_creates_8_tables_and_fts5_table_and_triggers_idempotently` | Run migration twice on fresh + populated DB. All tables, indexes, FTS5 virtual table, and the 3 triggers exist. |
| `migration_003_doctors_fts_triggers_filter_soft_deleted_per_7_33` | After replay, the triggers' SQL matches §7.33 exactly: inserts only when `new.deleted_at IS NULL`. |
| `with_audit_writes_audit_first_for_every_catalog_mutation` | Per §7.18 + phase-01 §7.7: instrument the writer; assert audit row precedes business row for create / update / soft_delete on all 8 entities. |

### §2.2 Tauri IPC handler tests

One test per command. Happy + at least one error path.

The full 27-command IPC matrix (5 entities × CRUD + soft_delete = 30 base, minus 3 that aren't exposed as IPCs = 27 effective). Phase-03 also adds the `effective_price` resolver IPC and the `doctors::set_active` from §7.23.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `check_types_list` | `returns_array_of_check_types_optionally_including_deleted` | `returns_validation_for_invalid_include_deleted_type` |
| `check_types_get` | `returns_one_check_type` | `returns_not_found` |
| `check_types_create` | `creates_flat_type_returns_persisted_row` | `xor_rule_violation_returns_typed_error` -- e.g., both `has_subtypes=1` AND `base_price_iqd=Some` |
| `check_types_update` | `updates_name_and_writes_audit_row` | `non_superadmin_returns_forbidden` -- per §7.36 |
| `check_types_soft_delete` | `returns_unit_when_no_references` | `returns_referenced_when_subtypes_or_pricing_exist` |
| `check_types_toggle_has_subtypes` (extracted per §7.1) | `toggle_0_to_1_clears_base_price` | `toggle_1_to_0_with_subtypes_returns_subtypes_exist` |
| `check_subtypes_list_by_type` | `returns_subtypes_for_parent` | `returns_validation_for_invalid_type_id` |
| `check_subtypes_create` | `creates_and_writes_audit` | `parent_not_subtyped_returns_error` -- per §7.2 |
| `check_subtypes_update` | `updates_and_audits` | -- |
| `check_subtypes_soft_delete` | `returns_unit` | -- |
| `doctors_list` | `returns_doctors_with_optional_query_fts5` | `query_below_2_chars_returns_validation` -- per §7.14 |
| `doctors_get` | `returns_doctor_with_pricings_array` | -- |
| `doctors_create` | `inserts_and_indexes_in_fts5` | `empty_name_after_trim_returns_validation` |
| `doctors_update` | `re_indexes_fts5_after_name_change` | -- |
| `doctors_soft_delete` | `cascades_pricings_and_writes_audit_per_row` | -- |
| `doctors_set_active` (per §7.23) | `flips_is_active_and_audits_update` | `non_superadmin_returns_forbidden` |
| `doctor_pricing_upsert` | `inserts_or_updates_by_unique_tuple` | `pct_value_above_100_returns_validation` |
| `doctor_pricing_soft_delete` | `returns_unit` | -- |
| `operators_list` | `returns_operators` | -- |
| `operators_get` | `returns_operator_with_specialties` | -- |
| `operators_create` | `inserts_and_audits` | -- |
| `operators_update` | `updates_and_audits` | -- |
| `operators_soft_delete` | `cascades_specialties_in_one_tx` -- per §7.22 | -- |
| `operators_set_active` | `flips_is_active_without_blocking_open_shifts` -- per §7.24 | `non_superadmin_returns_forbidden` |
| `operator_specialties_upsert` | `inserts_or_returns_existing` | `duplicate_pair_returns_existing_not_duplicate` |
| `operator_specialties_soft_delete` | `returns_unit` | -- |
| `inventory_catalog_list` | `returns_items_with_optional_query` | `query_below_2_chars_returns_validation` -- per §7.15 |
| `inventory_catalog_get` | `returns_item_with_consumption_map` | -- |
| `inventory_catalog_create` | `inserts_and_audits` | `empty_unit_returns_validation` -- per §7.5 |
| `inventory_catalog_update` | `updates_and_audits` | -- |
| `inventory_catalog_soft_delete` | `blocked_when_consumption_map_references_exist` -- per §7.8 | -- |
| `inventory_consumption_upsert` | `inserts_with_dye_only_when_parent_supports_dye` | `dye_not_supported_on_parent_returns_typed_error` -- per §7.34 |
| `inventory_consumption_soft_delete` | `returns_unit_and_audits` | -- |
| `pricing_resolve_effective_price` (new IPC for phase-05 consumption) | `returns_correct_price_per_7_26_resolution_steps` | `returns_validation_for_malformed_uuids` |

All IPC tests construct `AppState` directly. Each test asserts the serialized error shape.

### §2.3 Sync server route handlers

Phase 03 adds NO new routes -- all 8 catalog entities flow through `/sync/push` and `/sync/pull` (declared in phase-01).

File: `sync-server/test/sync/catalog-phase03.test.ts` (NEW).

DB: real Prisma test DB via `testcontainers`; per-test teardown.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_check_type_xor_violation_rejected_422` | Per §7.1 server: payload with `has_subtypes=1` AND `base_price_iqd != null` -> 422 with `error.code = 'CHECK_TYPE_XOR_VIOLATION'`. |
| `POST /sync/push` | `push_check_subtype_with_parent_has_subtypes_zero_rejected_422` | Per §7.2 server. |
| `POST /sync/push` | `push_check_type_lww_tiebreak_by_origin_device_id_lex` | Two updates with identical `updatedAt`, different `originDeviceId` -> lex-smaller wins. Per §7.17. |
| `POST /sync/push` | `push_doctor_pricing_paired_unique_no_subtype_blocks_duplicates` | Per §7.20 raw-SQL partial unique on Postgres: insert two pricings with same `(doctor, check_type)` and `check_subtype_id=NULL` -> second returns 422 + `error.code = 'DUPLICATE_PRICING_ROW'`. |
| `POST /sync/push` | `push_doctor_pricing_paired_unique_with_subtype_blocks_duplicates` | -- |
| `POST /sync/push` | `push_doctor_pricing_pct_value_above_100_rejected_via_typebox_then_check` | Per §7.6 server. |
| `POST /sync/push` | `push_inventory_consumption_paired_unique_no_subtype_blocks_duplicates` | Per §7.21. |
| `POST /sync/push` | `push_inventory_consumption_subtype_required_or_forbidden_per_parent` | Per §7.9 server. |
| `POST /sync/push` | `push_inventory_consumption_dye_only_requires_parent_dye_supported` | Per §7.34 server. |
| `POST /sync/push` | `push_operator_soft_delete_cascades_specialties_at_server` | Per §7.22 server mirror: applying a soft-delete on `operators` also soft-deletes the operator's specialties in the same Prisma tx. |
| `POST /sync/push` | `push_inventory_items_quantity_on_hand_is_informational` | Per §7.25: pull-time recompute responsibility lives client-side (phase-06 hook); the server accepts the value but doesn't enforce a constraint. |
| `GET /sync/pull` | `pull_returns_all_8_catalog_entities_for_tenant` | -- |
| `GET /sync/pull` | `pull_excludes_other_tenants_catalog` | -- |
| `GET /sync/pull` | `pull_sets_pulled_at_on_all_8_models_per_7_19` | After the pull, each pulled row has `pulledAt` set. |
| (raw-SQL migration) | `prisma_migrate_deploy_applies_paired_unique_indexes_in_order` | Per §7.31: the `<ts>_drop_unique_<entity>` file runs before `<ts+1>_partial_unique_<entity>`. `pnpm prisma migrate status` is clean after each. |

### §2.4 React Query mutation / query flows

Mocked IPC; all component tests run `describe.each([['ltr'],['rtl']])`.

| Hook | Test | Asserts |
|-|-|-|
| `useCheckTypesList` | `caches_under_catalog_checkTypes_list_key` | -- |
| `useCheckTypeToggleHasSubtypes` | `dispatches_check_types_toggle_has_subtypes_and_invalidates_keys` | -- |
| `useDoctorsList` | `query_below_2_chars_skipped_via_search_helper` | Per §7.14 + `src/lib/search.ts`. |
| `useDoctorsList` | `debounces_query_at_250ms` | Vitest fake timer; rapid keystrokes coalesce into one IPC call after 250ms quiet. |
| `useDoctorCreate` | `invalidates_doctors_list_and_emits_no_pricing_changed_event_for_doctor_only` | Per §7.27: only pricing/check-type/subtype changes emit `catalog:pricing_changed`. Pure doctor name edits don't. |
| `useDoctorPricingUpsert` | `emits_catalog_pricing_changed_event_via_listener` | Confirms the listener is wired; the event handler receives the payload with `kind='doctor_pricing'`. |
| `useOperatorsList` | -- | -- |
| `useOperatorSoftDelete` | `cascades_specialties_visually` | After mutation, the operator row's specialties count drops to 0 in the UI. |
| `useInventoryCatalog` | -- | -- |
| `useInventoryConsumptionUpsert` | `dye_only_radio_disabled_when_parent_dye_supported_zero` | Component test; the disabled state matches the parent's `dye_supported` flag. |
| `useEffectivePrice(doctorId, checkTypeId, subtypeId)` | `returns_price_per_resolution_steps` | -- |

Components covered (each `describe.each([['ltr'],['rtl']])`):
- `<AdminShell>` renders 7 sub-sidebar items in order (per §7.11: Users, Check Types, Doctors, Operators, Inventory, Settings, Audit). Active item highlighted.
- `<AdminShell>` is wrapped in `<RequireRole roles={['superadmin']}>` (§7.36). Non-superadmin sees `<Navigate to="/no-access" />`.
- `<HasSubtypesToggle>` prompts "Setting subtypes mode clears the flat price. Continue?" on 0→1 flip.
- `<HasSubtypesToggle>` blocks 1→0 flip with toast when subtypes exist.
- `<DoctorPricingEditor>` row layout: check-type picker + subtype picker (only when parent has subtypes) + `cut_kind` radio + `cut_value` input + optional `price_override_iqd`.
- `<OperatorSpecialtyPicker>` multi-select combobox; diffs against existing rows on save; emits upserts + soft_deletes per the diff.
- `<ConsumptionMapEditor>` enables `on_dye_only` toggle only when parent's `dye_supported=1`.
- `<CheckTypeForm>` (per §7.12): name_ar (required), name_en (optional), `<HasSubtypesToggle>`, base_price (when flat), dye_supported / report_supported toggles, active flag.
- `<InventoryAdminTable>` (per §7.13) joins `inventory_items` to audit log for last-edit-author column.
- Search inputs (`<DoctorSearch>`, `<InventoryItemSearch>`) require min 2 chars + 250ms debounce per §7.14.
- `<UserMenu>` hides the admin sub-sidebar links for non-superadmins. Per §7.36.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

Phase 03 adds NO server routes. The contract surface is the 8 entity payload schemas embedded in `/sync/push` and `/sync/pull`.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (request) | `CheckTypePushSchema` (per §7.1 XOR-refined) | `fixtures/payloads/check-type-flat-push.json`, `check-type-subtyped-push.json`, `check-type-xor-violation-push.json` (negative). The negative MUST fail Ajv with the custom keyword `xorHasSubtypesAndBasePrice`. |
| `POST /sync/push` (request) | `CheckSubtypePushSchema` | -- |
| `POST /sync/push` (request) | `DoctorPushSchema` | -- |
| `POST /sync/push` (request) | `DoctorPricingPushSchema` (per §7.6 pct-value-in-range refinement) | Each `cut_kind` variant. |
| `POST /sync/push` (request, negative) | `DoctorPricingPushSchema` | `doctor-pricing-pct-above-100-push.json` MUST fail. |
| `POST /sync/push` (request) | `OperatorPushSchema`, `OperatorSpecialtyPushSchema`, `InventoryItemPushSchema`, `InventoryConsumptionMapPushSchema` | -- |
| `POST /sync/push` (request, negative) | `InventoryConsumptionMapPushSchema` | `inv-consumption-dye-only-non-dye-parent.json` MUST fail per §7.34. |
| `GET /sync/pull` (response) | `CheckTypeResponseSchema` (+ 7 siblings) | Captured live for seeded tenant. Each entity's row MUST validate including `pulledAt` per §7.19. |

### §3.2 IPC shape contract

27 commands + the §3.2 fixed-error-envelope row.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `check_types_list` | `Vec<CheckType>` | `z.array(CheckTypeSchema)` |
| `check_types_get` | `CheckType` | `CheckTypeSchema` |
| `check_types_create` / `_update` | `CheckType` | -- |
| `check_types_soft_delete` | `()` | `z.void()` |
| `check_types_toggle_has_subtypes` | `CheckType` | -- |
| `check_subtypes_list_by_type` | `Vec<CheckSubtype>` | -- |
| `check_subtypes_create` / `_update` / `_soft_delete` | `CheckSubtype` / `CheckSubtype` / `()` | -- |
| `doctors_list` | `Vec<Doctor>` (with FTS-prefix `query` filter) | -- |
| `doctors_get` | `{ doctor: Doctor, pricings: Vec<DoctorCheckPricing> }` | (NEW) `DoctorWithPricingsSchema` |
| `doctors_create` / `_update` / `_soft_delete` / `_set_active` | -- | -- |
| `doctor_pricing_upsert` / `_soft_delete` | `DoctorCheckPricing` / `()` | -- |
| `operators_list` / `_get` / `_create` / `_update` / `_soft_delete` / `_set_active` | (mirror) | -- |
| `operator_specialties_upsert` / `_soft_delete` | -- | -- |
| `inventory_catalog_list` / `_get` (with `consumption: Vec<...>`) / `_create` / `_update` / `_soft_delete` | -- | -- |
| `inventory_consumption_upsert` / `_soft_delete` | -- | -- |
| `pricing_resolve_effective_price` | `i64` | `z.number().int().nonnegative()` |
| (Error envelope -- fixed) | `AppError` (with new variants `CheckTypeError`, `CheckSubtypeError`, `DoctorError`, `PricingError`, `OperatorError`, `InventoryItemError`, `ConsumptionMapError`) | `AppErrorSchema` -- shared schema. New `kind` values: each entity-error kind. |

### §3.3 Sync envelope contract

- **Push payload conforms.** All 8 entities' Rust push payloads serialize to JSON matching the TypeBox schemas.
- **Pull payload conforms.** Server's per-entity response schemas (with `pulledAt`) match the client's mirrored Zod schemas.
- **Conflict-resolution policy registry agrees.** Per §4 sync-semantics: all 8 entities are `last-write-wins`. Per §7.17: `originDeviceId` lex tiebreak.
- **Versioned envelope.** `envelope_version: 1`.
- **Snapshot files**:
  - `expected/sync/check-type-flat-push-canonical.json.sha256`
  - `expected/sync/check-type-subtyped-push-canonical.json.sha256`
  - `expected/sync/check-subtype-push-canonical.json.sha256`
  - `expected/sync/doctor-push-canonical.json.sha256`
  - `expected/sync/doctor-pricing-push-canonical.json.sha256`
  - `expected/sync/operator-push-canonical.json.sha256`
  - `expected/sync/operator-specialty-push-canonical.json.sha256`
  - `expected/sync/inventory-item-push-canonical.json.sha256`
  - `expected/sync/inventory-consumption-map-push-canonical.json.sha256`

---

## §4 E2E Tests (Pyramid Layer 4)

Specs live under `e2e/specs/admin/`. Selectors are `data-testid`.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `check-type-flat-and-subtyped-crud.e2e.ts` | Mariam (superadmin) | 1) Create a flat check type with name + base price. 2) Verify in list. 3) Toggle to subtyped -> confirm dialog -> base price clears. 4) Add 2 subtypes. 5) Try toggle back to flat -> blocked toast. 6) Soft-delete subtypes. 7) Toggle to flat with new price. | All steps audit; each transition idempotent. |
| `doctor-create-and-pricing.e2e.ts` | Mariam | 1) Create doctor "د. Layla". 2) Verify FTS search finds her in `ar` ("لي") and `en` ("Lay"). 3) Add a pricing row for check-type X with cut_kind=pct, cut_value=25. 4) Save. | `catalog:pricing_changed` event fires; payload has `kind='doctor_pricing'`. |
| `doctor-soft-delete-cascades-pricings.e2e.ts` | Mariam | 1) Soft-delete a doctor with 3 pricings. 2) Verify in `/admin/doctors` they disappear. 3) Open audit log (phase-08 forward-ref; phase-03 verifies via test-only IPC). | All 4 rows soft-deleted in one tx; 4 audit rows. |
| `operator-soft-delete-cascades-specialties.e2e.ts` | Mariam | Per §7.22: soft-delete operator with 3 specialties; verify all 4 rows soft-deleted. | -- |
| `inventory-catalog-create-and-consumption-map.e2e.ts` | Mariam | 1) Create item "Lidocaine" with unit "ml". 2) Open consumption map editor. 3) Add row: check-type X + qty 2 + `on_dye_only=true`. 4) Verify the `on_dye_only` radio is disabled when X.dye_supported=0. | -- |
| `admin-route-role-guard-for-receptionist.e2e.ts` | Mehdi (receptionist) | Attempt to navigate to `/admin/check-types`. | Redirected by `<RequireRole roles={['superadmin']}>` to `/no-access`. Per §7.36. `<UserMenu>` does not show admin links. |
| `admin-rtl-mirror.e2e.ts` | Mariam | Switch locale to `ar`; navigate every `/admin/*` page. | Sub-sidebar mirrors to the right edge; eyebrow rules mirror; chevrons rotate via `rtl:rotate-180` per phase-08 §7.18 lint. |
| `breadcrumb-resolves-locale-name.e2e.ts` | Mariam | Navigate `/admin/check-types/<id>`. | Breadcrumb shows `resolveLocaleName` output (e.g., "MRI" in `en`, "تصوير بالرنين" in `ar`). Per §7.16 + §7.28. |
| `fts5-search-handles-arabic-and-english.e2e.ts` | Mariam | Search "Lay" (en); "لي" (ar). | Both return matching doctors. Per §7.33: soft-deleted doctors excluded. |
| `pricing-changed-event-fires-on-check-type-edit.e2e.ts` | Mariam | Edit a check type's base_price. | Listener observes `catalog:pricing_changed` with `kind='check_type'`, `check_type_id=<edited>`. Per §7.27 + §7.35. |

### §4.2 Failure-path flows

- **`offline-doctor-create-drains-on-reconnect.e2e.ts`** -- Per phase-01 pattern.
- **`xor-violation-rejected-at-three-layers.e2e.ts`** -- (a) UI prevents submitting with both fields set. (b) If raw IPC bypasses, `Err(CheckTypeError::XorViolation)`. (c) If server-side push bypass, 422. Per §7.1.
- **`pricing-pct-out-of-range-rejected.e2e.ts`** -- Set cut_value=150 with cut_kind=pct; UI Zod refuses; IPC refuses; server CHECK + TypeBox refuses. Per §7.6.
- **`consumption-dye-only-without-parent-support-rejected.e2e.ts`** -- Per §7.34.
- **`inventory-item-soft-delete-blocked-by-consumption-map.e2e.ts`** -- Per §7.8.
- **`check-type-soft-delete-blocked-by-references.e2e.ts`** -- Per §4.
- **`receptionist-cannot-set_active-doctor.e2e.ts`** -- Per §7.23: only superadmin can flip is_active. Receptionist IPC -> `Forbidden`.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-catalog-lww-tiebreak.e2e.ts` | Device A + B both edit the same doctor's name with identical `updatedAt`. Server-side tiebreak. | Lex-smaller `originDeviceId` wins; both devices converge. Per §7.17. |
| `two-device-paired-unique-pricing-conflict.e2e.ts` | Device A and Device B both insert `(doctor, check_type, NULL)` pricing rows offline. Both reconnect. | First push wins; second receives 422 `DUPLICATE_PRICING_ROW`. Per §7.20. |
| `two-device-doctor-soft-delete-propagates-cascade.e2e.ts` | Device A soft-deletes a doctor with pricings. Device B pulls. | Device B's local rows all show `deleted_at != null`; the doctor disappears from `<DoctorAutocomplete>`. |
| `two-device-pricing-changed-event-fires-on-pull.e2e.ts` | Device A edits a doctor's pricing. Device B pulls. | Device B's `catalog:pricing_changed` listener fires after pull-apply; phase-05's `<PricingChangedBanner>` on B (forward-ref) flags affected drafts. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: `<AdminShell>` sub-sidebar in both directions.** 7 items in the canonical order (§7.11); active item highlighted; collapsed/expanded state per the macOS-style layout.
- **`<DoctorPricingEditor>` per-row UX.** Verify the subtype picker is only visible when the parent has subtypes; the `cut_kind` radio swap clears the `cut_value` input on switch.
- **`<ConsumptionMapEditor>` cross-row invariants.** Verify the `on_dye_only` toggle is greyed out when parent's `dye_supported=0`; tooltip explains why.
- **FTS5 search responsiveness.** With 200 doctors seeded, typing "Lay" must show results within 50ms p99 (per §7 perf SLOs). Visual verification.
- **Catalog edit on settings:changed.** Per phase-02 §7.4 cross-coupling: when `settings.internal_doctor_pct` changes, `catalog:pricing_changed` fires with `kind='settings'`; phase-05's banner (forward-ref) renders.

### §5.2 Cross-references to `personas.md`

- `personas.md` -> **P3 Mariam the Superadmin** -> steps 4-7 (creates check types, doctors, operators, inventory items). Required for §8 DoD.
- `personas.md` -> **P2 Mehdi the Receptionist** -> step 1 (verifies `/admin/*` is gated; UserMenu hides links). Reinforcement.
- `personas.md` -> **P1 Asma the Accountant** -> verifies `/admin/*` blocked (same as receptionist). Reinforcement.

**Canonical: P3 Mariam the Superadmin.**

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **Catalog `created_at` / `updated_at` UTC.** All timestamps stored as ISO-8601 UTC; displayed in Baghdad local in the admin UI.
- **Clock skew vs server.** Per phase-01: pulled `updated_at` is server-authoritative.
- **Pricing-change rollover.** A pricing edit at `23:59 local` lands in today's audit; the catalog:pricing_changed event timestamp uses UTC.
- **DST defensive.** CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `domains/catalog/`.

### §6.2 i18n & RTL

- **en/ar swap on every admin route.** All 8 list + 8 detail routes. Strings from `admin.*` namespace. Asserted via §2.4 + phase-08 §7.9 lint.
- **Arabic-Indic numerals on `cut_value`, `base_price_iqd`, `price_override_iqd`, `quantity_per_check`, `low_stock_threshold`.** Per phase-02 §7.12.
- **RTL layout invariants.** `<AdminShell>` sub-sidebar mirrors. Numeric columns right-aligned in LTR -> aligned to page edge in RTL.
- **Mixed-direction doctor names.** `"د. Layla"` round-trips byte-stable through create -> FTS -> pull -> render.
- **`resolveLocaleName` helper.** Per §7.16: every list cell uses the helper; no inline `entity.name_ar || entity.name_en` fallback in JSX.

### §6.3 Offline & Network

- **Full offline CRUD.** All 8 entities work offline; the UI never blocks on a network call.
- **Intermittent connection.** Per phase-01 pattern.
- **Token expiry mid-sync.** Per phase-02 §7.25.
- **Server 5xx during push.** Per phase-01 pattern.
- **Partial-batch push.** Push 50 mixed catalog ops where op 27 has an XOR violation. Ops 1-26 + 28-50 applied; op 27 rejected.

### §6.4 Concurrency & Conflicts

- **2-device LWW on every entity.** Per §4 sync-semantics: all 8 entities are LWW. Tested representatively on `doctors` (§4.3).
- **3-device chain on `doctors` LWW.** Devices A, B, C all rename the same doctor offline; reconnect random order; converge to highest `updatedAt`.
- **Conflict policy invocation.** Assert all 8 entities registered as `'last-write-wins'`; assert no `manual` 409 ever emitted for catalog.
- **Conflict resolver round-trip.** N/A for catalog (LWW, never parks).
- **Delete-vs-edit.** Per phase-01 §7.16: incoming edit at T1 against local soft-delete at T2 -> deletion wins.
- **Paired-unique-index race.** Two devices simultaneously insert `(doctor, check_type, NULL)` pricing rows. The second push fails 422 server-side; the engine parks via §7.17 outbox.parked.

### §6.5 Crash & Recovery

- **SIGKILL mid-catalog-write.** Standard tx atomicity per phase-01.
- **SQLite WAL after crash.** Per phase-01.
- **FTS5 trigger atomicity.** Insert + FTS5 trigger row are in the same SQLite tx (the trigger fires on the same WAL frame). A crash between them rolls both back. Asserted in `fts5_trigger_atomicity_under_crash`.
- **Disk full.** Per phase-01 pattern.

### §6.6 Scale & Performance

- **200 doctors FTS search.** `doctors::list(query='Lay', limit=10)` < 50 ms p99. The `doctors_fts` virtual table drives it. Per §9 default.
- **500 inventory items list.** `inventory_catalog::list` < 30 ms p99. The `inventory_items_active` partial index (per §7.7) drives it.
- **`effective_price` resolver.** Single resolve call < 5 ms p99 (PK lookup + at most 1 join).
- **`catalog:pricing_changed` event throughput.** During a bulk pricing-row import (100 rows), the event fires once per row; the listener applies the diff in < 10 ms per event.
- **Pull-time catalog apply.** A pull of 1000 catalog rows applies in < 500 ms p99. The bulk insert path uses prepared statements.

### §6.7 Security & Permissions

- **`/admin/*` route role gate.** Per §7.36: receptionist + accountant -> `/no-access`. Three-layer defence (route + `<UserMenu>` + IPC `require_role`).
- **JWT tampering on catalog push.** Server verifies RS256; tampered role -> 401.
- **FTS5 query injection.** Per phase-05 forward-ref: phase-03 owns the first FTS5 surface. Input `"Layla MATCH 'foo'"` treated as literal FTS query, not as MATCH syntax. Asserted in `doctors_fts_query_injection`.
- **Soft-delete bypass.** Catalog entities' soft-delete preserves the row; raw SELECT shows it; the IPC excludes it. Same as phase-01 pattern.
- **`set_active` role-gate.** Per §7.23 / §7.24: superadmin-only.
- **Server-side role gate on `/sync/push` mutations.** Per §7.36 server side: a non-superadmin JWT pushing a `check_types` mutation -> 403. Defence in depth.

### §6.8 Data Integrity

- **Migration replay forward.** `003_catalog.sql` idempotent on fresh + populated DB.
- **Migration replay against populated DB.** Pre-load phase-01 + phase-02 + 1 doctor + 1 pricing; replay; rows preserved; FTS5 reindexed.
- **FK enforcement.** Insert `doctor_check_pricing` with non-existent `doctor_id` -> FK violation.
- **Soft-delete cascade.** Per §7.22: operator -> specialties. Per §4: doctor -> pricings.
- **Paired partial unique indexes.** Per §7.20 + §7.21: blocking duplicates is the integrity invariant.
- **CHECK constraint enforcement.** XOR on `check_types`; price/cut_value ranges; on_dye_only-vs-dye_supported (service-level).
- **`sync_version` monotonicity.** Every mutation increments version by 1.
- **FTS5 trigger consistency.** After 1000 random insert/update/delete operations on `doctors`, the FTS5 index returns the same row set as a direct `SELECT name FROM doctors WHERE deleted_at IS NULL`. Asserted in `fts5_consistency_property_test`. Per §7.33.

---

## §7 Performance SLOs (this phase's surfaces)

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `doctors::list(query='Lay', limit=10)` over 200 doctors | < 50 ms p99 | yes | `perf_doctors_fts_at_200` | §9 FTS5 default. |
| Tauri (SQLite) | `inventory_catalog::list({is_active:true})` over 500 items | < 30 ms p99 | yes | `perf_inventory_catalog_active_at_500` | §9 default; index-driven via `inventory_items_active`. |
| Tauri (SQLite) | `pricing_resolve_effective_price` single call | < 5 ms p99 | yes | `perf_effective_price_single_call` | §9 single-PK default. |
| Tauri (SQLite) | `check_types::list` over 20 types | < 5 ms p99 | yes | `perf_check_types_list_small` | -- |
| Tauri (SQLite) | `operator::soft_delete` cascade with 5 specialties | < 30 ms p99 | no (tighter than §9's 200ms lock SLO because catalog cascades are bounded fan-outs) | `perf_operator_cascade_5_specialties` | -- |
| Tauri (SQLite) | `doctor::soft_delete` cascade with 10 pricings | < 50 ms p99 | no | `perf_doctor_cascade_10_pricings` | -- |
| Tauri (SQLite) | `with_audit` for a single catalog mutation | < 30 ms p99 | yes (inherits phase-01 default) | `perf_catalog_mutation_with_audit` | -- |
| Tauri (IPC) | Catalog mutation full round-trip | < 80 ms p99 | yes | `perf_catalog_mutation_ipc_round_trip` | §9 default. |
| Sync engine | Pull a 1000-row catalog batch | < 2 s p95 | yes | `perf_pull_1000_catalog_rows` | §9 default. |
| Sync server (Postgres) | `/sync/push` 50-op mixed catalog batch | < 200 ms p95 | yes | `perf_server_push_50_catalog_mixed` | -- |
| Sync server (Postgres) | `/sync/pull` 100-row catalog page | < 200 ms p95 | yes | `perf_server_pull_100_catalog_rows` | -- |
| Frontend | `<AdminPage>` (any of the 8 list pages) cold paint with 100 rows | < 200 ms | -- | `perf_admin_list_cold_paint_100` | -- |
| Frontend | `<DoctorPricingEditor>` cold paint with 10 pricing rows | < 150 ms | -- | `perf_doctor_pricing_editor_cold_paint` | -- |
| Frontend | `<ConsumptionMapEditor>` cold paint with 5 rows | < 150 ms | -- | `perf_consumption_map_editor_cold_paint` | -- |
| Frontend | `catalog:pricing_changed` event handler dispatch latency | < 20 ms p99 | -- | `perf_pricing_changed_handler_dispatch` | Phase-05's banner depends on this for snappy UX. |

---

## §8 Definition of Done

- [ ] All §1 unit tests green in CI.
- [ ] All §2 integration tests green in CI:
  - `cargo test --test catalog_phase03`
  - IPC handler tests for all 27 commands listed in §2.2.
  - `pnpm --filter sync-server test -- sync/catalog-phase03`
  - `vitest run --project integration`
- [ ] All §3 contract tests green in CI.
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- admin/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P3 Mariam the Superadmin** runs end-to-end and passes.
- [ ] §6 all eight edge categories addressed.
- [ ] §7 SLOs met for every row.
- [ ] Coverage gates met per §1.3.
- [ ] No open P0 or P1 defects.
- [ ] Snapshot files committed (9 catalog push snapshots listed in §3.3).
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green.

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P3 Mariam the Superadmin** | -- | -- | -- | -- |
| P2 Mehdi the Receptionist (reinforcement) | -- | -- | -- | Optional, verifies `/admin/*` role-gate redirect. |
| P1 Asma the Accountant (reinforcement) | -- | -- | -- | Optional, verifies `/admin/*` role-gate. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P03-G01 -- Prisma back-relations validation (HIGH)

- **Source:** phase-03.md §7.30 Prisma back-relations
- **Target test section:** §3.1
- **Category:** Missing Contract Test

The build spec mandates required back-relations on `CheckType`, `CheckSubtype`, `Doctor`, and `Operator` Prisma models. §3.1 currently validates request/response payloads but never asserts the Prisma schema itself passes `pnpm prisma validate`. Without this row, a missing back-relation regression compiles and ships, surfacing only at server boot.

| Route | Schema id | Sample payload |
|-|-|-|
| (Prisma schema build) | `prisma validate` | Run `pnpm --filter sync-server prisma validate`; assert exit 0 AND `grep -E '(check_subtypes|doctor_check_pricing|operator_specialties).*relation' sync-server/prisma/schema.prisma` returns expected back-relation fields per §7.30. |

### §9.2 P03-G02 -- `catalog:pricing_changed` emit coverage (HIGH)

- **Source:** phase-03.md §7.27 emit coverage
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1 currently asserts emit on `doctor_pricing::upsert` and on consumption-map mutations. §7.27 mandates emit on every check-type / check-subtype / doctor-pricing service write, including `CheckTypeService::update`, `CheckSubtypeService::create|update|soft_delete`, and `DoctorPricingService::soft_delete`. Phase-05's `<PricingChangedBanner>` depends on the full set; a missed emit silently leaks a stale draft.

| Scenario | Asserts |
|-|-|
| `check_type_service_update_emits_catalog_pricing_changed_kind_check_type` | After commit, listener receives `{ kind: 'check_type', check_type_id, changed_at }`. Per §7.27 + §7.35. |
| `check_subtype_service_create_emits_catalog_pricing_changed_kind_check_subtype` | Payload includes `check_subtype_id` AND parent `check_type_id`. |
| `check_subtype_service_update_emits_catalog_pricing_changed_kind_check_subtype` | -- |
| `check_subtype_service_soft_delete_emits_catalog_pricing_changed_kind_check_subtype` | -- |
| `doctor_pricing_service_soft_delete_emits_catalog_pricing_changed_kind_doctor_pricing` | Mirror of the upsert emit; payload retains `doctor_id`, `check_type_id`, `check_subtype_id`. |

### §9.3 P03-G03 -- Emit-after-audit ordering invariant (HIGH)

- **Source:** phase-03.md §7.27 emit-after-audit ordering
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.27 requires the `catalog:pricing_changed` event fire ONLY after `with_audit` commits the underlying SQLite transaction. A crash between business write and emit must leave subscribers unnotified rather than notified-but-rolled-back. §2.1 covers ordering of audit-vs-business writes but not emit-vs-commit ordering.

| Scenario | Asserts |
|-|-|
| `pricing_changed_emit_fires_only_after_with_audit_commit` | Instrument the emitter and the SQLite tx; the recorded emit timestamp is strictly greater than the tx-commit timestamp on every catalog mutation that emits. |
| `pricing_changed_emit_suppressed_when_with_audit_rolls_back` | Inject a panic between audit-row write and business-row write; assert tx rolls back AND emitter records zero emissions for that mutation. |
| `pricing_changed_emit_suppressed_on_sigkill_mid_commit` | Spawn a child process that SIGKILLs itself between the audit-row commit and the emit call; on restart, the WAL replays cleanly AND no `catalog:pricing_changed` listener fired (no spurious banner). |

### §9.4 P03-G04 -- Audit delta payload capture (HIGH)

- **Source:** phase-03.md §7.18 audit delta payload
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.18 requires `audit_log.delta` to capture before/after JSON for catalog updates. §2.1's current `with_audit_writes_audit_first_for_every_catalog_mutation` only asserts ordering -- not that the delta payload contains both states. Phase-08's audit viewer (forward-ref) depends on the shape.

| Scenario | Asserts |
|-|-|
| `check_type_update_audit_delta_contains_before_and_after_json` | Update a check type's `name_ar` from "A" to "B"; the corresponding `audit_log.delta` row deserializes to `{ before: { name_ar: 'A', ... }, after: { name_ar: 'B', ... } }` with full pre/post snapshots. Per §7.18. |
| `doctor_pricing_upsert_audit_delta_contains_before_null_for_insert_path` | First-write of a pricing row -> `delta.before === null`, `delta.after` is the inserted row. |
| `doctor_pricing_upsert_audit_delta_contains_before_for_update_path` | Re-upsert against an existing tuple -> `delta.before` is the prior row, `delta.after` is the new row. |
| `inventory_consumption_soft_delete_audit_delta_contains_after_with_deleted_at_set` | Soft-delete a consumption-map row -> `delta.after.deleted_at != null` AND `delta.before.deleted_at === null`. |

### §9.5 P03-G05 -- `doctors::list` `include_id` branch (MEDIUM)

- **Source:** phase-03.md §7.23 `<DoctorAutocomplete>` include_id
- **Target test section:** §2.2 / §2.4
- **Category:** Missing Integration Test

§7.23 specifies `doctors::list({active_only, include_id})` accepts an optional `include_id` that forces inclusion of the named doctor even if `is_active=0`. The branch supports the autocomplete on an in-progress draft whose original doctor has since been deactivated. §2.2 covers FTS happy/error paths but not this branch.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `doctors_list` (include_id) | `returns_inactive_doctor_when_include_id_matches_and_active_only_true` -- seed one inactive doctor `D1`; call `list({active_only: true, include_id: D1.id})`; D1 appears in the result alongside active doctors. Per §7.23. | `returns_validation_for_malformed_include_id_uuid` |

| Hook | Test | Asserts |
|-|-|-|
| `useDoctorsList({ activeOnly: true, includeId })` | `includes_draft_inactive_doctor_when_includeId_passed` | The hook surfaces the inactive doctor in the autocomplete list when `includeId` is supplied; omits it when `includeId` is undefined. |

### §9.6 P03-G06 -- `inventory_items::soft_delete` informational warning (MEDIUM)

- **Source:** phase-03.md §7.8 step 2 informational warning
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.8 step 2 specifies that `inventory_items::soft_delete` produces an INFORMATIONAL warning (not a block) when the item has adjustments within 90 days. §2.1 currently asserts the block path (`ReferencedByConsumptionMap`) but never the informational path -- a regression converting the warning to a block would ship undetected.

| Scenario | Asserts |
|-|-|
| `inventory_item_soft_delete_emits_informational_warning_for_recent_adjustments_but_does_not_block` | Seed inventory item I with one `inventory_adjustments` row dated 30 days ago (phase-06 forward-ref via raw insert); call `inventory_items::soft_delete(I)`; assert result is `Ok` with `warning: Some(InventoryItemWarning::RecentAdjustments { count: 1 })` AND the row's `deleted_at != null`. |
| `inventory_item_soft_delete_does_not_warn_for_adjustments_older_than_90d` | Seed adjustment dated 100 days ago; soft-delete returns `Ok` with `warning: None`. Per §7.8. |

### §9.7 P03-G07 -- LIKE-prefix query exercise (MEDIUM)

- **Source:** phase-03.md §7.15 LIKE-prefix query
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§7.15 declares `check_types::list` and `inventory_catalog::list` use LIKE-prefix search (not FTS5) on `query`. §2.2 currently asserts the min-2-chars validation but never the LIKE-prefix matching behaviour itself; a regression to substring or fuzzy match would not be caught.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `check_types_list` (query) | `like_prefix_query_matches_name_ar_and_name_en_from_start_only` -- seed types "MRI", "MRI Contrast", "Cardiac MRI"; `query="MRI"` returns the first two only (prefix match on `name_en`). Per §7.15. | -- |
| `inventory_catalog_list` (query) | `like_prefix_query_matches_unit_independent_substrings_in_name_only` -- prefix match against `name_ar` / `name_en`; the `unit` column is excluded from the search. | -- |

### §9.8 P03-G08 -- Outbox enqueue per catalog mutation (MEDIUM)

- **Source:** phase-03.md §7.3 step 5 outbox enqueue
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.3 step 5 mandates every catalog create/update/soft_delete enqueues an outbox row in the same SQLite transaction. §2.1 covers audit-row and emit assertions but does not pin the outbox enqueue per service; a regression skipping the outbox path leaves the row stranded on the device.

| Scenario | Asserts |
|-|-|
| `check_type_create_enqueues_outbox_row_in_same_tx` | After commit: one `outbox` row exists with `entity='check_types'`, `op='create'`, `entity_id=<new>` AND it shares the tx commit timestamp with the business row. |
| `check_type_update_enqueues_outbox_row_op_update` | -- |
| `check_subtype_create_update_softdelete_each_enqueue_one_outbox_row` | Three sequential mutations -> three outbox rows in order with matching `op` values. |
| `doctor_create_update_softdelete_each_enqueue_one_outbox_row` | -- |
| `doctor_pricing_upsert_enqueues_outbox_row_op_upsert` | -- |
| `operator_create_update_softdelete_each_enqueue_one_outbox_row` | -- |
| `operator_specialty_upsert_softdelete_each_enqueue_one_outbox_row` | -- |
| `inventory_catalog_create_update_softdelete_each_enqueue_one_outbox_row` | -- |
| `inventory_consumption_upsert_softdelete_each_enqueue_one_outbox_row` | -- |

### §9.9 P03-G09 -- Server `TENANT_MODELS` membership (MEDIUM)

- **Source:** phase-03.md §5 TENANT_MODELS
- **Target test section:** §3.3
- **Category:** Missing Contract Test

§5 declares the 8 new catalog table names (`check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`) must be added to the server's `TENANT_MODELS` array. §3.3 covers envelope and conflict-policy registry but does not assert membership; a missing entry breaks tenant filtering on `/sync/pull`.

| Aspect | Asserts |
|-|-|
| Server `TENANT_MODELS` contains all 8 phase-03 entities | Import `sync-server/src/app/domains/sync/config/tenant-models.ts`; assert the exported array contains each of: `'check_types'`, `'check_subtypes'`, `'doctors'`, `'doctor_check_pricing'`, `'operators'`, `'operator_specialties'`, `'inventory_items'`, `'inventory_consumption_map'`. Per §5. (Cross-ref: phase-08 §2.3 owns the final 15-entry assertion; this row pins phase-03's slice.) |

### §9.10 P03-G10 -- i18n error key inventory existence (MEDIUM)

- **Source:** phase-03.md §7.29 + §7.32 i18n error keys
- **Target test section:** §1.3 / §6.2
- **Category:** Missing Coverage Gate

§7.29 and §7.32 require the phase-03 error key inventory (`errors:catalog.*`, `errors:consumption.*`, `errors:doctor.*`, `errors:operator.*`, `errors:inventory.*`) exist in both `en` and `ar` resource bundles. §1.3 / §6.2 cover format helpers and en/ar swap but not key-inventory presence; a missing translation surfaces as a raw key in production.

| Aspect | Asserts |
|-|-|
| Phase-03 error keys present in en + ar (§1.3 coverage row) | `vitest` test imports `src/locales/en/errors.json` and `src/locales/ar/errors.json`; asserts every key in the phase-03 enumerated set exists in both files AND that no value is empty. Enumerated set: `catalog.xor_violation`, `catalog.subtypes_exist`, `catalog.parent_not_subtyped`, `catalog.subtype_required`, `catalog.subtype_forbidden`, `catalog.referenced`, `catalog.duplicate_pricing_row`, `catalog.pct_out_of_range`, `consumption.dye_not_supported_on_parent`, `consumption.quantity_per_check_invalid`, `doctor.empty_name`, `operator.cascade_specialties`, `inventory.empty_unit`, `inventory.referenced_by_consumption_map`, `inventory.low_stock_threshold_invalid`. Per §7.29 + §7.32. |
| §6.2 RTL row reinforcement | Each error message renders correctly with `dir=rtl` (no LTR-only punctuation leakage); asserted in §2.4's RTL parameterization. |

### §9.11 P03-G11 -- `handle.crumb` per admin detail route (MEDIUM)

- **Source:** phase-03.md §7.28 handle.crumb per admin detail route
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.28 requires every admin detail route declares a `handle.crumb` resolver that yields a breadcrumb segment per route via `resolveLocaleName`. §4.1 currently has one breadcrumb spec (`breadcrumb-resolves-locale-name.e2e.ts`); the build spec mandates verification on every detail route under `/admin/*`.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `admin-detail-breadcrumbs-per-route.e2e.ts` | Mariam (superadmin) | Navigate sequentially to `/admin/check-types/<id>`, `/admin/check-types/<id>/subtypes/<sid>`, `/admin/doctors/<id>`, `/admin/doctors/<id>/pricing/<pid>`, `/admin/operators/<id>`, `/admin/operators/<id>/specialties/<sid>`, `/admin/inventory/<id>`, `/admin/inventory/<id>/consumption/<cid>`. | At each route, the breadcrumb trail's last segment renders the entity's `resolveLocaleName` output (en + ar) AND clicking the prior segment navigates to the parent list. Per §7.28 + §7.16. |

### §9.12 P03-G12 -- List `includeInactive` flag (MEDIUM)

- **Source:** phase-03.md §3 list `includeInactive` flag
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§3 declares `doctors::list`, `operators::list`, and `inventory_catalog::list` each accept an `includeInactive` flag. §2.2 covers the default (active-only) branch but never exercises `includeInactive: true`; a regression collapsing the flag would silently hide deactivated entities from admin views.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `doctors_list` (includeInactive) | `returns_active_and_inactive_doctors_when_includeInactive_true` -- seed 2 active + 1 inactive doctor; `list({includeInactive: true})` returns all 3; `list({includeInactive: false})` returns 2. | -- |
| `operators_list` (includeInactive) | `returns_active_and_inactive_operators_when_includeInactive_true` | -- |
| `inventory_catalog_list` (includeInactive) | `returns_active_and_inactive_items_when_includeInactive_true` | -- |

### §9.13 P03-G13 -- `<InventoryAdminTable>` audit-log join (LOW)

- **Source:** phase-03.md §7.13 `<InventoryAdminTable>` audit-log join
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.13 specifies the `<InventoryAdminTable>` joins `inventory_items` to the audit log to render a "last edited by" column. §2.4 lists the component as covered but does not assert the join's correctness; a regression dropping the join surfaces as a blank column.

| Hook | Test | Asserts |
|-|-|-|
| `useInventoryCatalogWithLastEditor` | `joins_inventory_items_to_audit_log_for_last_edit_author_column` | Seed one item with two audit rows (created by user A, updated by user B); the hook returns the row with `lastEditedBy.userId === B.id` AND `lastEditedBy.displayName === B.name`. The `<InventoryAdminTable>` renders the name in the last-edit-author column. Per §7.13. |

### §9.14 P03-G14 -- `CutKind` enum wire mapping (LOW)

- **Source:** phase-03.md §2 `CutKind` enum
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§2 declares the Prisma `CutKind { pct, fixed }` enum; the wire format must match the Rust serde tag and TS Zod literal union. §3.1 covers payload schemas but not the enum-value mapping; a regression converting `pct` to `percent` on either side would break push/pull silently for pricing rows.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (request) | `DoctorPricingPushSchema.cut_kind` | Ajv validation accepts only `'pct'` or `'fixed'`; rejects `'percent'`, `'fixed_amount'`, uppercase variants. `fixtures/payloads/doctor-pricing-cut-kind-invalid-push.json` MUST fail. |
| `GET /sync/pull` (response) | `DoctorPricingResponseSchema.cut_kind` | Round-trip a row created with `cut_kind='pct'`; assert the pulled row deserializes identically AND that `CutKindSchema.parse('pct').is_ok()` on the TS side. |

### §9.15 P03-G15 -- Catalog pull canonical snapshots (LOW)

- **Source:** phase-03.md §10 catalog pull canonicals
- **Target test section:** §3.3
- **Category:** Missing Snapshot

§3.3 lists 9 push canonical snapshot files but no pull canonicals. §10 mandates the pull side has equivalent locked snapshots so that a regression in `pulledAt` placement, field ordering, or version bump is caught byte-exactly.

| Aspect | Asserts |
|-|-|
| Pull canonical snapshots committed (mirror of §3.3 push list) | Each of the following exists under `expected/sync/` and is byte-validated in §3.3: `check-type-flat-pull-canonical.json.sha256`, `check-type-subtyped-pull-canonical.json.sha256`, `check-subtype-pull-canonical.json.sha256`, `doctor-pull-canonical.json.sha256`, `doctor-pricing-pull-canonical.json.sha256`, `operator-pull-canonical.json.sha256`, `operator-specialty-pull-canonical.json.sha256`, `inventory-item-pull-canonical.json.sha256`, `inventory-consumption-map-pull-canonical.json.sha256`. Each captures the canonical row from a seeded tenant including `pulledAt` per §7.19. Per §10. |

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). Pass 2 re-audited phase-03 after Pass 1's 15 additions landed; this section captures the residual exposures that survived Pass 1's net. Format mirrors §9 exactly: every subsection names a source build-spec ref, target test section, category, narrative justification, and a concrete test row table.

### §10.1 P03-G16 -- Consumption-map upsert audit delta capture (HIGH)

- **Source:** phase-03.md §7.10 consumption-map audit writes
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.10 mandates that `ConsumptionMapService::upsert` writes a `with_audit` row with `action ∈ {create, update}` and a before/after delta payload, mirroring §7.18's catalog-wide rule. §9.4 (P03-G04) covers the consumption-map `soft_delete` delta only; it does not pin the `create` path (where `delta.before === null`) nor the `update` path (where both halves are present) for the upsert command itself. A regression that omitted the upsert audit row or stubbed its delta would slip past every Pass 1 row because the §9.4 scenarios all target other entities.

| Scenario | Asserts |
|-|-|
| `consumption_upsert_create_path_audit_delta_before_null_after_inserted_row` | First upsert against `(item, check_type, NULL)`: `audit_log` row exists with `entity='inventory_consumption_map'`, `action='create'`, `delta.before === null`, `delta.after` deserializes to `{ item_id, check_type_id, check_subtype_id: null, quantity_per_check: <n>, on_dye_only: <bool>, deleted_at: null }`. Per §7.10 step 5 + §7.18. |
| `consumption_upsert_update_path_audit_delta_carries_full_before_and_after` | Re-upsert the same tuple with a different `quantity_per_check`: `audit_log` row has `action='update'`, `delta.before` is the prior row snapshot, `delta.after` is the new row snapshot; `quantity_per_check` differs across the two halves. |
| `consumption_upsert_audit_delta_records_on_dye_only_flip_independently` | Upsert flipping only `on_dye_only` from `0` to `1`: `delta.before.on_dye_only === false`, `delta.after.on_dye_only === true`, `quantity_per_check` identical across before/after. |

### §10.2 P03-G17 -- `<ActiveDraftsBadge>` listener subscription (HIGH)

- **Source:** phase-03.md §7.27 emit listeners
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.27 names two `catalog:pricing_changed` listeners: `<PricingChangedBanner>` AND `<ActiveDraftsBadge>`. §2.4 wires the banner listener test (`useDoctorPricingUpsert.emits_catalog_pricing_changed_event_via_listener`) but the badge subscription is not asserted. The badge is what tells the receptionist that drafts they have parked are now affected by a pricing change; a regression unsubscribing it would silently leave receptionists staring at unaffected counters while drafts go stale.

| Hook | Test | Asserts |
|-|-|-|
| `<ActiveDraftsBadge>` (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `active_drafts_badge_subscribes_to_catalog_pricing_changed_and_increments_affected_count` | Mount `<ActiveDraftsBadge>` with mocked IPC returning 3 open drafts (2 with `doctor_id=D1`, 1 with `doctor_id=D2`). Emit `catalog:pricing_changed { kind: 'doctor_pricing', doctor_id: D1, check_type_id: T, check_subtype_id: null, changed_at }`. The badge's "affected" count increments from 0 to 2; drafts on D2 are not flagged. Per §7.27 + §7.35 payload scope intersection. |
| `<ActiveDraftsBadge>` | `active_drafts_badge_unsubscribes_on_unmount_and_does_not_leak_listener` | Mount the badge, unmount it, then emit a `catalog:pricing_changed` event. No exception (no stale listener); a remounted badge re-subscribes and observes subsequent emits. |

### §10.3 P03-G18 -- `prisma migrate status` clean post-§7.20/§7.21 (HIGH)

- **Source:** phase-03.md §7.31 raw-SQL migration ordering
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.31 requires `pnpm prisma validate` AND `pnpm prisma migrate status` to be clean after each phase's raw-SQL migration land. §2.3 asserts migrations apply in lex order, and §9.1 (P03-G01) asserts `prisma validate` exit-0 on the schema in isolation, but neither asserts that after the §7.20 drop-then-create pair AND the §7.21 drop-then-create pair both run, `prisma migrate status` reports a clean state with no drift and no pending diff. A regression where the partial-unique migration was authored but never paired with a `_drop_unique_*` predecessor would leave the deployment in a `migration drift` state that only surfaces on production `prisma migrate deploy`.

| Route | Schema id | Sample payload |
|-|-|-|
| (Prisma migration state) | `prisma migrate status` after §7.20 pair | Boot a Postgres test DB; apply all phase-01 + phase-02 migrations; apply the §7.20 ordered pair (`<ts>_drop_unique_doctor_check_pricing/migration.sql` then `<ts+1>_partial_unique_doctor_check_pricing/migration.sql`); run `pnpm --filter sync-server prisma migrate status`. Exit 0; output contains "Database schema is up to date" AND no entry listed under "Following migrations have not yet been applied". Per §7.31. |
| (Prisma migration state) | `prisma migrate status` after §7.21 pair | Same as above for the §7.21 `inventory_consumption_map` pair. Both pairs combined leave the DB clean. |
| (Prisma schema validity) | `prisma validate` after both pairs applied | `pnpm --filter sync-server prisma validate` exits 0 against the schema with both `@@unique` blocks stripped (replaced by raw-SQL partial unique indexes). Per §7.31. |

### §10.4 P03-G19 -- Soft-deleted doctor with matching `include_id` still excluded (HIGH)

- **Source:** phase-03.md §7.23 + §1 doctors WHERE clause
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§7.23 specifies the `doctors::list` body filters via `WHERE deleted_at IS NULL AND (is_active = 1 OR id = :includeId)`. §9.5 (P03-G05) covers the `include_id` branch returning an INACTIVE doctor (where `is_active=0` but `deleted_at IS NULL`); it does NOT exercise the precedence rule that a SOFT-DELETED doctor (`deleted_at IS NOT NULL`) matching `include_id` is STILL excluded. The two predicates are AND-joined; a regression converting them to OR would leak tombstoned doctors into the receptionist's autocomplete, breaking PRD §6.1.4 inv 2.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `doctors_list` (include_id vs deleted) | `excludes_soft_deleted_doctor_even_when_include_id_matches` -- seed one soft-deleted doctor `D1` (`deleted_at = now()`, `is_active = 0`); call `list({active_only: true, include_id: D1.id})`; result MUST NOT contain D1. Per §7.23 AND-joined predicates. | `excludes_soft_deleted_doctor_even_when_include_id_matches_and_active_only_false` -- same setup; call `list({active_only: false, include_id: D1.id})`; D1 STILL excluded because `deleted_at IS NULL` is unconditional. |

### §10.5 P03-G20 -- Partial-batch push per-op result envelope (HIGH)

- **Source:** phase-03.md §7.1 XOR violation + §6.3 partial-batch push
- **Target test section:** §6.3
- **Category:** Missing Contract Test

§6.3 declares partial-batch push as an edge category (offline-and-network surface). §4.2's `xor-violation-rejected-at-three-layers.e2e.ts` covers the single-op rejection but never the mixed-success batch shape: when 27 ops push together and op #14 fails the XOR refinement, the server must return a per-op result envelope `{ op_id, status: 'ok' | 'error', error?: AppErrorBody }` so the client can park the failed op without rolling back the 26 successes. Without an asserted shape, a regression collapsing the envelope to a single batch-level 422 would force a full-batch retry loop.

| Scenario | Asserts |
|-|-|
| `push_partial_batch_returns_per_op_status_envelope_with_one_xor_violation` | Push 27 check-type ops where op index 14 carries an XOR violation (`has_subtypes=1 AND base_price_iqd != null`); server response body is `{ results: [...] }` with exactly 27 entries; entries 0-13 and 15-26 have `{ op_id, status: 'ok' }`; entry 14 has `{ op_id, status: 'error', error: { kind: 'XorViolation', message: <i18n key> } }`. HTTP status of the batch is 200 (per-op semantics, not all-or-nothing). |
| `push_partial_batch_persists_succeeded_ops_only_and_leaves_failed_op_in_outbox` | After the response, server DB contains rows for 26 ops; the failed op #14 row is absent; client's `outbox` retains op #14 with `last_error` populated for manual review. Per §6.3 + §7.1. |
| `push_partial_batch_result_order_matches_request_order_by_op_id` | The `results` array is ordered by request-side `op_id`, not by success/failure; clients can zip request and response by index. |

### §10.6 P03-G21 -- `pulledAt` server-only, absent from client response (MEDIUM)

- **Source:** phase-03.md §7.19 server-only `pulledAt`
- **Target test section:** §3.3
- **Category:** Missing Contract Test

§7.19 declares `pulledAt` as a server-side diagnostic column "not exposed to clients". §9.15 (P03-G15) commits snapshot fixtures whose captured rows INCLUDE `pulledAt` because they are captured from the server's internal canonical view. The two statements are in tension: the snapshot is the server's view, but the wire-format client response must strip `pulledAt`. Without an explicit test asserting the client-facing response schema OMITS `pulledAt`, a regression leaking the column would not be detected by either §3.3 or §9.15.

| Aspect | Asserts |
|-|-|
| Client-facing pull response omits `pulledAt` | `GET /sync/pull` integration test: capture the JSON response body for each of the 8 phase-03 entity types; for every row, `'pulledAt' in row === false` AND `'pulled_at' in row === false`. Per §7.19 "Used for diagnostics only; not exposed to clients". |
| Server's internal canonical snapshot retains `pulledAt` | §9.15's `expected/sync/<entity>-pull-canonical.json.sha256` fixtures (which capture the server-side row, not the wire format) DO include `pulledAt`; the two paths are intentionally divergent. The harness uses two fetchers: `fetchServerCanonical()` for snapshots, `fetchClientResponse()` for the wire-format contract row. Per §7.19. |

### §10.7 P03-G22 -- Composite IPC schema names declared (MEDIUM)

- **Source:** phase-03.md §3 composite IPC returns
- **Target test section:** §3.2
- **Category:** Missing Contract Test

§3.2's table names `DoctorWithPricingsSchema` for the `doctors_get` composite return shape `{ doctor, pricings }`. The table mentions `operators_get` with shape `{ operator, specialties }` and `inventory_catalog_get` with shape `{ item, consumption }` but does not declare named schemas for them. Without a declared schema name, the IPC shape diff harness in §3.2 cannot pin the composite shape; a regression dropping `specialties` from the operator composite would slip past contract tests.

| IPC command | Rust struct | TS schema |
|-|-|-|
| `operators_get` | `{ operator: Operator, specialties: Vec<OperatorSpecialty> }` | (NEW) `OperatorWithSpecialtiesSchema = z.object({ operator: OperatorSchema, specialties: z.array(OperatorSpecialtySchema) })` |
| `inventory_catalog_get` | `{ item: InventoryItem, consumption: Vec<InventoryConsumptionMap> }` | (NEW) `InventoryItemWithConsumptionSchema = z.object({ item: InventoryItemSchema, consumption: z.array(InventoryConsumptionMapSchema) })` |
| (shape diff) | Both new schemas | The §3.2 IPC shape-diff harness validates Rust serde JSON for `operators_get` and `inventory_catalog_get` against the two new Zod schemas; a regression dropping either composite field fails the diff. Per §3.2 + §3. |

### §10.8 P03-G23 -- `check_types::list({includeDeleted: true})` happy path (MEDIUM)

- **Source:** phase-03.md §3 IPC + §1 check_types
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§3's IPC table declares `check_types::list({ includeDeleted: bool })`. §2.2 mentions the command in passing as "list optionally_including_deleted" but never exercises the `includeDeleted: true` branch with a concrete assertion: that soft-deleted (`deleted_at != NULL`) rows DO appear in the result. A regression collapsing the flag to a no-op would silently hide tombstoned check types from the audit-restoration workflow that phase-08 builds atop.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `check_types_list` (includeDeleted) | `returns_soft_deleted_check_types_when_includeDeleted_true` -- seed 2 active + 1 soft-deleted check type (`deleted_at = now()`); `list({includeDeleted: true})` returns all 3 with the deleted row carrying `deleted_at != null`; `list({includeDeleted: false})` returns 2 with `deleted_at === null` on each. | `omits_soft_deleted_check_types_by_default` -- `list({})` (flag omitted) defaults to `includeDeleted: false` per §3 IPC signature; soft-deleted row excluded. |

### §10.9 P03-G24 -- Local clock-skewed write overwritten by server `updated_at` on pull (MEDIUM)

- **Source:** phase-03.md §6.1 + §7.17 LWW
- **Target test section:** §2.1 / §6.1
- **Category:** Missing Integration Test

§6.1 declares "pulled `updated_at` is server-authoritative". §7.17 declares LWW with `originDeviceId` tiebreak on equal millisecond timestamps. No scenario currently exercises the clock-skew path: a client whose local clock is ahead of the server writes a row at local-T2 (future from server view), pushes it, the server stamps it with server-T1, the pull-back response replaces the local `updated_at` with server-T1. Without this test, a regression that retained the local future timestamp on pull-apply would silently break LWW ordering across the fleet.

| Scenario | Asserts |
|-|-|
| `pull_apply_overwrites_local_future_dated_updated_at_with_server_authoritative_timestamp` | Set client clock to `2026-05-13T10:00:00Z`; set server clock to `2026-05-13T09:00:00Z` (client is 1 hour ahead). Create a check type locally with `updated_at = 2026-05-13T10:00:00Z`; push to server (server stamps it at its own `now()`, `2026-05-13T09:00:00Z`); pull the row back. Local SQLite's `updated_at` for the row is now `2026-05-13T09:00:00Z` (server-authoritative), NOT the original local future timestamp. Per §6.1 + §7.17. |
| `pull_apply_preserves_originDeviceId_for_lww_tiebreak_after_clock_correction` | Same setup; assert `origin_device_id` on the pulled row matches the writer's device ID, not the server's; LWW tiebreak still resolves to the original author on a same-millisecond conflict. Per §7.17. |

### §10.10 P03-G25 -- Concrete delete-vs-edit conflict scenario (MEDIUM)

- **Source:** phase-03.md §6.4 concurrency narrative
- **Target test section:** §6.4
- **Category:** Missing E2E Scenario

§6.4 narratively mentions a delete-vs-edit case (incoming edit at T1 vs local soft-delete at T2 -> deletion wins) but provides no concrete scenario row in the §6.4 table; the existing rows cover lex tiebreak and paired-unique conflicts only. Without a deterministic E2E, a regression that resurrected a tombstoned doctor when an inbound edit's payload arrived would slip past every Pass 1 row, because §9.x adds nothing for this surface.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `two-device-doctor-edit-vs-soft-delete-deletion-wins.e2e.ts` | Mariam on Device A; Mariam on Device B | 1) Seed doctor `D1` with `updated_at = T0` on both devices. 2) Take both devices offline. 3) On Device A at T1, edit `D1.name` from "Layla" to "Layla A.". 4) On Device B at T2 (T2 > T1), soft-delete `D1` (`deleted_at = T2`). 5) Reconnect Device A first; it pushes the edit; server accepts; `updated_at = T1` server-side. 6) Reconnect Device B; it pushes the soft-delete; server resolves the LWW comparison (T2 > T1) AND honors the deletion-wins tiebreak per §6.4 narrative: tombstone wins over a strictly-older non-tombstone edit. 7) Both devices pull. | Server-side `D1.deleted_at` is set to T2; `D1.name` is "Layla A." (the edit's value persists in the tombstoned row's payload, but `deleted_at` is the operative field). Device A's local row now shows `deleted_at = T2` AND `D1` disappears from `<DoctorAutocomplete>`; Device B unchanged. Per §6.4 + §7.17. |

### §10.11 P03-G26 -- `unit` whitespace-only validation (MEDIUM)

- **Source:** phase-03.md §7.5 + §1 inventory_items unit CHECK
- **Target test section:** §2.1
- **Category:** Missing Edge Coverage

§7.5 mandates `CHECK (length(trim(unit)) > 0)` on `inventory_items.unit`. §9.6 (P03-G06) covers the empty-string case and the §1 CHECK declaration covers literal `""`. The `trim()` predicate is the load-bearing piece for whitespace-only strings: `"\t"`, `"\n"`, `"   "`, and Unicode zero-width whitespace (`​`). Without explicit coverage, a regression that dropped `trim()` would accept whitespace-only units and surface them as blank cells in admin tables.

| Scenario | Asserts |
|-|-|
| `inventory_item_create_rejects_unit_with_only_tabs` | Submit `inventory_items::create { unit: "\t\t" }`; result is `Err(InventoryItemError::EmptyUnit)`; SQLite CHECK constraint also fires if service guard is bypassed. Per §7.5. |
| `inventory_item_create_rejects_unit_with_only_newlines_and_spaces` | Submit `unit: "  \n\r  "`; same rejection. |
| `inventory_item_create_rejects_unit_with_only_zero_width_whitespace` | Submit `unit: "​​"` (two zero-width spaces); rejected at the service layer (`unit.trim()` collapses to empty after stripping ZWSP via `char::is_whitespace`). The error message is the same `errors:inventory.empty_unit` i18n key. |
| `inventory_item_create_accepts_unit_with_leading_trailing_whitespace_after_trim` | Submit `unit: "  ml  "`; succeeds; the persisted row's `unit` is `"ml"` (trimmed before insert) per §7.5. |

### §10.12 P03-G27 -- Per-entity LWW policy registry contract (MEDIUM)

- **Source:** phase-03.md §7.17 LWW per-entity + §4 sync semantics
- **Target test section:** §3.3
- **Category:** Missing Contract Test

§3.3 declares "all 8 entities are `last-write-wins`" as a single narrative line. §7.17 mandates the rule be re-stated per-entity in the sync-policy registry so the dispatcher can look it up by entity name. No contract test currently enumerates the 8 entity table names and asserts the registry returns `'last-write-wins'` for each; a regression that omitted one entity from the registry would default it to `manual` and route conflicts to a dead-letter queue silently.

| Aspect | Asserts |
|-|-|
| Phase-03 conflict-policy registry covers all 8 entities | Import `sync-server/src/app/domains/sync/config/conflict-policies.ts` (or equivalent registry). For each of the 8 table names -- `'check_types'`, `'check_subtypes'`, `'doctors'`, `'doctor_check_pricing'`, `'operators'`, `'operator_specialties'`, `'inventory_items'`, `'inventory_consumption_map'` -- assert `getConflictPolicy(table) === 'last-write-wins'`. Per §7.17. |
| Tiebreak rule embedded in registry entry | Each registry entry exposes `{ policy: 'last-write-wins', tiebreak: 'origin_device_id_lex_min' }`; assertion via shape equality. Per §7.17 + phase-01 §4 SyncEngine. |
| Client-side mirror agrees | `src/lib/sync/conflict-policies.ts` exports the same 8 entries; a contract test diffs the two registries (server vs client) and fails on any divergence. Per §3.3. |

### §10.13 P03-G28 -- Settings -> emitter -> banner cross-phase chain (MEDIUM)

- **Source:** phase-03.md §7.27 + §7.35 + phase-02 §7.4
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.27 declares emission of `catalog:pricing_changed { kind: 'settings' }` when phase-02's `settings.internal_doctor_pct` changes. §5.1 mentions this cross-phase coupling in a manual review bullet but no §4.1 E2E exercises the chain end-to-end: phase-02 settings write -> phase-03 emitter -> phase-05 banner render. The chain spans three phases; without an automated spec, a regression breaking any link silently degrades the banner's coverage of the settings-driven price-change path.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `settings-change-fires-pricing-changed-banner-via-phase-03-emitter.e2e.ts` | Mariam (superadmin) on Device A; Mehdi (receptionist) on Device A | 1) As Mehdi, open a draft visit referencing a check-type with no doctor-override (price falls back to `settings.internal_doctor_pct`); leave it parked. 2) Switch to Mariam; navigate to `/admin/settings`; edit `internal_doctor_pct` from `0.40` to `0.45`; save. 3) Settings write commits; phase-03 emitter fires `catalog:pricing_changed { kind: 'settings', changed_entity_id: <settings_id>, check_type_id: null, check_subtype_id: null, doctor_id: null, changed_at }` (per §7.35 payload schema; `kind='settings'` carries null entity-scope fields because the change is global). 4) Switch back to Mehdi's session. | Phase-05's `<PricingChangedBanner>` (forward-ref) renders on Mehdi's open draft because the settings kind matches any draft whose effective-price resolution chains through `internal_doctor_pct`. Per §7.27 + §7.35 + phase-02 §7.4 cross-coupling. The §5.1 manual review bullet becomes the automated counterpart. |

### §10.14 P03-G29 -- Push envelope header snapshot (LOW)

- **Source:** phase-03.md §10 envelope versioning + §3.3
- **Target test section:** §3.3
- **Category:** Missing Snapshot

§3.3 declares `envelope_version: 1` and commits 9 per-entity push payload snapshots, but no snapshot captures the full envelope shape (header + ops array). A regression bumping `envelope_version` to `2` without coordinated client/server change, or reordering header fields, would not be caught by any per-entity snapshot because those snapshots only canonicalize the row payload, not the wrapper.

| Aspect | Asserts |
|-|-|
| Full push envelope snapshot committed | New file `expected/sync/push-envelope-v1-canonical.json.sha256` captures the canonicalized JSON of a full push envelope: `{ envelope_version: 1, tenant_id, device_id, batch_id, ops: [<2 ops>] }`. Two ops included (one check-type create, one doctor-pricing upsert) so the per-op shape inside the envelope is also locked. Per §10 envelope versioning. |
| Pull envelope snapshot committed | New file `expected/sync/pull-envelope-v1-canonical.json.sha256` captures the symmetric pull envelope: `{ envelope_version: 1, tenant_id, server_clock, rows: [...] }`. Both files added to §8 DoD snapshot list. Per §3.3. |

### §10.15 P03-G30 -- Admin sub-sidebar manual checklist (LOW)

- **Source:** phase-03.md §7.11 admin 7-area enumeration
- **Target test section:** §5.1
- **Category:** Manual Step

§7.11 mandates the `<AdminShell>` sub-sidebar list exactly 7 areas in the canonical order: Users, Check Types, Doctors, Operators, Inventory, Settings, Audit. §5.1 currently says "7 items in the canonical order" as a single bullet but does not enumerate names or order in a checklist form for manual review. A regression renaming, reordering, or duplicating an area would slip past visual review without an explicit name-by-name checklist.

| Item | Manual check |
|-|-|
| Sub-sidebar order locked, top-to-bottom | Manual reviewer opens `/admin` and confirms the sub-sidebar reads, in this exact order: (1) Users, (2) Check Types, (3) Doctors, (4) Operators, (5) Inventory, (6) Settings, (7) Audit. No additional entries; no missing entries; no reorder. Per §7.11. Run once in `dir=ltr` (top-down) and once in `dir=rtl` (the column is on the right edge but the vertical order is unchanged). |
| Active highlight follows the route | At each of the 7 areas, the matching sub-sidebar item carries the active highlight (per design-system §10 sidebar conventions); all others are at the inactive token. |
| `<UserMenu>` hides the entire sub-sidebar for non-superadmin | As Mehdi (receptionist), confirm `<UserMenu>` shows no `/admin/*` shortcut and direct URL navigation lands on `/no-access`. Per §7.36. |

### §10.16 P03-G31 -- Empty hook test entries filled (LOW)

- **Source:** phase-03.md §3 hooks list
- **Target test section:** §2.4
- **Category:** Incomplete Coverage

§2.4's hook table lists `useCheckSubtypesByType` and `useOperator(id)` (the latter implicitly via `useOperatorsList` siblings) but both rows have empty test entries (`-- | --`). The hooks are declared in §3's frontend hook inventory but no behaviour is asserted. A regression that broke the by-type filtering or the single-operator fetch path would slip past every contract and component test because no assertion exists.

| Hook | Test | Asserts |
|-|-|-|
| `useCheckSubtypesByType(checkTypeId)` | `caches_under_catalog_checkSubtypes_byType_<checkTypeId>_key` | The query key under React Query devtools is `['catalog', 'checkSubtypes', 'byType', <checkTypeId>]`; returns `Vec<CheckSubtype>` filtered to the parent check type; soft-deleted subtypes excluded by default. Per §3 + §7.2. |
| `useCheckSubtypesByType(checkTypeId)` | `returns_empty_array_for_flat_check_type_without_emitting_error` | When called against a check type with `has_subtypes=0`, the hook returns `[]` (not `undefined`, not an error); the underlying IPC call MUST short-circuit OR return an empty list -- either is acceptable but the hook's surface MUST be `[]`. Per §7.2 parent-state guard. |
| `useOperator(id)` | `fetches_single_operator_with_specialties_via_operators_get_composite_command` | Mounts the hook with a known operator id; the IPC mock receives `operators_get { id }`; the hook resolves to `{ operator, specialties }` matching the §10.7 `OperatorWithSpecialtiesSchema`. Cache key `['catalog', 'operators', 'detail', <id>]`. |
| `useOperator(id)` | `invalidates_on_useOperatorSoftDelete_and_returns_tombstoned_marker` | After `useOperatorSoftDelete(id).mutate()` resolves, the `useOperator(id)` cache entry is invalidated and the next read returns `{ operator: { ..., deleted_at: <ts> }, specialties: [] }` (specialties cascaded per §7.22). |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 8 Phase-03 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P03-G32 through P03-G39). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps. P03-G32 is the SINGLE Pass 3 CRITICAL gap across all 9 phases -- it gates first test authoring on phase-03.

### §11.1 P03-G32 -- Server /sync/push injects entityIdTenant from JWT (CRITICAL)

- **Source:** phase-03.md §4 Sync Server -- "writes the row with entityIdTenant = tenantId" (tenantId being the JWT claim).
- **Target test section:** §2.3
- **Category:** Missing Contract Test

The multi-tenant invariant is load-bearing: a push body carrying `entity_id` MUST be rewritten to the JWT's tenant before insert. §2.3 covers pull-side filtering but never asserts the push-side rewrite. A regression accepting payload-supplied `entity_id` is a cross-tenant data leak.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `push_overrides_payload_entity_id_with_jwt_tenant_for_every_catalog_entity` | For each of the 8 catalog entity types (doctors, operators, check_types, check_subtypes, doctor_check_pricing, operator_specialties, inventory_catalog, inventory_consumption_map): authenticate with a JWT bearing `entityId='tenant-A'`. POST a push payload with `entity_id='tenant-B'` (forged) in the body. Assert: (a) response 200 -- the push is accepted (no error); (b) the inserted row has `entity_id='tenant-A'` (the JWT value, NOT the body value); (c) querying the table from a `tenant-B` JWT returns zero rows for this id; (d) querying from a `tenant-A` JWT returns the row. Per §4 Sync Server tenant-injection rule. Mirror at §3.1 with a contract row asserting the request schema does NOT require `entity_id` (server derives it). |

### §11.2 P03-G33 -- Business-row sync columns (dirty=1, version+1) per catalog mutation (HIGH)

- **Source:** phase-03.md §7.3 step 4 + every catalog mutation -- "Mutation sets `dirty=1` and bumps `version` by 1 before commit".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§9.8 covers outbox emission; §10.16 spot-checks `Doctor::set_active`. No scenario asserts the `dirty=1` / `version+1` invariant across all 8 entities for create / update / soft_delete.

| Scenario | Asserts |
|-|-|
| `every_catalog_mutation_flips_dirty_and_bumps_version_across_all_8_entities` | Parametrize over the 8 entity types and the 3 mutation kinds (create, update, soft_delete). For each: seed a row (skip for create), capture `(dirty_before, version_before)`, run the mutation through the service, assert the resulting row has `dirty=1` AND `version = version_before + 1` (for create: `version=0`, `dirty=1`). A regression that skipped the dirty flag strands the row from the sync engine -- audit and outbox would still land per §9.8, but the sync engine would never ship it. 24 sub-cases total. Per §7.3 step 4. |

### §11.3 P03-G34 -- OperatorSpecialtyPicker diff algorithm (HIGH)

- **Source:** phase-03.md §4 Frontend `<OperatorSpecialtyPicker>` step 2 -- "Diff the picker's selection set against current rows; dispatch upserts for added, soft_deletes for removed".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

The diff algorithm is described but never asserted. A regression that re-upserts unchanged rows (audit-log spam) or drops a removal silently lands.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<OperatorSpecialtyPicker>` | `diff_algorithm_emits_minimum_upserts_and_soft_deletes_for_set_difference` | Render the picker with `current = [{id:1,checkType:A}, {id:2,checkType:B}, {id:3,checkType:C}]`. User selects `[A, C, D]` (kept A, removed B, kept C, added D). Submit. Assert the dispatched IPC sequence is EXACTLY: 1x `operator_specialties_upsert { checkType: D }`, 1x `operator_specialties_soft_delete { id: 2 }`. NO call for A or C (no-op). NO call for `id=3` (kept). Order independent. Per §4 Frontend step 2. |

### §11.4 P03-G35 -- useDoctor / useCheckType / useInventoryItem singular hooks (MEDIUM)

- **Source:** phase-03.md §3 React Query keys table -- `useDoctor(id)`, `useCheckType(id)`, `useInventoryItem(id)`.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§10.16 added `useOperator(id)` and `useCheckSubtypesByType` but three sibling singular-detail hooks remain.

| Hook | Test | Asserts |
|-|-|-|
| `useDoctor(id)` | `fetches_with_doctors_get_returning_doctor_and_pricings` | IPC mock receives `doctors_get { id }`; resolves to `{ doctor, pricings }` matching the §10.7 `DoctorWithPricingsSchema`. Cache key `['catalog','doctors','detail', <id>]`. |
| `useCheckType(id)` | `fetches_with_check_types_get_and_caches_under_singular_key` | IPC mock receives `check_types_get { id }`; resolves to `CheckType`. Cache key `['catalog','checkTypes','detail', <id>]`. Returns subtypes inline iff `has_subtypes=1`. |
| `useInventoryItem(id)` | `fetches_with_inventory_catalog_get_returning_item_and_consumption` | IPC mock receives `inventory_catalog_get { id }`; resolves to `{ item, consumption }` matching §10.7 `InventoryItemWithConsumptionSchema`. Cache key `['catalog','inventoryItems','detail', <id>]`. |

### §11.5 P03-G36 -- sort_order drag handle + check_types_sort index (MEDIUM)

- **Source:** phase-03.md §3 list pages "List with sort_order drag handle" + §1 `check_types_sort` partial index.
- **Target test section:** §2.1 / §2.4
- **Category:** Missing Integration Test

The reorder path and the index that supports it are unverified.

| Scenario | Asserts |
|-|-|
| `check_type_sort_order_reorder_writes_audit_and_outbox_per_affected_row_atomically` | Seed 5 check_types with `sort_order in [10, 20, 30, 40, 50]`. Drag row 5 to position 1. IPC dispatches an atomic `check_types_reorder { newOrder: [id5, id1, id2, id3, id4] }`. Assert: all 5 rows updated in ONE transaction; new `sort_order` values are `[10, 20, 30, 40, 50]` re-assigned in array order (or strictly increasing); 5 audit rows (`action='update', delta.sort_order={from,to}`); 5 outbox rows. A mid-tx failure leaves NO rows mutated. |
| `check_types_sort_index_used_by_order_by` | `EXPLAIN QUERY PLAN SELECT * FROM check_types WHERE deleted_at IS NULL ORDER BY sort_order ASC` -- assert plan mentions `USING INDEX check_types_sort`; no `SCAN TABLE check_types`. Per §1 + §3. |

### §11.6 P03-G37 -- HasSubtypesToggle atomicity at UI mutation seam (MEDIUM)

- **Source:** phase-03.md §4 Frontend `<HasSubtypesToggle>` step 2 -- "on confirm set base_price_iqd = null AND save in one mutation".
- **Target test section:** §2.4
- **Category:** Missing Edge Coverage

Rust-side atomicity covered by §1.1 `clears_base_price_atomically`; the UI mutation seam is not.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<HasSubtypesToggle>` | `toggle_to_subtypes_combines_clear_base_price_with_save_in_single_ipc_call` | Render `<CheckTypeForm>` with `has_subtypes=0`, `base_price_iqd=50000`. User toggles ON; confirm prompt accepts. Assert the dispatched IPC sequence is EXACTLY ONE call: `check_types_update { id, patch: { has_subtypes: true, base_price_iqd: null } }`. NOT two separate calls (clear, then save). A network failure on this single call leaves the form's local state unchanged (no half-state where `has_subtypes=true` but `base_price_iqd` still 50000). Per §4 step 2 atomicity. |

### §11.7 P03-G38 -- CheckTypeForm dye/report 1->0 inverse guard (MEDIUM)

- **Source:** phase-03.md §7.18 + §7.12 `<CheckTypeForm>` dye/report toggles + §7.34 inverse-of-create-time guard.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<CheckTypeForm>` | `toggling_dye_supported_from_1_to_0_blocks_when_dye_only_consumption_map_rows_exist` | Seed check_type with `dye_supported=1` and a consumption_map row `on_dye_only=1` referencing it. Render the form; toggle `dye_supported` to 0. Assert: confirm prompt fires with i18n key `catalog.checkTypes.disable_dye_blocked` listing the dependent consumption rows; save remains disabled until the user clears the dependent rows first; clicking Save with the toggle still 0 dispatches no IPC. Mirror for `report_supported` with `on_report_only=1`. Per §7.18 + §7.34 inverse. |

### §11.8 P03-G39 -- doctors_fts external-content mode storage assertion (LOW)

- **Source:** phase-03.md §1 -- `doctors_fts` declared with `content='doctors', content_rowid='rowid'`.
- **Target test section:** §6.8
- **Category:** Missing Edge Coverage

A migration regression rewriting the FTS table as contentless or non-external-content bloats storage by duplicating rows; behaviour tests don't catch it.

| Scenario | Asserts |
|-|-|
| `doctors_fts_uses_external_content_mode_to_avoid_duplicate_storage` | Apply migration 003; `SELECT sql FROM sqlite_master WHERE name='doctors_fts'`. Assert the returned DDL string contains BOTH `content='doctors'` AND `content_rowid='rowid'` substrings. Insert 100 doctor rows; assert `SELECT SUM(pgsize) FROM dbstat WHERE name='doctors_fts_data'` is bounded (the external-content mode stores only the index, not duplicated row text); compare against a known ceiling derived from the 100-row catalog. Per §1 FTS5 external-content declaration. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 2 Phase-03 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P03-G40 through P03-G41). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P03-G40 -- Server-side role gate on /sync/push catalog mutations (HIGH)

- **Source:** phase-03.md §7.36 + §6.7 -- "Server-side role gate on `/sync/push` mutations -- non-superadmin JWT pushing a `check_types` mutation -> 403. Defence in depth."
- **Target test section:** §2.3
- **Category:** Missing Contract Test

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `non_superadmin_jwt_cannot_push_catalog_mutations_server_side_role_gate` | For each of the 8 catalog entities, authenticate with a JWT bearing `role: 'receptionist'` (or `'accountant'`). Push a valid create/update payload. Assert response 403 with `error.code = 'FORBIDDEN'` (or the documented variant). The catalog row is NOT mutated server-side. Mirror with `role: 'superadmin'` -> 200. The three-layer defence claim (client + IPC + server) needs the SERVER row to be testable. Per §7.36. |

### §12.2 P03-G41 -- DUPLICATE_CONSUMPTION_ROW typed error code (LOW)

- **Source:** phase-03.md §7.21 -- server-side `DUPLICATE_CONSUMPTION_ROW` typed error code (paired with §7.20 `DUPLICATE_PRICING_ROW`).
- **Target test section:** §2.3
- **Category:** Missing Contract Test

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` | `consumption_map_duplicate_push_emits_typed_error_code` | Push two `inventory_consumption_map` rows with identical `(item_id, check_type_id, subtype_id NULL)` paired-unique key. Second push returns 422 with `error.code = 'DUPLICATE_CONSUMPTION_ROW'` (NOT a generic `VALIDATION_ERROR`). The error envelope conforms to §3.2 `AppErrorSchema`. Asymmetric with §7.20's covered `DUPLICATE_PRICING_ROW` row. Per §7.21. |
