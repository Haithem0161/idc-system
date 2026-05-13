# Phase 07: Accounting & Reports -- Test Plan

**Proves:** Accountants and superadmins (per §7.17 + §7.28 role-gates) can navigate the read-only accounting module end-to-end: the Dashboard renders 5 KPIs + 3 trend matrices + 3 top-5 cards (§7.1 + §7.22) against locked visits, the Visits Report supports all 7 group-by modes (§7.14) with per-group totals + the 13-column row mode (§7.3), CSV export honors UTF-8 BOM + CRLF + RFC 4180 (per research.md + §7.7 + §7.25), the Doctor and Operator earnings tables aggregate the per-doctor / per-operator snapshot columns (§7.4 + §7.5) with "House" pseudo-row + hours-on-shift join, drill-downs navigate to filtered visit lists (§7.15 + §7.30) and read-only Visit Detail (phase-05 §7.24), Daily Close runs idempotently with an `input_hash`-keyed PDF (§7.12 + §7.19), the provisional watermark fires when `pendingSync > 0` (§7.20), the per-doctor + per-operator + per-check-type breakdowns render (§7.9 + §7.21), the `[Sign and freeze]` button is hard-gated on `pendingSync === 0` (§7.11), the 90-day boundary correctly routes long-range visits queries to the server while keeping doctor / operator / dashboard reports local-clamped with a `accounting.banner.long_range_local_only` banner (§7.16), the daily-close audit row fires with `action='daily_close_run'` (§7.18 + phase-01 §7.36), and the entire surface stays RTL-clean with Arabic-Indic numerals on every money cell.

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, `<SyncPill>`, audit-action enum -- phase-07 adds `daily_close_run` per §7.18 + phase-01 §7.36), Phase 02 test (auth + `<RequireRole>`, `formatIqd` / `formatIQD` helpers from §7.12 / §7.30 used for every money cell), Phase 03 test (`effective_price` resolver, doctor / operator / check-type / inventory catalog -- reports JOIN to these read models), Phase 04 test (`operator_shifts` -- hours-on-shift aggregate joins these per §7.5), Phase 05 test (`visits` with all 7 name-snapshot columns + all money snapshot columns; `<VisitDetail>` in `mode=readonly` per §7.24; FTS5 patient lookups), Phase 06 test (`inventory_adjustments` -- daily-close inventory consumption value sums `delta` from `consume_visit` reason rows per §4 daily-close step 3).

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_visit_locked_with_doctor, make_visit_locked_house, make_visit_voided, make_shift_with_hours, make_consume_adjustment_for_date_range, make_daily_close_input_hash_fixture}` (extended).
- Factories (TS): `src/test-utils/factories.ts::{makeDashboardKpi, makeVisitsReportFilters, makeVisitsReportRow, makeVisitsReportGroup, makeDoctorEarnings, makeOperatorEarnings, makeDailyCloseArtifact}`.
- Factories (Sync server): `sync-server/test/support/factories.ts::{makeVisitsReportPushSeed, makeDailyCloseRequest}` -- the server only runs reports, doesn't push them.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- the canonical Tuesday: 30 locked visits across 8 doctors + 6 operators + 2 closed shifts. Phase-07 plan loads this fixture for happy-path runs.
- Synthetic fixture: `fixtures/scale/12-months.sql` (NEW for §6.6 + persona P5; ownership: this plan, consumed by `performance-soak.md`) -- 12 months of synthetic visits at PRD §1.3 rate (~100/day average); 25,000+ visits; 200+ doctors; 20+ operators.
- Empty-day fixture: `fixtures/edge/empty-day.sql` (NEW for §6.1 + §6.5) -- zero visits on the target date; tests that Daily Close handles the empty case gracefully.

**Tool prerequisites:**
- Inherited from phase-01..06 execution.
- Rust: `pdfium-render` already installed in phase-05-test for receipts; reused here for Daily Close PDF text-layer comparison.
- Frontend: no new.
- CSV testing: standard string-comparison + byte-comparison; no new tooling.
- None platform-level new.

**Out of scope (cross-cutting tests):**
- Refresh-token replay -- owned by `security.md`.
- 8-hour soak + 12-month scale runs aggregated -- owned by `performance-soak.md`. Phase-07 owns the `fixtures/scale/12-months.sql` source-of-truth but the soak harness runs in phase-08.
- Page-by-page i18n / RTL snapshots for `/accounting/*` -- phase-07 asserts core invariants; the full visual page-by-page sweep is in `i18n-rtl.md`.
- Horizon-1 signed Daily Close entity -- the `input_hash` is computed here (§7.12) but the signed entity itself is deferred. Phase-07 verifies the hash is deterministic and embedded in the PDF footer; the signing flow is not in scope.
- CSV import (bulk import is deferred to Horizon-1 per §7.29). Phase-07 verifies only the export side.

**Cross-phase commands:** none. Phase-07 owns 11 IPC commands (`reports::dashboard_kpis`, `reports::dashboard_tops` from §7.22, `reports::visits`, `reports::doctor_earnings`, `reports::doctor_drilldown`, `reports::operator_earnings`, `reports::operator_drilldown`, `reports::daily_close`, `reports::export_visits_csv`, `reports::export_doctors_csv` from §7.6, `reports::export_operators_csv` from §7.6, `reports::export_daily_close_pdf`).

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`DateRange` value object (`src-tauri/src/domains/reports/domain/value_objects/date_range.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `DateRange::today` | `returns_today_at_baghdad_local_midnight_inclusive_exclusive` | Per §7.8: `from = today_start_baghdad_local`, `to = today_start_baghdad_local + 1 day`. Inclusive-exclusive. |
| `DateRange::yesterday` | -- | -- |
| `DateRange::last_7_days` | -- | -- |
| `DateRange::this_month` / `last_month` | `respects_calendar_month_boundary_at_baghdad_local` | -- |
| `DateRange::custom` | `requires_from_le_to` | `from > to` -> `Err(DateRangeError::InvalidRange)`. |
| `DateRange::crosses_90_day_boundary` | `returns_true_when_from_before_now_minus_90_days` | Per §7.16: used to gate local vs server routing. |
| `DateRange::clamp_to_90_days` | `returns_clamped_with_banner_flag_when_long_range_doctor_or_operator_or_dashboard` | Per §7.16: doctor / operator / dashboard reports clamp; visits report routes to server. |

**`VisitsReportFilters` (`src-tauri/src/domains/reports/domain/value_objects/visits_filters.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `VisitsReportFilters::try_new` | `accepts_all_7_groupby_modes` | Per §7.14: `none | by_date | by_doctor | by_operator | by_check_type | by_subtype | by_status`. |
| `VisitsReportFilters::try_new` | `defaults_groupby_to_none` | -- |
| `VisitsReportFilters::try_new` | `accepts_status_subset` | `statuses = ['locked', 'voided']` is the typical accountant filter. |
| `VisitsReportFilters::try_new` | `accepts_house_included_or_excluded_flag` | Per §4.1 `<VisitsReportTable>`: doctor filter with house included as `(doctor_id IN (...) OR doctor_id IS NULL)`. |

**`ReportsService` aggregation helpers** (pure logic; I/O in §2.1)

| Module | Test | Asserts |
|-|-|-|
| `ReportsService::sum_visit_totals` | `sums_total_amount_snapshot_for_locked_visits_only` | Given a list of 30 visits (22 locked + 8 voided), the sum includes only locked. Per §4 step 3. |
| `ReportsService::sum_visit_totals` | `voided_sum_separate_and_negative_tinted` | Per §7.10: voided value is shown separately; does NOT subtract from total revenue. |
| `ReportsService::sum_visit_totals` | `house_pseudo_row_aggregates_doctor_id_is_null` | Per §4 + §7.4: house row sums all `doctor_id IS NULL` visits. |
| `ReportsService::compute_doctor_per_check_breakdown` | `groups_by_check_type_and_subtype` | Per §7.4 + §7.30: returns rows `(check_type_id, subtype_id, visits_count, revenue, doctor_cut)`. |
| `ReportsService::compute_operator_hours_on_shift` | `sums_check_out_minus_check_in_per_shift_in_range` | Per §7.5: closed shifts in the date range; sums `check_out_at - check_in_at`. Open shifts excluded (no `check_out_at`). |
| `ReportsService::compute_avg_cut_per_hour` | `divides_operator_cut_by_hours_on_shift` | Per §7.5: rounds to whole IQD. |
| `ReportsService::compute_avg_cut_per_visit` | `divides_doctor_cut_by_visits_count` | Per §7.4. Rounds to whole IQD. Zero visits -> 0 (no divide-by-zero). |
| `ReportsService::compute_daily_close_input_hash` | `blake3_of_canonicalized_input_json_deterministic_across_runs` | Per §7.12: hash is stable; recomputing with identical inputs yields the same prefix. |
| `ReportsService::compute_daily_close_input_hash` | `differs_when_a_new_visit_locks_after_first_run` | A new locked visit between two runs changes the hash. Per §7.19 recomputation chip. |
| `ReportsService::compute_per_check_type_breakdown` | `groups_locked_visits_by_check_type_id_and_subtype_id` | Per §7.21: returns `(check_type_id, name_ar, name_en, visits, revenue, doctor_cut, operator_cut)`. |
| `ReportsService::compute_daily_close_pending_sync` | `is_outbox_row_count_at_run_time` | Per §4 daily-close step 5: reads `SELECT COUNT(*) FROM outbox WHERE parked = 0`. |

**`CsvWriter` (`src-tauri/src/domains/reports/service/csv_writer.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `CsvWriter::write_visits` | `produces_utf8_bom_first_3_bytes` | Per §7.7: first 3 bytes are `0xEF 0xBB 0xBF`. |
| `CsvWriter::write_visits` | `uses_crlf_line_endings` | Per §7.7: every row ends with `\r\n`. |
| `CsvWriter::write_visits` | `quotes_fields_containing_comma_or_quote_or_newline_per_rfc4180` | A patient name `"O'Brien, Layla"` is wrapped in double quotes; embedded double quotes are escaped. |
| `CsvWriter::write_visits` | `header_matches_phase_07_7_7_canonical` | Exact match: `Date,Visit #,Patient,Check,Subtype,Doctor,Operator,Dye,Report,Price (IQD),Doctor Cut (IQD),Operator Cut (IQD),Net (IQD)`. |
| `CsvWriter::write_visits` | `sorts_rows_by_locked_at_asc_then_visit_id_asc_deterministic` | Per §7.25. |
| `CsvWriter::write_visits` | `footer_row_is_total_with_aggregate_sums` | Per §7.25: `TOTAL,,,,,,,,,<price>,<doctor>,<operator>,<net>`. |
| `CsvWriter::write_doctor_earnings` | `header_matches_phase_07_7_7_doctors` | `Doctor,Specialty,Visits,Revenue (IQD),Doctor Cut Total (IQD),Avg Cut Per Visit (IQD)`. Per §7.7. |
| `CsvWriter::write_doctor_earnings` | `house_row_is_last_per_7_25` | -- |
| `CsvWriter::write_operator_earnings` | `header_matches_phase_07_7_7_operators_with_hours_on_shift_column` | -- |
| `CsvWriter::filename_convention` | `produces_slug_dates_timestamp_per_7_23` | `visits_<from>_<to>_<HHmmss>.csv`. |

**`DailyCloseGenerator` (`src-tauri/src/domains/reports/service/daily_close_generator.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `DailyCloseGenerator::run` | `produces_artifact_with_all_required_fields_per_7_9_and_7_21` | The struct has: tenant_id, target_date, tz_offset, total_revenue_iqd, total_doctor_cuts_iqd, total_operator_cuts_iqd, total_inventory_consumption_value_iqd, net_iqd, voided_count, voided_value_iqd, per_doctor, per_operator, per_check_type, generated_at, input_hash, provisional. |
| `DailyCloseGenerator::run` | `inventory_consumption_value_sums_consume_visit_delta_negated_for_target_date` | Per §4 step 3: `SELECT SUM(-delta) FROM inventory_adjustments WHERE reason='consume_visit' AND date(created_at)=:date AND deleted_at IS NULL`. |
| `DailyCloseGenerator::run` | `tz_offset_field_is_plus_03_00_year_round` | Per §7.8: Iraq doesn't observe DST. |
| `DailyCloseGenerator::run` | `provisional_true_when_outbox_count_above_zero` | Per §7.20. |
| `DailyCloseGenerator::render_pdf` | `embeds_input_hash_prefix_in_filename` | Per §7.19: `daily-close_<date>_<inputHashPrefix>.pdf`. |
| `DailyCloseGenerator::render_pdf` | `pdf_footer_contains_input_hash_6_char_prefix` | Per §7.12. |
| `DailyCloseGenerator::render_pdf` | `provisional_watermark_present_when_pending_sync_above_zero` | Per §7.20: PDF footer reads `PROVISIONAL — N pending ops` exactly. |

### §1.2 TS pure functions / value objects

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/reports.ts::VisitsReportFiltersSchema` | `accepts_all_7_groupby_modes_and_defaults_to_none` | Per §7.14. |
| `src/lib/schemas/reports.ts::VisitsReportFiltersSchema` | `houseIncluded_default_false` | -- |
| `src/lib/schemas/reports.ts::DailyCloseSchema` | `requires_target_date_iso_format` | -- |
| `src/lib/schemas/reports.ts::DailyCloseSchema` | `per_doctor_per_operator_per_check_type_all_arrays` | Per §7.9 + §7.21. |
| `src/stores/accounting-filters-store.ts` | `persists_locally_not_synced` | Per §3 frontend: per-device store; never crosses sync. |
| `src/stores/accounting-filters-store.ts` | `includeVoided_default_false_per_7_2` | -- |
| `src/features/reports/long-range-clamp.ts::clampForReport` | `clamps_doctor_earnings_to_90_days_with_banner_flag` | Per §7.16: doctor / operator / dashboard clamp. |
| `src/features/reports/long-range-clamp.ts::clampForReport` | `routes_visits_to_server_when_range_exceeds_90_days` | Per §7.16: visits report routes to server. |
| `src/features/reports/long-range-clamp.ts::clampForReport` | `daily_close_falls_back_to_server_when_local_rows_missing_for_target_date` | Per §7.16. |
| `src/features/reports/csv-export-button.ts::buildSavePath` | `prepends_appdata_exports_path_and_extension` | -- |
| `src/features/reports/csv-export-button.ts::buildSavePath` | `filename_pattern_matches_phase_07_7_23` | `<slug>_<from>_<to>_<HHmmss>.<ext>`. |
| `src/features/reports/format-trend-delta.ts::formatDelta` | `renders_arrow_up_or_down_based_on_sign` | `+14% vs Apr` (success tint); `-3% vs Apr` (crimson tint). |
| `src/features/reports/format-trend-delta.ts::formatDelta` | `respects_arabic_digits_when_setting_true` | -- |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/reports/domain/**` (DateRange, VisitsReportFilters, value objects) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::reports::domain` |
| `src-tauri/src/domains/reports/service/**` (aggregation helpers, CsvWriter, DailyCloseGenerator, input_hash) | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::reports::service` |
| `src-tauri/src/domains/reports/infrastructure/**` (read-model SQL builders, PDF renderer wrapper) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::reports::infrastructure` |
| `src/features/reports/**`, `src/features/accounting/**`, `src/lib/schemas/reports.ts`, `src/stores/accounting-filters-store.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/{reports,accounting}/**,src/lib/schemas/reports.ts,src/stores/accounting-filters-store.ts"` |
| `src/pages/accounting/**`, `src/components/accounting/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/accounting/**,src/components/accounting/**"` |
| `sync-server/src/app/domains/reports/service/**` (visits + daily-close aggregates) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/reports/presentation/**` (`/reports/visits`, `/reports/daily-close/:date`) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/reports_phase07.rs` (already exists at HEAD).

**New scenarios in `reports_phase07.rs`:**

| Scenario | Asserts |
|-|-|
| `dashboard_kpis_returns_5_kpis_plus_3_trends_plus_3_tops` | Per §7.1 + §7.22: response includes all required tiles. KPI sums match per-row sums. |
| `dashboard_kpis_respects_include_voided_toggle` | Per §7.2: with `include_voided=true`, voided visits' values appear in their own row but don't subtract from totals. |
| `dashboard_kpis_clamps_to_90_days_with_banner_flag` | Per §7.16: a 365-day range returns `range_clamped_to_90_days: true`. |
| `dashboard_tops_returns_top_5_doctors_by_revenue` | Per §7.22: response has `top_doctors: Vec<DoctorTopRow>` sorted by revenue DESC, limit 5. |
| `dashboard_tops_returns_top_5_operators_by_visits` | -- |
| `dashboard_tops_returns_top_5_check_types_by_revenue` | -- |
| `visits_report_groupby_none_returns_rows_with_13_columns` | Per §7.3: each row has all 13 fields. |
| `visits_report_groupby_by_date_aggregates_per_local_day` | Per §7.14 + §7.8: dates bucket by Baghdad local. |
| `visits_report_groupby_by_doctor_aggregates_with_house_pseudo_row` | Per §7.14: house row uses `doctor_id IS NULL`. |
| `visits_report_groupby_by_operator_aggregates_per_operator_id` | -- |
| `visits_report_groupby_by_check_type_aggregates_with_subtype_collapse` | -- |
| `visits_report_groupby_by_subtype_aggregates_with_parent_check_type_resolution` | -- |
| `visits_report_groupby_by_status_aggregates_locked_voided_separately` | -- |
| `visits_report_uses_snapshot_columns_not_live_joins_for_locked_visits` | Per §4 Tauri step 3: SUM(price_snapshot_iqd), SUM(doctor_cut_snapshot_iqd), SUM(operator_cut_snapshot_iqd). The catalog joins are for display columns only; the aggregates use snapshots. |
| `visits_report_house_included_filter_combines_doctor_id_in_with_null` | Per §4.1 frontend: `(doctor_id IN (...) OR (doctor_id IS NULL AND :house_included))`. |
| `visits_report_filters_combine_all_seven_groupby_modes_with_date_range` | Filter matrix test. |
| `visits_report_returns_voided_with_negative_tinted_marker` | Per §7.10: voided rows in row-mode include a `marker: 'voided'` field. |
| `doctor_earnings_returns_rows_with_avg_cut_per_visit_column` | Per §7.4. |
| `doctor_earnings_house_pseudo_row_present_when_house_visits_exist` | -- |
| `doctor_drilldown_per_check_breakdown_per_7_4_and_7_30` | Per-check breakdown: rows per `(check_type_id, subtype_id)`. |
| `doctor_drilldown_source_visits_excludes_drafts_and_voided` | -- |
| `operator_earnings_hours_on_shift_sums_closed_shifts_in_range` | Per §7.5. |
| `operator_earnings_avg_cut_per_hour_rounds_to_whole_iqd` | Zero-hour case -> 0 (no divide-by-zero). |
| `operator_drilldown_shifts_in_window_columns_per_7_30` | Per §7.30: Date, Check-in, Check-out, Duration, Lines run, Cut earned. |
| `operator_drilldown_attributed_visits_per_7_30` | -- |
| `daily_close_run_produces_artifact_with_input_hash` | Per §7.12. |
| `daily_close_run_audits_with_action_daily_close_run` | Per §7.18: audit row with `action='daily_close_run'`, `entity='daily_close'`, `entity_id=<canonical date>`. |
| `daily_close_run_marks_provisional_true_when_outbox_count_above_zero` | Per §7.20. |
| `daily_close_run_idempotent_when_no_new_visits_between_runs` | Per §7.19: same input_hash. |
| `daily_close_run_emits_recomputed_chip_when_new_visits_between_runs` | Per §7.19: a new locked visit changes the hash; the chip surfaces with the count of new visits. |
| `daily_close_per_doctor_breakdown_per_7_9` | -- |
| `daily_close_per_operator_breakdown_per_7_9` | -- |
| `daily_close_per_check_type_breakdown_per_7_21` | -- |
| `daily_close_voided_value_iqd_sums_total_amount_snapshot_per_7_10` | -- |
| `daily_close_uses_baghdad_local_midnight_boundary_per_7_8` | A visit locked at `23:55 local` on day D is included; one at `00:05 local` on D+1 is NOT. |
| `daily_close_includes_visits_locked_at_exactly_midnight_local_in_the_new_day` | Boundary inclusive of `00:00` and exclusive of `24:00`. |
| `daily_close_on_empty_day_produces_zero_totals_and_zero_count_artifact` | Per §6.5: zero locked visits on target date; artifact has all zeros; provisional false (outbox count zero). |
| `csv_writer_visits_produces_utf8_bom_crlf_rfc4180_sorted_with_total_footer` | -- |
| `csv_writer_doctors_house_row_last` | -- |
| `csv_writer_operators_with_hours_column_decimal_formatted` | -- |
| `cross_90_day_visits_routes_to_server_endpoint` | Per §7.16: Rust side checks the range and calls `ReportsClient::visits_remote` instead of local. |
| `daily_close_falls_back_to_server_when_local_visits_missing_for_target_date` | Per §7.16: device was offline that day; local has 0 visits for D; the IPC falls back to `/reports/daily-close/:date`. |
| `all_reports_ipcs_require_role_accountant_or_superadmin` | Per §7.17: receptionist caller -> `AppError::Forbidden` for every `reports::*` IPC. |
| `visits_report_voided_button_in_visit_drill_requires_superadmin` | Per §7.17: phase-07 forwards to phase-05's void button which is superadmin-only. The accountant detail page hides the button. |

### §2.2 Tauri IPC handler tests

| Command | Happy-path test | Error-path test |
|-|-|-|
| `reports_dashboard_kpis` | `returns_5_kpis_plus_trends` -> | `non_accountant_non_superadmin_returns_forbidden` -- per §7.17 |
| `reports_dashboard_tops` | `returns_top_5_each_per_7_22` -> | (role-gate mirror) |
| `reports_visits` | `returns_rows_or_groups_per_groupby_mode` -> | `range_above_90_days_routes_to_remote_or_clamps_per_groupby_mode` -- per §7.16 |
| `reports_doctor_earnings` | `returns_doctor_aggregates_with_house_row` -> | `clamps_range_to_90_days_with_banner` -- per §7.16 |
| `reports_doctor_drilldown` | `returns_per_check_breakdown_plus_source_visits` -> | -- |
| `reports_operator_earnings` | `returns_operator_aggregates_with_hours` -> | -- |
| `reports_operator_drilldown` | `returns_shifts_plus_attributed_visits` -> | -- |
| `reports_daily_close` | `returns_artifact_with_input_hash_and_provisional_flag` -> | `non_accountant_returns_forbidden` |
| `reports_export_visits_csv` | `writes_csv_to_path_returns_unit` -> | `path_outside_appdata_exports_returns_validation` -- per §5 fs:scope |
| `reports_export_doctors_csv` | -- | -- |
| `reports_export_operators_csv` | -- | -- |
| `reports_export_daily_close_pdf` | `writes_pdf_with_input_hash_in_footer_and_filename` -> | -- |

### §2.3 Sync server route handlers

File: `sync-server/test/reports/reports-phase07.test.ts` (NEW).

| Route | Test | Asserts |
|-|-|-|
| `GET /reports/visits` | `returns_rows_when_groupBy_none` | -- |
| `GET /reports/visits` | `returns_groups_when_groupBy_by_doctor_or_by_operator_or_etc` | Per §7.14 + §7.24 schema. |
| `GET /reports/visits` | `cursor_pagination_stable_across_concurrent_writes` | Per §7.24: cursor opaque base64 of `{ lockedAt, visitId }`. |
| `GET /reports/visits` | `caps_at_limit_10000_per_request` | -- |
| `GET /reports/visits` | `tz_query_param_defaults_to_asia_baghdad` | Per §7.8 + §7.24. |
| `GET /reports/visits` | `requires_accountant_or_superadmin_jwt_role` | Per §7.17: receptionist JWT -> 403. |
| `GET /reports/daily-close/:date` | `returns_authoritative_aggregate_for_date_and_tz` | Per §4 server step 1. |
| `GET /reports/daily-close/:date` | `accepts_tz_query_param_overriding_default_baghdad` | -- |
| `GET /reports/daily-close/:date` | `tenant_scoped_via_jwt_entity_id` | -- |
| `GET /reports/daily-close/:date` | `requires_accountant_or_superadmin_jwt_role` | -- |

### §2.4 React Query mutation / query flows

Mocked IPC; component tests run `describe.each([['ltr'],['rtl']])`.

| Hook | Test | Asserts |
|-|-|-|
| `useDashboardKpis` | `caches_per_range_and_include_voided_filter` | -- |
| `useDashboardTops` | `returns_top_5_per_category` | -- |
| `useVisitsReport` | `routes_to_server_when_range_above_90_days` | Per §7.16. |
| `useVisitsReport` | `passes_groupBy_through_to_ipc_or_server` | -- |
| `useDoctorEarnings` | `clamps_to_90_days_with_banner_when_long_range` | -- |
| `useDoctorDrilldown` | `passes_doctor_id_undefined_for_house` | Per §7.15. |
| `useOperatorEarnings` | `includes_hours_on_shift_column` | -- |
| `useOperatorDrilldown` | -- | -- |
| `useDailyClose` | `marks_provisional_when_outbox_count_above_zero` | -- |
| `useExportVisitsCsv` | `opens_save_dialog_then_dispatches_ipc` | -- |
| `useExportDailyClosePdf` | `embeds_input_hash_in_filename_per_7_19` | -- |

Components covered:
- `<KpiCard>` renders value + delta chip; delta tint per success/crimson semantic colors.
- `<TrendMatrix>` renders 5-row × 5-col grid (KPI, current, prior, delta, % change). Per §7.1.
- `<DateRangePicker>` shows today / yesterday / 7d / month / last month / custom presets per §3.
- `<VisitsReportFilters>` renders all 7 group-by options (per §7.14) + status / check / subtype / doctor / operator / dye / report filters.
- `<VisitsReportTable>` 13-column header per §7.3.
- `<VisitsReportTable>` voided rows render with negative tint per §7.10.
- `<DoctorEarningsTable>` 6 columns per §7.4; house row last.
- `<OperatorEarningsTable>` 6 columns per §7.5 with Hours on Shift column as decimal.
- `<DailyCloseLayout>` side-by-side today / prior with deltas.
- `<DailyCloseLayout>` `[Sign and freeze]` disabled when `pendingSync > 0`. Per §7.11.
- `<DailyCloseLayout>` `[Run close]` always enabled. Per §7.11.
- `<DailyCloseLayout>` provisional banner renders when `provisional=true`. Per §7.20.
- `<DailyCloseLayout>` recomputed chip renders when `input_hash` differs between runs. Per §7.19.
- `<CsvExportButton>` opens `tauri-plugin-dialog` save-as.
- `<DoctorPerCheckBreakdownTable>`, `<DoctorSourceVisitsTable>`, `<OperatorShiftsInWindowTable>`, `<OperatorAttributedVisitsTable>` per §7.30.
- `<AccountingDashboard>` includes status toggle (locked / include voided) per §7.2.
- `<AccountingDashboard>` top-5 cards link to drill-down per §7.22.
- `/accounting/visits/:id` renders `<VisitDetail mode="readonly">` per §7.13 + phase-05 §7.24.
- `<AccountingShell>` wrapped in `<RequireRole roles={['accountant','superadmin']}>`. Per §7.28.

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /reports/visits` (request) | `VisitsReportQuerySchema` (per §7.24) | `fixtures/payloads/reports-visits-query-canonical.json` MUST validate. |
| `GET /reports/visits` (response) | `VisitsReportResponseSchema` (tagged union per §7.14) | Captured live for rows-mode and groups-mode. |
| `GET /reports/daily-close/:date` (response) | `DailyCloseResponseSchema` (per §7.9 + §7.21) | All required fields. |

### §3.2 IPC shape contract

| IPC command | Rust struct | TS schema |
|-|-|-|
| `reports_dashboard_kpis` | `DashboardKpis` | `DashboardKpisSchema` |
| `reports_dashboard_tops` | `DashboardTops { top_doctors, top_operators, top_check_types }` | `DashboardTopsSchema` |
| `reports_visits` | `VisitsReport` (tagged union of rows / groups per §7.14) | `VisitsReportSchema` |
| `reports_doctor_earnings` | `Vec<DoctorEarnings>` | -- |
| `reports_doctor_drilldown` | `DoctorDrilldown` | -- |
| `reports_operator_earnings` | `Vec<OperatorEarnings>` | -- |
| `reports_operator_drilldown` | `OperatorDrilldown` | -- |
| `reports_daily_close` | `DailyClose` (all fields from §1.1 + §7.9 + §7.21) | `DailyCloseSchema` |
| `reports_export_*_csv` | `()` | `z.void()` |
| `reports_export_daily_close_pdf` | `()` | `z.void()` |
| (Error envelope -- fixed) | `AppError` (with new `ReportsError` variants `Forbidden`, `DateRangeInvalid`, `RangeAbove90Days`, `EmptyDay`) | `AppErrorSchema` |

### §3.3 Sync envelope contract

No new sync envelope contributions. Reports are read-only. The audit row `action='daily_close_run'` is the only new outbox addition (per §7.18 + phase-01 §7.36); its push payload uses the existing `audit_log` `additive-only` policy.

- **Snapshot files**:
  - `expected/reports/csv-visits-30-day-canonical.csv.sha256` -- byte-stable hash of the rendered CSV for the seeded clinical-day fixture.
  - `expected/reports/csv-doctors-30-day-canonical.csv.sha256`
  - `expected/reports/csv-operators-30-day-canonical.csv.sha256`
  - `expected/reports/daily-close-tuesday-canonical.pdf.sha256` -- text-layer hash + page-1 bitmap hash at 150dpi. Per phase-05 §10 receipt-snapshot rules.

---

## §4 E2E Tests (Pyramid Layer 4)

Specs live under `e2e/specs/accounting/`.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `dashboard-kpis-and-tops.e2e.ts` | Asma (accountant) | 1) Login + navigate to `/accounting`. 2) Verify 5 KPI tiles + 3 trend matrices + 3 top-5 cards. 3) Toggle "include voided"; verify totals update. | Per §7.1 + §7.2 + §7.22. |
| `visits-report-groupby-and-csv-export.e2e.ts` | Asma | 1) Navigate `/accounting/visits`. 2) Set filter: status=paid, group_by=by_check_type. 3) Verify groups render. 4) Switch to group_by=by_doctor. 5) Click CSV export; save; verify BOM + headers + sort order. | Per §7.14 + §7.7. |
| `daily-close-run-and-pdf-export.e2e.ts` | Asma | 1) Navigate `/accounting/daily-close`. 2) Set date to today. 3) Click `[Run close]`. 4) Verify side-by-side rendering. 5) Click PDF export; verify filename includes `input_hash` prefix. | Per §7.12 + §7.19 + §7.23. |
| `daily-close-provisional-when-outbox-nonzero.e2e.ts` | Asma | 1) Force outbox count > 0 (test-only IPC). 2) Run daily close. 3) Verify provisional banner. 4) Verify `[Sign and freeze]` disabled. | Per §7.11 + §7.20. |
| `daily-close-recomputed-chip-after-new-visits.e2e.ts` | Asma | 1) Run daily close. 2) Lock a new visit (via test-only IPC). 3) Run daily close again. 4) Verify recomputed chip with count "1 new visit". | Per §7.19. |
| `doctor-drilldown-house-pseudo-row.e2e.ts` | Asma | 1) `/accounting/doctors`. 2) Click "House" row. 3) Verify `/accounting/doctors/house` route. 4) Verify per-check breakdown for house visits. | Per §7.15. |
| `operator-drilldown-shifts-and-visits.e2e.ts` | Asma | 1) `/accounting/operators`. 2) Click Kareem. 3) Verify shifts in window + attributed visits. | Per §7.30. |
| `accounting-route-role-guard-for-receptionist.e2e.ts` | Mehdi | Attempt to navigate to `/accounting`. | Redirected to `/no-access`. Per §7.28. |
| `accountant-readonly-visit-detail.e2e.ts` | Asma | Navigate to `/accounting/visits/<locked-id>`. | `<VisitDetail mode="readonly">` per §7.13: no Edit, no Void, no Discard buttons; Print remains. |
| `arabic-numerals-on-money-cells.e2e.ts` | Asma | Toggle `arabic_numerals` setting; verify all money cells render `"١٠٬٠٠٠ د.ع"` instead of `"10,000 د.ع"`. | Per phase-02 §7.12 + §7.30. |
| `breadcrumb-on-doctor-drilldown.e2e.ts` | Asma | Navigate to `/accounting/doctors/:id`. | Breadcrumb shows `resolveLocaleName(doctor)`. Per §7.27. |

### §4.2 Failure-path flows

- **`offline-dashboard-renders-from-local.e2e.ts`** -- Set offline; navigate to `/accounting`; assert KPIs render from local SQLite; no network call attempted (dashboard is local-only per §7.16).
- **`long-range-visits-routes-to-server-with-loading-state.e2e.ts`** -- Set range to 120 days; assert UI shows "Querying server" indicator; assert IPC routes to `/reports/visits`; verify success.
- **`long-range-doctor-clamps-to-90-with-banner.e2e.ts`** -- Set doctor earnings range to 120 days; assert clamping banner renders; assert IPC returns local 90-day data.
- **`daily-close-server-fallback-on-missing-local-rows.e2e.ts`** -- Lock visits on Device B; quit Device A; lock more visits on B; restart A; run daily close for the missing day; assert fallback to server; assert correct totals.
- **`receptionist-tries-reports-ipc-blocked.e2e.ts`** -- Per §7.17: receptionist dispatches `reports::dashboard_kpis` via dev-tools; IPC returns `AppError::Forbidden`.
- **`server-5xx-during-daily-close-fallback.e2e.ts`** -- Server returns 503 during the daily-close server fallback; assert UI surfaces the error with retry button; no half-rendered PDF.
- **`csv-export-path-outside-appdata-rejected.e2e.ts`** -- Per §5 fs:scope: a user-supplied path outside `$APPDATA/idc-system/exports/` returns `AppError::Validation`; no file written.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-daily-close-converges-after-pull.e2e.ts` | Device A locks 5 visits; reconnects. Device B pulls; runs daily close for today. | Device B's daily close matches Device A's totals. |
| `two-device-visits-report-converges.e2e.ts` | Device A locks visits while Device B is offline; both reconnect; both run visits report. | Same totals on both devices. |
| `provisional-watermark-when-other-device-has-unpushed-ops.e2e.ts` | Device A locks 3 visits (in outbox, not yet pushed). Device B (offline) is unaware. Device A runs daily close. | Device A's PDF shows `PROVISIONAL — 3 pending ops`. After A pushes and B pulls, B's daily close shows non-provisional totals reflecting A's 3 visits. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: `/accounting` dashboard in both directions.** Verify 5 KPI tiles in single-pixel-gap grid per `.claude/rules/design-system.md` §5.5; trend matrices render as 5-row × 5-col; top-5 cards mini-tables.
- **Visual: `<DailyCloseLayout>`.** Side-by-side today / prior; deltas tinted per design-system semantic colors; provisional watermark renders correctly.
- **Visual: PDF export.** Open the rendered PDF; verify footer contains `input_hash` 6-char prefix; verify `PROVISIONAL` watermark visible if applicable; verify Arabic-Indic numerals render correctly when setting on.
- **Visual: CSV in spreadsheet.** Open exported CSV in LibreOffice + Excel; verify columns align; verify BOM doesn't show as `ï»¿`; verify totals row matches per-row sums.
- **Keyboard navigation.** Tab through dashboard -> filter controls -> table -> top-5 cards; verify focus rings visible per design-system §3.3.

### §5.2 Cross-references to `personas.md`

- `personas.md` -> **P1 Asma the Accountant** -> steps 1-12 (the entire accountant day). Required for §8 DoD.
- `personas.md` -> **P5 Year-End Audit** -> steps 2-5 (12-month visit aggregate; uses the `fixtures/scale/12-months.sql` fixture). Reinforcement.

**Canonical: P1 Asma the Accountant.**

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **Asia/Baghdad fixed offset.** All aggregates bucket by Baghdad local day. A visit locked at `23:55 +03:00` on D is in D, not D+1. Per §7.8.
- **Daily close boundary.** Per §6 verification step 13: lock at 23:55 D and 00:05 D+1; daily close for D includes only the first. Asserted in §2.1.
- **Clock skew vs server.** Per phase-01: server-authoritative `updated_at`. Reports read snapshot columns from `visits.locked_at` (which is client-canonical; preserved through sync per phase-05 §7.39 invariant).
- **Empty day.** Per §6.5 + §6.1: a target date with zero visits returns a zero-totals artifact, not an error. Per §2.1 `daily_close_on_empty_day_produces_zero_totals_and_zero_count_artifact`.
- **DST defensive.** CI `grep` test forbids `chrono_tz::Tz::Baghdad` in `domains/reports/`.

### §6.2 i18n & RTL

- **en/ar swap on every `/accounting/*` route.** Strings from `accounting.*` namespace.
- **Arabic-Indic numerals on every money cell.** KPIs, table cells, CSV exports (CSV stays ASCII for spreadsheet compat -- the `arabic_numerals` setting does NOT affect exports per research.md), PDF text-layer for Daily Close uses Arabic-Indic per the locale.
- **RTL layout invariants.** KPI tiles flow right-to-left; tables right-align numeric columns to the page edge in RTL.
- **Mixed-direction patient names in CSV.** A patient `"Layla هاشم"` renders byte-stable in the CSV; spreadsheets render with proper bidi.
- **PDF Arabic rendering.** Per phase-05 §6.2 receipt RTL pattern; daily-close PDF in `ar` mirrors layout and uses Arabic-Indic digits.

### §6.3 Offline & Network

- **Dashboard offline.** Per §7.16: dashboard is local-only. The UI renders without network.
- **Visits report local for short ranges.** < 90 days reads from local; the IPC never hits the network.
- **Visits report long-range falls back to server.** ≥ 90 days routes to `/reports/visits`. Per §7.16.
- **Daily close server fallback.** When local lacks visits for the target date (device was offline), automatic fallback. Per §7.16.
- **Server returns 5xx during daily-close fallback.** UI surfaces error; retry button; no partial render.
- **Token expiry mid-report.** Per phase-02 §7.25: one 401 -> refresh + retry once.

### §6.4 Concurrency & Conflicts

- **N/A for read-only surface.** Reports don't push; no conflict policy invocations.
- **2-device daily close.** Per §4.3: both devices converge after pull.
- **Provisional watermark across devices.** Per §4.3: outbox count is per-device; the watermark fires only on the device with unpushed ops.

### §6.5 Crash & Recovery

- **SIGKILL during CSV export.** Aborted write; the temp file is removed; no partial CSV in `$APPDATA/exports`.
- **SIGKILL during PDF render.** Same atomic-rename pattern as receipts (phase-05 §7.16): write to tmp, rename on success. Partial file cleaned on next boot.
- **SIGKILL during daily-close audit row write.** The `with_audit` transaction rolls back; no partial daily close. The next attempt is fresh.
- **Empty day handling.** Per §6.1: zero visits -> zero artifact, not an error. Tested in §2.1.

### §6.6 Scale & Performance

- **12-month aggregate (25k+ visits).** Persona P5 step 2: `/accounting/visits` over 12 months: < 4 s p95. Per §6.1. Driven by `visits_status_date` + `visits_check_type` indexes. Fixture `12-months.sql` owned by this phase.
- **CSV export of 1000 rows.** < 500 ms p95.
- **Daily close on 100-visit day.** < 1 s p95 (per §9 default).
- **Daily close on 12-month range.** N/A -- daily close is per-day; the 12-month aggregate is via visits report.
- **Top-5 cards refresh.** < 200 ms p95.

### §6.7 Security & Permissions

- **Role gate: receptionist tries `reports::*`.** Returns `AppError::Forbidden`. Per §7.17.
- **Route gate: accountant or superadmin only.** `<RequireRole roles={['accountant','superadmin']}>`. Per §7.28.
- **Server-side role gate.** `/reports/visits` and `/reports/daily-close/:date` require accountant or superadmin JWT. Per §7.17.
- **Void button visibility.** Per §7.17: void in `<VisitDetail mode="readonly">` only visible to superadmin (cross-ref phase-05 §7.24).
- **CSV / PDF path scope.** Per §5: `$APPDATA/idc-system/exports/` only.
- **JWT tampering.** Cross-cutting in `security.md`.

### §6.8 Data Integrity

- **Migration 007 idempotent.** No DDL by default; ships as placeholder.
- **Aggregates use snapshot columns for locked visits.** Per §4 + PRD §4.1: locked visits never re-join to live catalog pricing. Asserted in §2.1.
- **House row uses `doctor_id IS NULL`.** Per §7.4.
- **CSV deterministic sort.** Per §7.25: stable across runs.
- **Daily close `input_hash` deterministic.** Per §7.12 + §1.1.
- **`sync_version` monotonicity on the daily-close audit row.** The single `with_audit` call writes one audit row per run; no version bump on a non-existent business row (the audit row is the only mutation).

---

## §7 Performance SLOs (this phase's surfaces)

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `reports::visits` for 30-day range with 200 locked visits | < 100 ms p99 | no (tighter than §9's 200ms because list aggregations should feel snappy) | `perf_visits_report_30_day_200_visits` | -- |
| Tauri (SQLite) | `reports::dashboard_kpis` for today | < 50 ms p99 | no (tighter; dashboard is the landing page) | `perf_dashboard_kpis_today` | -- |
| Tauri (SQLite) | `reports::dashboard_tops` for 30-day range | < 100 ms p99 | no | `perf_dashboard_tops_30_day` | -- |
| Tauri (SQLite) | `reports::doctor_earnings` for 30-day range with 200 visits and 8 doctors | < 200 ms p99 | yes | `perf_doctor_earnings_30_day` | §9 default. |
| Tauri (SQLite) | `reports::operator_earnings` for 30-day range with 200 visits and 6 operators | < 200 ms p99 | yes | `perf_operator_earnings_30_day` | -- |
| Tauri (SQLite) | `reports::daily_close` for typical day (30 visits + 50 adjustments) | < 1 s p95 | yes | `perf_daily_close_typical` | §9 default. |
| Tauri (Reports + I/O) | `reports::export_visits_csv` for 1000 rows | < 500 ms p95 | -- | `perf_export_visits_csv_1000` | -- |
| Tauri (Reports + I/O) | `reports::export_daily_close_pdf` | < 3 s p95 | yes | `perf_export_daily_close_pdf` | §9 default. |
| Sync server (Postgres) | `/reports/visits` 12-month range over 25k+ visits | < 4 s p95 | no (scaled from §9 90-day < 1 s) | `perf_server_visits_12_month` | -- |
| Sync server (Postgres) | `/reports/daily-close/:date` typical day | < 1 s p95 | yes | `perf_server_daily_close_typical` | §9 default. |
| Frontend | `<AccountingDashboard>` cold paint | < 300 ms | -- | `perf_dashboard_cold_paint` | One IPC + render. |
| Frontend | `<VisitsReportTable>` paint with 100 rows | < 200 ms | -- | `perf_visits_table_paint_100` | -- |
| Frontend | `<DailyCloseLayout>` cold paint | < 300 ms | -- | `perf_daily_close_layout_cold_paint` | -- |

---

## §8 Definition of Done

- [ ] All §1 unit tests green.
- [ ] All §2 integration tests green.
- [ ] All §3 contract tests green.
- [ ] All §4 E2E tests green; multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P1 Asma the Accountant** passes.
- [ ] §6 all eight edge categories addressed.
- [ ] §7 SLOs met.
- [ ] Coverage gates met per §1.3.
- [ ] No open P0 or P1 defects.
- [ ] Snapshot files committed:
  - `expected/reports/csv-visits-30-day-canonical.csv.sha256`
  - `expected/reports/csv-doctors-30-day-canonical.csv.sha256`
  - `expected/reports/csv-operators-30-day-canonical.csv.sha256`
  - `expected/reports/daily-close-tuesday-canonical.pdf.sha256` (text-layer + bitmap hashes)
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green.

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P1 Asma the Accountant** | -- | -- | -- | -- |
| P5 Year-End Audit (reinforcement) | -- | -- | -- | Optional, exercises 12-month aggregate against `fixtures/scale/12-months.sql`. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P07-G01 -- daily_close_run audit delta payload (CRITICAL)

- **Source:** phase-07.md §7.18 + phase-01.md §7.36
- **Target test section:** §2.1
- **Category:** Missing Integration Test

The `daily_close_run` audit row is the only mutation a daily-close emits, so its `delta` JSON is the entire forensic trail. The current §2.1 row `daily_close_run_audits_with_action_daily_close_run` only asserts `action`, `entity`, and `entity_id`. It must also assert the exact payload shape per §7.18, or a regression that strips fields will pass.

| Scenario | Asserts |
|-|-|
| `daily_close_run_audit_delta_carries_full_payload_per_7_18` | The `audit_log.delta` JSON of the `daily_close_run` row contains: `input_hash` (full 64-char blake3 hex), `generated_at` (ISO8601 UTC), `total_revenue_iqd`, `locked_count`, `voided_count`, `pending_sync_count`, and `provisional` (bool). Field types and presence checked; values match the artifact returned by the same `reports::daily_close` call. |

### §9.2 P07-G02 -- daily-close PDF filename invariant (CRITICAL)

- **Source:** phase-07.md §7.19 + §7.23
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.19's recomputation chip and §7.23's filename convention together promise that every distinct daily-close artifact lands as a separate file -- the operator can audit every version. The existing `embeds_input_hash_prefix_in_filename` unit test only confirms the pattern; nothing exercises the cross-run uniqueness invariant against the live filesystem.

| Scenario | Asserts |
|-|-|
| `daily_close_pdf_filename_embeds_target_date_and_input_hash_prefix` | Rendered PDF path matches the regex `daily-close_\d{4}-\d{2}-\d{2}_[0-9a-f]{6}\.pdf`; the date component equals the `target_date` argument; the 6-char prefix equals the first 6 chars of the artifact's `input_hash`. |
| `daily_close_pdf_second_run_after_new_lock_produces_distinct_filename_no_overwrite` | Run daily close, snapshot file path, lock a new visit, re-run daily close. Both files exist on disk (no overwrite); filenames differ in the `input_hash` prefix; byte hashes of the two files differ. |

### §9.3 P07-G03 -- server /reports/visits 10000 cap + cursor null (HIGH)

- **Source:** phase-07.md §4 server step 2 + §7.24
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§4 server step 2 caps each `/reports/visits` response at 10000 rows and paginates beyond via cursor. §7.24 specifies `nextCursor: null` when the page is the last. Neither bound is currently tested; a regression that returns 50000 rows or that emits a non-null cursor on the final page would slip through.

| Route | Test | Asserts |
|-|-|-|
| `GET /reports/visits` | `caps_response_rows_at_10000_per_request_and_emits_nextCursor_for_more` | Seed 12000 locked visits in range; request without cursor returns `rows.length == 10000` and `nextCursor != null`; second request with that cursor returns the remaining 2000 with `nextCursor == null`. |
| `GET /reports/visits` | `nextCursor_null_on_final_page_when_total_below_10000` | Seed 200 visits; single request returns all 200 and `nextCursor == null`. |

### §9.4 P07-G04 -- long-range banner i18n key + Authoritative toggle (HIGH)

- **Source:** phase-07.md §7.16
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.16 declares the `accounting.banner.long_range_local_only` i18n key and an explicit "Authoritative" toggle for forcing server-only mode. Without a contract-test snapshot of the banner DOM and the toggle's `aria-pressed` states, a key rename or accidental removal of the toggle is invisible to CI.

| Route | Schema id | Sample payload |
|-|-|-|
| Frontend snapshot: `<LongRangeBanner>` rendered with `range_clamped_to_90_days=true` | `expected/reports/long-range-banner.snapshot.json` | Snapshot pins the i18n key path `accounting.banner.long_range_local_only` (en + ar text resolved from `i18n` namespace), the inline `[Authoritative]` toggle button with `aria-pressed={authoritative}`, and that toggling sets the IPC param `authoritative=true`. |

### §9.5 P07-G05 -- drill-down link query strings (HIGH)

- **Source:** phase-07.md §7.15
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.15 specifies that clicking into the dashboard's top-5 doctor / check-type cards routes to `/accounting/visits?from=...&doctorId=...&checkTypeId=...`, and clicking into a per-shift cell routes to `/reception/shifts?focus=<shift_id>`. The current §2.4 only verifies `<KpiCard>` and `<TrendMatrix>` rendering; nothing pins the query string assembly, so a query-param rename breaks navigation silently.

| Hook | Test | Asserts |
|-|-|-|
| `<DashboardTops>` top-doctor card click handler | `top_doctor_card_navigates_to_visits_with_from_to_doctorId_query_params` | Click on top-doctor row pushes to `/accounting/visits?from=<rangeStart>&to=<rangeEnd>&doctorId=<doctor.id>`; values URL-encoded; no extra keys. |
| `<DashboardTops>` top-check-type card click handler | `top_check_type_card_navigates_to_visits_with_checkTypeId_query_param` | Click on top-check-type row pushes to `/accounting/visits?from=<rangeStart>&to=<rangeEnd>&checkTypeId=<check_type.id>`. |
| `<OperatorEarningsTable>` per-shift cell click handler | `per_shift_cell_navigates_to_reception_shifts_focus_param` | Click on per-shift hours cell pushes to `/reception/shifts?focus=<shift_id>`. |

### §9.6 P07-G06 -- server-side role gate for superadmin (HIGH)

- **Source:** phase-07.md §7.17
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.17 widens the `/reports/*` role gate to accept superadmin in addition to accountant. §2.3 covers receptionist 403 and accountant 200; superadmin 200 is implicit. An E2E that exercises a superadmin JWT against the server endpoints catches drift where the server enum check is narrowed to `accountant` only.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `superadmin-reports-server-endpoints.e2e.ts` | Reza (superadmin) | 1) Login as superadmin. 2) Set range to 120 days (forces server routing). 3) Navigate `/accounting/visits`. 4) Verify rows load from `/reports/visits` (network log). 5) Navigate `/accounting/daily-close` for a date with no local data; run close. | Both server endpoints return 200 for superadmin JWT; no 403; UI renders identically to accountant flow. |

### §9.7 P07-G07 -- void button visibility for superadmin (MEDIUM)

- **Source:** phase-07.md §7.17
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.17 says the void button on `<VisitDetail mode="readonly">` is visible to superadmins, hidden from accountants. §4.1 has `accountant-readonly-visit-detail.e2e.ts` covering the hidden case; the visible case is untested, so a regression that hides the button universally is invisible.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `superadmin-sees-void-button-on-accounting-visit-detail.e2e.ts` | Reza (superadmin) | 1) Login as superadmin. 2) Navigate to `/accounting/visits/<locked-id>`. 3) Observe action buttons. | `[data-testid="visit-void-button"]` is visible and enabled; clicking it opens the void confirmation modal (modal flow itself is owned by phase-05 §7.24, this spec only asserts the entry point exists). |

### §9.8 P07-G08 -- dashboard_tops Top-5 cards SLO row (MEDIUM)

- **Source:** phase-07.md §6.6 / §7.22
- **Target test section:** §7
- **Category:** Missing Performance SLO

§6.6 mentions "Top-5 cards refresh < 200 ms p95" as an edge-coverage line item, but §7's SLO table is the gating CI surface and has no row for it. A typed SLO row makes the threshold a hard pass/fail gate per `.claude/rules/testing.md` §3 / §9.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `reports::dashboard_tops` refresh on filter change (top-doctors + top-operators + top-check-types) | < 200 ms p95 | no (tighter than §9's 200ms list-query default because top-5 is the dashboard's interactive control) | `perf_dashboard_tops_refresh_p95` | §6.6 calls out top-5 refresh as a discrete interactive surface. |

### §9.9 P07-G09 -- voided rows: negative tint, revenue exclusion, inventory inclusion (MEDIUM)

- **Source:** phase-07.md §7.10
- **Target test section:** §6.8
- **Category:** Missing Edge Coverage

§7.10 captures a subtle three-part invariant: voided visits render with a negative tint, do NOT subtract from revenue, but their inventory consumption IS still reflected in inventory-consumption totals (because the consume happened before the void). The plan tests each clause separately; nothing pins the combined invariant on a single fixture row.

- **Voided-visit triple invariant on shared fixture.** Seed one voided visit on the target day with `total_amount_snapshot_iqd = 50000` and one `consume_visit` inventory adjustment with `delta = -3` items worth 12000 IQD. Assert: (a) `<VisitsReportTable>` row carries the `marker: 'voided'` class and renders with negative tint; (b) `dashboard_kpis.total_revenue_iqd` for the day equals revenue from locked visits only (50000 NOT subtracted); (c) `daily_close.total_inventory_consumption_value_iqd` includes the 12000 IQD from the voided visit's consume adjustment (not reversed).

### §9.10 P07-G10 -- Authoritative toggle override (MEDIUM)

- **Source:** phase-07.md §7.16
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.16's "Authoritative" toggle lets the operator force server-mode even when local data exists. Today only the automatic fallback (local missing -> server) is tested in `daily_close_falls_back_to_server_when_local_visits_missing_for_target_date`; the explicit operator override is untested.

| Scenario | Asserts |
|-|-|
| `daily_close_authoritative_toggle_forces_server_path_even_when_local_complete` | Seed a target date with 30 locked visits locally AND on the server. Call `reports::daily_close { target_date, authoritative: true }`. Assert the IPC hits `/reports/daily-close/:date` (network mock recorded one call) and returns the server-side `total_revenue_iqd`, not the local SQLite aggregate. |
| `visits_report_authoritative_toggle_forces_server_path_within_90_days` | Seed a 7-day range with visits locally. Call `reports::visits { from, to, authoritative: true }`. Assert IPC routes to `/reports/visits` despite the short range. |

### §9.11 P07-G11 -- per-check-type breakdown PDF structural hash (LOW)

- **Source:** phase-07.md §7.21
- **Target test section:** §3.3
- **Category:** Missing Snapshot

§7.21's per-check-type breakdown is a distinct PDF section. The current §3.3 snapshot file `daily-close-tuesday-canonical.pdf.sha256` covers text-layer + page-1 bitmap as a whole. A structural hash for the per-check-type section in isolation catches layout regressions to that section without false-positive churn on unrelated layout work.

- **Snapshot file (new):** `expected/reports/daily-close-tuesday-per-check-type-section.sha256` -- structural hash of the per-check-type breakdown subsection only, extracted via the same pdfium text-layer pass used for the page-1 hash. Owned by this phase; regeneration requires written justification per `.claude/rules/testing.md` §16.

### §9.12 P07-G12 -- imports::* reserved-but-unwired (LOW)

- **Source:** phase-07.md §7.29
- **Target test section:** §1.3
- **Category:** Missing Coverage Gate

§7.29 reserves the `imports::*` IPC namespace for Horizon-1 CSV import but says nothing is wired in v1. A negative coverage-gate test prevents accidentally registering a stub in `lib.rs::generate_handler!`, which would expose an unfinished surface.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/lib.rs` | grep returns zero matches for `imports::` inside the `generate_handler!` macro body | `cargo test -p idc-system --test capability_lint -- imports_namespace_not_registered_in_v1` (custom integration lint test under `src-tauri/tests/capability_lint.rs`) |

---

## §10 Gap Analysis Pass 2 Additions

Pass 2 of [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md) re-compared the build spec against the §9 Pass-1 additions and surfaced 14 further gaps (P07-G13 through P07-G26). Each subsection below encodes one gap; the `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s) when the plans are merged for execution.

### §10.1 P07-G13 -- daily_close_run accepted by application-enforced audit-action enum (HIGH)

- **Source:** phase-07.md §7.18 + phase-01.md §7.8
- **Target test section:** §2.1 / §6.8
- **Category:** Missing Integration Test

§7.18 declares that the `daily_close_run` value is "added to the application-enforced audit-action enum (phase-01 §7.8 expanded by reference)" -- meaning the enum is a Rust-side constant the writer validates against before INSERT. Pass-1 §9.1 pins the `delta` payload but not that the action token itself survives the enum gate. If the phase-01 enum expansion is forgotten or reverts, the writer rejects `daily_close_run` before §7.18's audit row is ever produced, and the §9.1 assertion never fires (no row exists to inspect).

| Scenario | Asserts |
|-|-|
| `daily_close_run_action_token_accepted_by_audit_writer_enum_gate` | Call the audit writer directly with `action='daily_close_run'`, `entity='daily_close'`. The writer returns `Ok(())` (not `AuditAction::Unknown` / `Validation`). Inverse: a fabricated `action='daily_close_signed'` returns `AppError::Validation` with the unknown-action variant. Asserts the enum gate is the actual mechanism (not a coincidence that other layers admit the value). Per §7.18 final clause. |

### §10.2 P07-G14 -- dashboard_tops include_voided toggle + role-gate behaviour (HIGH)

- **Source:** phase-07.md §7.22
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§7.22's IPC signature is `reports::dashboard_tops | { range: DateRange, include_voided: bool } | ...`. The §2.2 happy-path row exercises a default invocation but never flips `include_voided`; the error-path row is a placeholder `(role-gate mirror)` that points at a phase-02 helper rather than asserting the gate fires under the Phase-07 command name. A regression that silently ignores `include_voided` (always treating it as `false`) or that drops the `require_role` call would pass both today.

| Scenario | Asserts |
|-|-|
| `dashboard_tops_include_voided_true_changes_top_doctors_and_top_check_types_ordering` | Seed 4 locked visits and 2 voided visits where the voided pair would re-rank the top doctor. Call `reports::dashboard_tops { range, include_voided: false }` -> top_doctor is doctor A. Call with `include_voided: true` -> top_doctor is doctor B (the one with the voided weight added). Inventory of the voided pair is still excluded from revenue per §7.10; only ranking widens. |
| `dashboard_tops_rejects_receptionist_caller_with_forbidden` | Invoke `reports::dashboard_tops` with a receptionist `ctx`; assert `AppError::Forbidden` is returned (NOT a stub success). Mirror test for accountant returns `Ok`; for superadmin returns `Ok`. Per §7.17 + §7.22. |

### §10.3 P07-G15 -- groups-mode response shape against TypeBox schema (HIGH)

- **Source:** phase-07.md §7.14 + §7.24
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§7.14 specifies the tagged-union response `{ mode: 'rows', rows, totals } | { mode: 'groups', groups: [{ key, label, count, revenue, doctor_cut, operator_cut, net }], totals }` and §7.24 mirrors the `groupBy` enum on the server schema, but only the `mode: 'rows'` shape is contract-tested today. A change that emitted `groups[i].group_key` instead of `key`, or that swapped `doctor_cut` for `doctorCut`, would slip past §3.1's Ajv pass because no `mode: 'groups'` sample is run.

| Route | Schema id | Sample payload |
|-|-|-|
| `GET /reports/visits?groupBy=by_doctor` | `VisitsReportResponseSchema` (TypeBox tagged union, `groups` branch) | Seed 12 locked visits across 3 doctors in the target range. Request with `groupBy=by_doctor`. Ajv validates the live response against the schema: `mode === 'groups'`; `groups` is an array of length 3; every element has exactly the keys `{ key, label, count, revenue, doctor_cut, operator_cut, net }`; `totals` aggregates match the row sums; `nextCursor` is null on a single page. Repeat for `groupBy=by_check_type` and `groupBy=by_status` to exercise three distinct enum literals against the same shape. Per §7.14 + §7.24. |

### §10.4 P07-G16 -- daily_close_run audit row server round-trip (HIGH)

- **Source:** phase-07.md §7.18 + §6.4 sync semantics
- **Target test section:** §6.4 / §3.3
- **Category:** Missing Integration Test

§6.4 / §3.3 currently states "reports don't push" -- accurate for the artifact, but the §7.18 `daily_close_run` audit row is an `audit_log` row, and `audit_log` is an additive-only synced entity per phase-01. The row therefore DOES leave the device on the next outbox drain, and the server must accept it under its additive-only policy. No test currently asserts that this specific audit-row shape (with `delta.input_hash`, `delta.provisional`, etc.) survives the round trip into the server's `audit_log` table without truncation or schema rejection.

| Scenario | Asserts |
|-|-|
| `daily_close_run_audit_row_pushes_and_persists_server_side_intact` | Run `reports::daily_close` locally -> one new outbox row of kind `audit_log_insert`. Drain the outbox against the test server. Query the server `audit_log` table for `action='daily_close_run'`; assert one row exists; `delta` JSON deserializes to the same object as the local row (every field from §7.18 + §9.1: `input_hash`, `generated_at`, `total_revenue_iqd`, `locked_count`, `voided_count`, `pending_sync_count`, `provisional`); `entity_id` equals the canonical date string; no field truncated; the sync envelope's `conflict_policy` field is `additive-only`. |

### §10.5 P07-G17 -- Authoritative toggle wires `authoritative=true` into IPC calls (HIGH)

- **Source:** phase-07.md §7.16
- **Target test section:** §2.4
- **Category:** Missing Integration Test

Pass-1 §9.4 contract-snapshots the `<LongRangeBanner>` DOM and the toggle's `aria-pressed` states, but a snapshot pins markup, not behaviour. The toggle's actual job is to forward `authoritative=true` to every reports IPC the page calls (`reports::visits`, `reports::doctor_earnings`, `reports::operator_earnings`, `reports::dashboard_kpis`, `reports::daily_close`). A regression where the toggle visually flips but the IPC param is never threaded through (a common React state-vs-prop bug) is invisible to today's tests.

| Hook / Component | Test | Asserts |
|-|-|-|
| `<AccountingLayout>` reports-toggle wiring (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `authoritative_toggle_forwards_param_to_every_reports_ipc_call` | Render `<AccountingLayout>` with mocked IPC layer. Default state: every `reports::*` call observed by the mock has `authoritative: false` (or absent). Click `[Authoritative]` toggle -> `aria-pressed` flips per §9.4 snapshot. Trigger a refresh on each of the 5 reports IPCs (`visits`, `doctor_earnings`, `operator_earnings`, `dashboard_kpis`, `daily_close`); the mock records exactly 5 calls, each with `authoritative: true`. Toggle off -> next refresh round records `authoritative: false`. Per §7.16. |

### §10.6 P07-G18 -- voided rows rendered under totals on Daily Close PDF (MEDIUM)

- **Source:** phase-07.md §7.10
- **Target test section:** §3.3 / §1.1
- **Category:** Missing Snapshot

§7.10 specifies that voided revenue is shown as "a negative-tinted row below the totals (informational; does NOT subtract from `total_revenue_iqd`)". Pass-1 §9.9 exercises the data-side invariant (revenue not subtracted, inventory included) but not the rendering invariant: that the row is positioned BELOW the totals row in the PDF and rendered with the negative-tint style. A renderer regression that placed the voided row above the totals -- making it look like it was subtracted -- would pass §9.9's data assertions.

| Snapshot file / unit test | Asserts |
|-|-|
| `expected/reports/daily-close-tuesday-voided-row-position.sha256` (NEW structural hash) + `pdf_render_voided_row_below_totals_with_negative_tint` Rust unit test on `DailyCloseGenerator::render_pdf` | Structural hash pins the row order: header -> per-doctor section -> per-operator section -> per-check-type section -> totals row -> voided informational row (in that order). Unit test asserts the voided row's color token resolves to `--crimson` per design-system §1.4 negative-tint convention and that the row carries a `style: 'informational'` attribute (not `style: 'summary'`). Per §7.10 + §7.21 ordering. |

### §10.7 P07-G19 -- doctor/operator CSV footer column-count + house-row position (MEDIUM)

- **Source:** phase-07.md §7.25
- **Target test section:** §1.1
- **Category:** Missing Unit Test

§7.25 says doctor and operator CSV footers must be `TOTAL,...` rows "matching column count", and the doctor CSV must place the `(house)` pseudo-row LAST in body rows (before the footer). Current §1.1 unit tests pin only `csv_writer_visits` footer (13 columns); `csv_writer_doctors` and `csv_writer_operators` have no footer assertion, and no test enforces the house-row-last invariant. A regression that emitted a 5-column footer on the 6-column doctor CSV would import as a malformed row in Excel without any test flagging it.

| Test name | Asserts |
|-|-|
| `csv_writer_doctors_footer_column_count_matches_header_and_house_row_is_last_body_row` | Write a doctor CSV with 3 doctors plus the house pseudo-doctor. Final body row's first column is `(house)`. The TOTAL footer row has exactly 6 comma-separated cells matching the §7.7 doctor header column count. Footer numeric cells equal the sum of body numeric columns. Per §7.7 + §7.25. |
| `csv_writer_operators_footer_column_count_matches_header` | Write an operator CSV with 5 operators. The TOTAL footer row has exactly 6 cells matching the §7.7 operator header column count; footer money cells equal body sums. Per §7.7 + §7.25. |

### §10.8 P07-G20 -- i18n key namespace stability for breakdown table columns (MEDIUM)

- **Source:** phase-07.md §7.26 + §7.30
- **Target test section:** §3.3
- **Category:** Missing Snapshot

§7.30 commits the IDC-system to the i18n key namespaces `accounting.doctors.breakdown.columns.*`, `accounting.operators.shifts.columns.*`, and §7.26 commits to `accounting.actions.export_csv`. These keys are referenced by component code in seven different files; a rename in the locales JSON (or a typo) silently breaks the column headers without breaking the build. Pinning the key paths in a snapshot prevents drift.

| Snapshot file | Asserts |
|-|-|
| `expected/reports/accounting-i18n-key-namespace.snapshot.json` | Snapshot pins the resolved key paths and their en + ar values: `accounting.doctors.breakdown.columns.{check_type, subtype, visits, revenue, doctor_cut, avg_cut}`, `accounting.operators.shifts.columns.{date, check_in, check_out, duration, lines_run, cut_earned}`, `accounting.actions.export_csv`. Generated by walking the locale JSON for those prefixes and serializing the sorted key + en + ar value triples. Any rename, addition, or removal trips the snapshot. Per §7.26 + §7.30. |

### §10.9 P07-G21 -- Print buttons remain visible in readonly visit detail (MEDIUM)

- **Source:** phase-07.md §7.13 + Verification step 11
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

§7.13 + §6 Verification step 11 say "Edit and Void buttons are absent; Print buttons remain". The existing E2E `accountant-readonly-visit-detail.e2e.ts` asserts the absence half (Edit, Void, Discard) but not the presence half. A regression that hid the entire action row in readonly mode would slip through: accountants would lose the print affordance with no test catching it.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `accountant-readonly-visit-detail-print-buttons-retained.e2e.ts` | P1 Asma (`accountant`) | 1) Login as accountant. 2) Navigate to `/accounting/visits/<locked_visit_id>`. 3) Observe the action row of `<VisitDetail mode="readonly">`. | `[data-testid="visit-print-receipt-a5"]` is visible and enabled. `[data-testid="visit-print-receipt-thermal"]` is visible and enabled. Both buttons fire their respective `printing::*` IPCs when clicked (network log records the call). The Edit, Void, and Discard buttons remain absent (regression guard inherited from existing spec). Per §7.13 + §6 step 11. |

### §10.10 P07-G22 -- "Last run at" timestamp refresh on idempotent re-run (MEDIUM)

- **Source:** phase-07.md §7.19
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.19 says re-running daily close updates the on-screen "Last run at" timestamp (and writes a fresh audit row). Pass-1 §9 covers the audit row and the recomputation chip; nothing exercises the timestamp re-render. Because the artifact is idempotent (same `input_hash`), a naive React Query implementation that returns the cached artifact without re-rendering the timestamp would surface the bug.

| Hook | Test | Asserts |
|-|-|-|
| `<DailyCloseLayout>` re-run timestamp behaviour (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `last_run_at_timestamp_refreshes_on_each_run_close_click_even_when_input_hash_unchanged` | Render `<DailyCloseLayout>` with mocked IPC returning the same artifact on every call. Click `[Run close]` -> assert `[data-testid="daily-close-last-run-at"]` renders timestamp T1. Advance fake clock by 90 seconds. Click `[Run close]` again -> the IPC mock records 2 calls; the rendered timestamp is now T1 + 90s (NOT the cached T1); the `input_hash` prefix in the filename hint unchanged (no new artifact); no recomputation chip surfaces. Per §7.19. |

### §10.11 P07-G23 -- `audit_log.delta.provisional` flips with outbox depth (MEDIUM)

- **Source:** phase-07.md §7.20
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.20 explicitly says `audit_log.delta.provisional = true` when `pendingSync > 0`. Pass-1 §9.1 asserts the artifact's `provisional` field but only on the artifact return value; the audit row's `delta.provisional` is a SEPARATE write path. A regression that set the field on the artifact but stripped it from the audit row would pass §9.1.

| Scenario | Asserts |
|-|-|
| `daily_close_run_audit_delta_provisional_reflects_outbox_depth` | Three sub-cases on the same test fixture. (a) Outbox empty -> run daily close -> `audit_log` row's `delta.provisional === false`; artifact `provisional === false`. (b) Seed one pending outbox row -> run daily close -> `audit_log` row's `delta.provisional === true`; artifact `provisional === true`; `delta.pending_sync_count === 1`. (c) Drain outbox -> re-run -> a fresh `audit_log` row written with `delta.provisional === false` and `delta.pending_sync_count === 0`. The two cases never share a row; each run emits its own row per §7.19. Per §7.20. |

### §10.12 P07-G24 -- `ReportsError` variant enumeration contract (MEDIUM)

- **Source:** phase-07.md §3.2 + §7.17
- **Target test section:** §3.2
- **Category:** Missing Contract Test

§3.2's `AppErrorSchema` kinds row in the existing §3.2 contract table names the shared envelope, but the Phase-07-specific `ReportsError` variants (`Forbidden`, `DateRangeInvalid`, `RangeAbove90Days`, `EmptyDay`) need their own enumeration test. The TS Zod definition and the Rust `serde` tags must agree on the four variant names verbatim; otherwise the frontend cannot pattern-match on the error kind to render the right banner (e.g. `RangeAbove90Days` triggers the §7.16 long-range banner).

| Schema | Test | Asserts |
|-|-|-|
| `ReportsErrorSchema` (Zod) vs Rust `ReportsError` enum tags | `reports_error_zod_and_serde_variants_agree_verbatim` | Read the Zod schema's `kind` literal union (`'Forbidden' | 'DateRangeInvalid' | 'RangeAbove90Days' | 'EmptyDay'`); read the Rust enum via the `serde_introspect` macro (or a JSON sample produced by `serde_json::to_value` on each variant); assert set equality on the four tag strings. Inverse: a synthetic Rust `ReportsError::Unknown` variant added in the test breaks the assertion. Per §3.2 + §7.17. |

### §10.13 P07-G25 -- "Sign and freeze" disabled-button tooltip text (LOW)

- **Source:** phase-07.md §7.11
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§7.11 says `[Sign and freeze]` is "disabled with tooltip 'Available in v0.2'". Pass-1 covers the disabled state and the gate condition, but not the tooltip text itself. A locale-file edit that left the key empty -- or a component refactor that swapped Radix `<Tooltip>` for a `title` attribute and broke the i18n binding -- is invisible to today's tests.

| Hook | Test | Asserts |
|-|-|-|
| `<DailyCloseLayout>` sign-and-freeze tooltip (component test, both directions via `describe.each([['ltr'],['rtl']])`) | `sign_and_freeze_disabled_button_renders_tooltip_with_i18n_key_v0_2_copy` | Render `<DailyCloseLayout>` with the button in disabled state. Hover (or focus, since disabled buttons may not receive pointer events) `[data-testid="daily-close-sign-and-freeze"]`. The Radix tooltip surfaces with `role="tooltip"`. Tooltip text resolves from i18n key `accounting.daily_close.sign_and_freeze_tooltip_v0_2` -- en: "Available in v0.2"; ar: localized equivalent. Snapshot the resolved string in both locales. Per §7.11. |

### §10.14 P07-G26 -- Atomic-rename pattern on CSV writes (LOW)

- **Source:** phase-07.md §6.5 crash & recovery + §4 CsvWriter
- **Target test section:** §6.5
- **Category:** Missing Edge Coverage

The existing §6.5 row pins atomic-rename behaviour for the daily-close PDF (write to `.tmp`, rename on success, remove `.tmp` on abort). CSV writes (`CsvWriter::write_visits`, `write_doctors`, `write_operators`) use the same pattern per the implementation note, but no test asserts it. A regression that wrote CSVs directly to the destination path would leak half-written files on a kill mid-write; today nothing catches it.

| Scenario | Asserts |
|-|-|
| `csv_writer_abort_mid_write_removes_tmp_file_and_leaves_destination_untouched` | For each of the three CSV writers (`write_visits`, `write_doctors`, `write_operators`): start a write to `dest_path`; inject a panic between the row stream and the final `rename` call; assert `dest_path` does NOT exist on disk (or, if it pre-existed, its contents are unchanged byte-for-byte); assert no `dest_path.tmp` (or `.partial`, depending on the suffix chosen) file remains in the export directory; assert no permissions leak (tmp file's mode at write time was `0o600` per phase-01 export-scope rule). Three sub-cases per writer. Per §6.5 atomic-rename invariant + §4 CsvWriter contract. |

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 10 Phase-07 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P07-G27 through P07-G36). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P07-G27 -- Authoritative toggle no-server-path for deferred endpoints (HIGH)

- **Source:** phase-07.md §7.16 -- "doctor/operator/dashboard server endpoints DEFERRED to Horizon-1".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§10.5 (Authoritative toggle wiring) implicitly assumed all 5 reports IPCs route to server; §7.16 says doctor/operator/dashboard endpoints are deferred. A regression wiring them up silently would slip through.

| Scenario | Asserts |
|-|-|
| `authoritative_toggle_is_inert_for_deferred_endpoints` | For each of `reports::doctor_earnings`, `reports::operator_earnings`, `reports::dashboard_kpis`, `reports::dashboard_tops`: dispatch the IPC with `authoritative=true`. Spy on `axios.get`; assert ZERO outbound HTTP requests to `/reports/doctors`, `/reports/operators`, `/reports/dashboard/*` paths. The toggle is silently inert for these four endpoints (local result returned regardless). `reports::visits` IS the only IPC where `authoritative=true` triggers a server call. Per §7.16. |

### §11.2 P07-G28 -- UserMenu Accounting link role-gated (HIGH)

- **Source:** phase-07.md §7.28 -- "`<UserMenu>` hides the Accounting link based on the same role check".
- **Target test section:** §4.1
- **Category:** Missing E2E Scenario

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `usermenu-accounting-link-role-hidden.e2e.ts` | Mehdi (`receptionist`) | 1) Log in as Mehdi. 2) Open `<UserMenu>` via the avatar in the top bar. 3) Inspect menu items. | (a) Accounting menu entry is ABSENT (not just disabled). (b) Re-login as P1 Asma (accountant); Accounting entry visible. (c) Re-login as P3 Mariam (superadmin); Accounting entry visible. Per §7.28. |

### §11.3 P07-G29 -- handle.crumb resolution for visit / operator drilldowns (MEDIUM)

- **Source:** phase-07.md §7.27 -- `handle.crumb` on `/accounting/visits/:id` returns `data.patient_name_snapshot || 'visit'`; on `/accounting/operators/:id` returns `resolveLocaleName(data)`.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `breadcrumb_resolves_patient_name_with_visit_fallback` | Navigate to `/accounting/visits/<id>` for a visit with `patient_name_snapshot='Ali Hassan'`. Inspect breadcrumb; assert it reads `Accounting / Visits / Ali Hassan`. For an unsnapshotted patient (`patient_name_snapshot=null` -- legacy data), assert it reads `Accounting / Visits / visit` (the literal fallback string, i18n-resolved). |
| `breadcrumb_resolves_operator_name_per_locale` | Navigate to `/accounting/operators/<id>` for an operator with `name_ar='احمد'`, `name_en='Ahmad'`. In `en` locale, breadcrumb reads `... / Operators / Ahmad`. Switch to `ar`; breadcrumb reads `... / المشغلون / احمد`. Per §7.27 `resolveLocaleName`. |

### §11.4 P07-G30 -- Daily Close per-check-type row carries name_ar AND name_en (MEDIUM)

- **Source:** phase-07.md §7.21 -- per-check-type row `{ name_ar, name_en, count, ... }`.
- **Target test section:** §1.1
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `daily_close_check_type_rows_carry_both_locale_names` | Run daily close for a seeded day with 3 distinct check types. Inspect the resulting `DailyCloseResult.by_check_type` slice: for each row, assert BOTH `name_ar` AND `name_en` are non-empty strings AND match the source check_type's name fields from the active locale-resolution path (not just the active-locale value mirrored). A regression that emitted only one would fail RTL switch + the dual-locale PDF requirement. Per §7.21. |

### §11.5 P07-G31 -- Server emits no audit_log for /reports/daily-close/:date (MEDIUM)

- **Source:** phase-07.md §7.18 -- "no corresponding server write (Daily Close is local-first in v1)".
- **Target test section:** §2.3
- **Category:** Missing Integration Test

| Route | Test | Asserts |
|-|-|-|
| `GET /reports/daily-close/:date` | `server_daily_close_query_writes_no_audit_log_row` | Authenticate as accountant; GET `/reports/daily-close/2026-05-13`. Capture `audit_log` row count BEFORE the request and AFTER. Assert the count is unchanged (server-side daily close is read-only in v1; the audit row lives client-side per §7.18). Per §7.18 server-side absence. |

### §11.6 P07-G32 -- /reports/visits tenant scoping (MEDIUM)

- **Source:** phase-07.md §2.3 -- tenant scoping via `entityId` on every aggregate.
- **Target test section:** §2.3
- **Category:** Missing Integration Test

`tenant_scoped_via_jwt_entity_id` exists for `/reports/daily-close/:date` but NOT for `/reports/visits`.

| Route | Test | Asserts |
|-|-|-|
| `GET /reports/visits` | `visits_route_filters_by_jwt_entity_id_across_all_group_modes` | Seed two tenants A and B, each with 5 visits in the same date range. Authenticate with JWT for tenant A. For each of the 7 `groupBy` modes (none, doctor, operator, check_type, patient, day, hour): GET `/reports/visits?from=...&to=...&groupBy=<mode>`; assert response contains ONLY tenant-A rows; no tenant-B leakage. Per §2.3. |

### §11.7 P07-G33 -- accounting.daily_close.recomputed_n_new_visits i18n snapshot (MEDIUM)

- **Source:** phase-07.md §7.16 + §7.19 -- i18n key `accounting.daily_close.recomputed_n_new_visits`.
- **Target test section:** §3.3
- **Category:** Missing Snapshot

| Snapshot file | Asserts |
|-|-|
| `expected/i18n/accounting.daily_close.recomputed.json.sha256` | Hash of the canonical JSON `{ "en": { "accounting.daily_close.recomputed_n_new_visits": "Daily close recomputed: {count} new visits" }, "ar": { "accounting.daily_close.recomputed_n_new_visits": "..." } }`. Locks both locales' surface strings AND verifies the `{count}` placeholder is preserved verbatim in both. A regression that hardcoded "3 new visits" in en (dropping the `{count}` interpolation) would trip the hash. Per §7.16 + §7.19. |

### §11.8 P07-G34 -- CSV filename slug enumeration (LOW)

- **Source:** phase-07.md §7.23 -- filename slug inventory: `visits`, `doctor-earnings`, `operator-earnings`, `daily-close`.
- **Target test section:** §1.1
- **Category:** Missing Unit Test

| Module | Test | Asserts |
|-|-|-|
| `CsvWriter` / `PdfWriter` | `filename_slugs_enumerate_to_four_documented_slugs` | Parametrize over the 4 declared slugs: invoke each writer with deterministic input (from='2026-05-01', to='2026-05-13', now='10:30:00'). For `visits`, `doctor-earnings`, `operator-earnings`: assert generated filename matches `<slug>_2026-05-01_2026-05-13_103000.csv`. For `daily-close`: assert filename matches `daily-close_2026-05-13_<inputHashPrefix>.pdf`. No slug other than these 4 is reachable from the writer module. Per §7.23. |

### §11.9 P07-G35 -- Drill-down NavLink RTL chevron flip (LOW)

- **Source:** phase-07.md §7.15 -- "Each link uses React Router `<NavLink>` with proper RTL chevron flip".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DrilldownNavLink>` (`describe.each([['ltr'],['rtl']])`) | `chevron_orients_correctly_per_direction` | Render the link with a chevron icon. In LTR, assert the chevron component is `<ChevronRight>` (pointing to the trailing edge). In RTL, assert it is `<ChevronLeft>` (mirrored to point to the trailing edge in RTL). The flip MUST be by component swap, NOT `transform: scaleX(-1)` (which inverts the stroke and breaks the design-system §6 icon rules). Per §7.15. |

### §11.10 P07-G36 -- visits_report dye / report y|n|all three-value filter semantics (LOW)

- **Source:** phase-07.md §4 visits_report step 1 + §7.24 -- `dye` and `report` filters accept `y|n|all`.
- **Target test section:** §1.1
- **Category:** Missing Edge Coverage

| Scenario | Asserts |
|-|-|
| `visits_report_dye_filter_distinguishes_y_n_all_three_values` | Seed 3 visits: dyeYes (`dye_required=1`), dyeNo (`dye_required=0`), dyeMixed (one of each). For each value `v in ['y','n','all']`: dispatch `reports::visits { dye: v, ... }`; assert the returned visit set: `'y'` -> only dyeYes; `'n'` -> only dyeNo; `'all'` -> both dyeYes AND dyeNo. A regression treating `'all'` as `'n'` (defaulting and excluding rows) would surface. Mirror for `report` filter. Per §4 step 1 + §7.24. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 3 Phase-07 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P07-G37 through P07-G39). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P07-G37 -- Top Operators by Visits card drill-down (MEDIUM)

- **Source:** phase-07.md §7.22 + §7.15 -- Top-5 mini-tables link each row to the corresponding drill-down route. Three cards total: doctors, check_types, operators.
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DashboardTops>` (`describe.each([['ltr'],['rtl']])`) | `top_operators_card_row_click_routes_to_operator_drilldown` | Render `<DashboardTops>` with mocked `reports::dashboard_tops` returning three operator rows. Click the first row in the Top Operators card. Assert `router.location.pathname === '/accounting/operators/<operator_id>'` with no extra query string. Mirror for the second and third rows. Closes the closed-set gap (P03-doctor and P03-check-type covered in §9.5; operator card was the residual third). |

### §12.2 P07-G38 -- Cursor opaque base64 of {lockedAt, visitId} (MEDIUM)

- **Source:** phase-07.md §7.24 -- visits cursor is "opaque base64 of `{ lockedAt, visitId }`".
- **Target test section:** §3.1
- **Category:** Missing Integration Test

| Route | Test | Asserts |
|-|-|-|
| `GET /reports/visits` | `visits_cursor_is_opaque_base64_of_lockedAt_and_visitId_only` | Issue a first page request; capture `nextCursor` from the response. Assert `nextCursor` is a base64 string. `Buffer.from(nextCursor, 'base64').toString('utf-8')` decodes to JSON. Parse it; assert the JSON has EXACTLY two top-level keys: `lockedAt` (ISO 8601 string) and `visitId` (UUID). NO other keys (`updatedAt`, `entityId`, etc.). Re-encode the parsed JSON; the round-trip MUST yield the original `nextCursor` bytes (stable encoding). A plaintext `<visitId>` cursor or a cursor with extra fields trips this. Per §7.24. |

### §12.3 P07-G39 -- DailyCloseLayout KPI percentage rendering (LOW)

- **Source:** phase-07.md §4 Frontend `<DailyCloseLayout>` step 2 -- "Render KPIs side by side with deltas AND percentages".
- **Target test section:** §2.4
- **Category:** Missing Integration Test

| Hook / Component | Test | Asserts |
|-|-|-|
| `<DailyCloseLayout>` (`describe.each([['ltr'],['rtl']])`) | `kpi_tiles_render_absolute_delta_and_percentage_in_both_directions` | Seed daily-close result with `today.revenue=120000`, `prior.revenue=100000` (20% increase). For each KPI tile (revenue, visits, doctor_cut, operator_cut, net): assert the tile renders the absolute delta (`+20,000` or `+20`) AND the percentage (`+20%`) as separate sub-elements; raw decimals (`0.2`) MUST NOT appear; the percent sign MUST be present. RTL mirror: `+20%` reads left-to-right per i18n number convention. Per §4 step 2. |
