# Phase 02: Authentication & Users -- Test Plan

**Proves:** A user can log in online (RS256 JWT issued + Argon2id-cached for offline fallback), an offline relaunch succeeds against the stronghold-cached hash, idle-timeout locks the screen and `auth::unlock(password)` resumes the same `UserContext`, refresh tokens rotate atomically (the presented token is revoked in the SAME tx as the new pair is issued, no window where neither is valid), and `auth::change_password` invalidates the local cache + revokes ALL refresh tokens for the user server-side. The Admin Users CRUD + Settings form ship with full role gating (UI hide + IPC `require_role(superadmin)` + server JWT-claim re-check on `/sync/push` for `users` / `settings` mutations). The `users` LWW + `settings` manual conflict policies fire correctly. The fresh-install bootstrap flow (`users::create_first_admin`) seeds the very first superadmin so the rest of the app becomes reachable. Required-key delete protection is enforced at three layers (UI hides, IPC rejects, server `acceptPush` returns `422 SETTINGS_REQUIRED_KEY_IMMUTABLE`). The `users::list` response strips `password_hash` (never leaves the Rust process boundary).

**Surfaces under test:** All (Frontend + Tauri/Rust + Sync Server).
**Dependencies (other test plans):** Phase 01 test (sync plumbing, `with_audit`, outbox, envelope versioning, `<SyncPill>` shell, `<RtlBoundary>`, JWT-key bootstrap stub from §7.10, audit-action enum from §7.8, `errors:sync.*` keys from §7.30, `<RequireRole>` route guard introduced here for use by every later phase).

**Test Data:**
- Factories (Rust): `src-tauri/tests/support/factories.rs::{make_user, make_user_with_role, make_setting, make_user_context, make_argon2id_hash, make_jwt_claims}` (extended -- the support module bootstrapped in phase-01-test).
- Factories (TS): `src/test-utils/factories.ts::{makeUser, makeUserResponse, makeSetting, makeLoginInput, makeChangePasswordInput}`.
- Factories (Sync server): `sync-server/test/support/factories.ts::{makeUserPushPayload, makeSettingPushPayload, makeRefreshToken, makeJwtToken}`.
- Fixture: `docs/idc-system/testing/fixtures/clinical-day.sql` -- contains the seeded v1 required settings + 4 users (1 superadmin, 1 accountant, 2 receptionists). Phase-02 plan consumes the fixture for persona runs; the schema-creation rows for `users` and `settings` are owned here.

**Tool prerequisites:**
- Inherited from phase-01-test execution: `cargo-llvm-cov`, `vitest` + `@testing-library/react` + `jsdom` + `@vitest/coverage-v8`, `webdriverio` + `tauri-driver`, `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser`, `wiremock`, `testcontainers`.
- Rust: `argon2` crate already present from the build cycle; `tauri-plugin-stronghold` (NEW Tauri plugin registration per phase-01 §5 + this phase §3 -- the test rig needs a stronghold mock for offline-login assertions). `jsonwebtoken` (NEW Rust dep, `cargo add jsonwebtoken --features default`).
- Frontend: no new tooling.
- None new platform-level -- inherits the full phase-01-test toolchain.

**Out of scope (cross-cutting tests):**
- Refresh-token replay attacks under adversarial conditions -- owned by `security.md`. Phase-02 verifies the rotation mechanic itself (atomic revoke + issue in one tx); the security-row matrix against tampered tokens, leaked tokens, and cross-tenant replay lives in `security.md`.
- 3xN conflict matrix exhaustively -- the `last-write-wins` cell for `users` and the `manual` cell for `settings` are exercised here; the cross-product against every later entity lives in `sync-conflicts.md`.
- Page-by-page i18n / RTL snapshots for `/admin/*` and `/login` -- phase-02 asserts core invariants per `.claude/rules/design-system.md` §12; the full visual page-by-page sweep is in `i18n-rtl.md`.
- Stronghold key-management lifecycle (rotation, backup, recovery) -- owned by `security.md`. Phase-02 verifies that creds-cache writes/reads work; the key-storage primitives are cross-cutting.

**Cross-phase commands:** none. Phase-02 owns every command it registers (10 auth commands + 6 users commands + 3 settings commands = 19 total; full list in §2.2). The `<RequireRole>` component from §7.8 is consumed by every later phase but ownership remains here.

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services

**`User` entity (`src-tauri/src/domains/auth/domain/entities/user.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `User::try_new` | `produces_user_with_argon2id_hash_uuid_v7_id_lowercase_email` | Defaults; `password_hash` is Argon2id (verified by header `$argon2id$`); `email` is lowercased. |
| `User::try_new` | `rejects_empty_email` | `email = ""` -> `Err(UserError::EmailEmpty)`. |
| `User::try_new` | `rejects_invalid_email_format` | `"not-an-email"` -> `Err(UserError::EmailInvalid)`. The regex matches the Zod side. |
| `User::try_new` | `lowercases_email_with_unicode_safe_normalization` | `"Test@Example.COM"` -> stored as `"test@example.com"`. Per §7.6 + §7.16. |
| `User::try_new` | `rejects_empty_name_after_trim` | `name = "   "` -> `Err(UserError::NameEmpty)`. Per §7.6. |
| `User::try_new` | `rejects_password_shorter_than_8_chars` | `password = "short"` -> `Err(UserError::PasswordTooShort)`. |
| `User::try_new` | `accepts_each_of_3_roles` | `Superadmin`, `Receptionist`, `Accountant` round-trip. Anything else fails to compile. |
| `User::authenticate` | `returns_ok_for_correct_password` | Round-trip via argon2id verify. |
| `User::authenticate` | `returns_err_invalid_for_wrong_password` | `Err(AuthError::Invalid)`. The error variant is distinct from `EmailEmpty` / `EmailInvalid`. |
| `User::authenticate` | `does_not_log_password_or_hash_in_event_stream` | Wrap the call in a `tracing::Span`; capture events; assert no event payload contains the password bytes OR the hash bytes. Per §1.1 RedactionLayer (phase-01 §7.14). |
| `User::deactivate` | `sets_is_active_false_and_preserves_other_fields` | All fields except `is_active`, `updated_at`, `version` unchanged. |
| `User::soft_delete` | `sets_deleted_at_and_is_active_zero_atomically` | Per §7.5: a single `User::soft_delete` invocation produces a new `User` with both flags set, `version` incremented, `dirty=true`. |

**`UserResponse` struct (per §7.20)**

| Module | Test | Asserts |
|-|-|-|
| `UserResponse::from(User)` | `strips_password_hash_field` | The serialized `UserResponse` JSON contains no `password_hash` key. `serde_json::to_value().get("password_hash") == None`. Per §7.20: this is the type-level proof that the hash never crosses the IPC boundary. |
| `UserResponse::from(User)` | `preserves_all_other_fields` | `id`, `email`, `name`, `role`, `is_active`, `last_login_at`, `created_at`, `updated_at`, `entity_id`, `version`, `dirty` all round-trip. |
| `UserResponse` | `compile_time_check_no_password_hash_field` | `trybuild` test: a `let r: UserResponse = ...; r.password_hash` does not compile. The field literally doesn't exist on the type. |

**`Setting` entity (`src-tauri/src/domains/settings/domain/entities/setting.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `Setting::try_new` | `accepts_each_v1_required_key_with_correct_value_type` | `dye_cost_iqd` requires `SettingValue::Int`; `arabic_numerals` requires `Bool`; `currency_symbol` requires `Text`. Mismatched type -> `Err(SettingsError::TypeMismatch)`. |
| `Setting::try_new` | `rejects_unknown_key_in_v1` | `key = "horizon_pet_mode"` -> `Err(SettingsError::UnknownKey)`. The v1 key set is closed at the entity layer; new keys require an explicit code change. |
| `Setting::try_new` | `thermal_width_must_be_32_or_48` | Per §7.1: `key = "thermal_width", value = Int(64)` -> `Err(SettingsError::ThermalWidthInvalid)`. Only 32 or 48 accepted. |
| `Setting::try_new` | `internal_doctor_pct_must_be_0_to_100` | `value = Int(150)` -> `Err`. |
| `Setting::try_new` | `idle_lock_minutes_must_be_positive` | `value = Int(0)` -> `Err`; `Int(-5)` -> `Err`. |
| `Setting::try_new` | `currency_symbol_max_length_8` | `value = Text("very-long-symbol-text")` -> `Err`. |
| `SettingsService::soft_delete` | `rejects_each_of_10_required_keys` | Per §7.2: iterate over all 10 required keys; each soft-delete attempt -> `Err(SettingsError::RequiredKeyImmutable)`. |

**`AuthService` pure helpers (`src-tauri/src/domains/auth/service/auth_service.rs`)** (I/O goes to §2)

| Module | Test | Asserts |
|-|-|-|
| `AuthService::compute_offline_cache_key` | `derives_stronghold_key_from_email_only_not_password` | The stronghold cache key is `creds/<lowercased_email>`; the password is NEVER part of the key (otherwise password changes would orphan caches). |
| `AuthService::compute_session_lock_target` | `returns_lock_when_idle_minutes_exceeded` | Given `last_activity_at = now - 11min` and `settings.idle_lock_minutes = 10`, returns `LockNow`. At exactly 10 min it stays unlocked (boundary inclusive of activity). |
| `AuthService::require_role` | `accepts_listed_role` | `require_role(Superadmin, &[Superadmin])` -> `Ok(())`. |
| `AuthService::require_role` | `rejects_other_role_with_forbidden` | `require_role(Receptionist, &[Superadmin])` -> `Err(AuthError::Forbidden { required: vec![Superadmin] })`. Used by every later phase via the §7.28 IPC role-gate convention. |

**`TokenManager` (`src-tauri/src/domains/auth/service/token_manager.rs`)**

| Module | Test | Asserts |
|-|-|-|
| `TokenManager::classify_401` | `first_401_triggers_refresh_and_retry` | Per §7.25: returns `RefreshAndRetry`. |
| `TokenManager::classify_401` | `second_401_in_a_row_triggers_session_expired_and_pause` | Returns `SessionExpired`. Outbox is NEVER cleared. |
| `TokenManager::classify_401` | `non_consecutive_401_resets_counter` | A 200 between two 401s resets the counter; the second standalone 401 again triggers `RefreshAndRetry`. |

### §1.2 TS pure functions / value objects (Vitest, no IPC, no React)

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/auth.ts::LoginSchema` | `parses_valid_email_and_password_min_8` | Round-trip. |
| `src/lib/schemas/auth.ts::LoginSchema` | `rejects_invalid_email` | `email = "not-email"` -> ZodError on path `["email"]`. |
| `src/lib/schemas/auth.ts::LoginSchema` | `rejects_password_shorter_than_8` | -> error on `["password"]`. |
| `src/lib/schemas/auth.ts::ChangePasswordSchema` | `requires_old_and_new_both_min_8` | -- |
| `src/lib/schemas/user.ts::UserSchema` | `parses_user_response_without_password_hash` | Asserts the schema has no `password_hash` field; a payload that includes it fails strict mode. Mirror of the Rust `UserResponse` invariant. |
| `src/lib/schemas/user.ts::UserCreateSchema` | `requires_email_name_role_password` | All four fields required; role is the 3-value enum. |
| `src/lib/schemas/setting.ts::SettingSchema` | `parses_int_decimal_text_bool_value_types` | Each variant round-trips. |
| `src/lib/schemas/setting.ts::SettingsBundleSchema` | `enforces_known_v1_keys_only` | A bundle with a `horizon_pet_mode` key fails. |
| `src/lib/i18n/first-launch.ts::detectInitialLocale` | `defaults_to_ar_when_store_empty` | Per §7.11. |
| `src/lib/i18n/first-launch.ts::detectInitialLocale` | `respects_stored_locale_on_subsequent_launch` | Pre-seeded `'en'` returns `'en'`. |
| `src/lib/format/numerals.ts::formatIqd` | `renders_eastern_arabic_when_setting_true_locale_ar` | Per §7.12: `formatIqd(1234, { locale: 'ar', arabicDigits: true })` -> `"١٬٢٣٤ د.ع"` (with the Arabic thousands separator and the seeded currency symbol). |
| `src/lib/format/numerals.ts::formatInt` | `respects_locale_thousands_separator` | `formatInt(1234, { locale: 'en' })` -> `"1,234"`; `locale: 'ar'` -> `"١٬٢٣٤"` when arabicDigits=true. |
| `src/lib/format/money.ts::formatIQD` | `appends_currency_symbol_from_settings_param` | Per §7.30: `formatIQD(1234, 'en', 'د.ع')` -> `"1,234 د.ع"`. The helper does NOT hard-code `'د.ع'`. |
| `src/stores/auth-store.ts` | `persists_via_auth_provider_not_plain_localStorage` | Stub `localStorage`; after `setUser({...})`, assert `localStorage.getItem('auth_user')` returns null (the store persists through the Rust IPC path, never via plain web storage). |
| `src/stores/idle-store.ts` | `resets_lastActivityAt_on_each_dispatched_event` | Mousemove resets the timer; keydown resets; click resets. |
| `src/stores/idle-store.ts` | `does_not_reset_when_locked` | Once `locked=true`, activity events do NOT reset `lastActivityAt`. The lock screen ignores them. |
| `src/components/auth/require-role.tsx::shouldAllow` (pure helper) | `accepts_when_current_role_in_allowed_set` | `shouldAllow('superadmin', ['superadmin', 'accountant'])` -> `true`. |
| `src/components/auth/require-role.tsx::shouldAllow` | `rejects_when_current_role_not_in_allowed_set` | `shouldAllow('receptionist', ['superadmin'])` -> `false`. |

### §1.3 Coverage targets

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/auth/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::auth::domain` |
| `src-tauri/src/domains/auth/service/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::auth::service` |
| `src-tauri/src/domains/auth/infrastructure/**` (sqlx UserRepo, stronghold creds cache, jsonwebtoken verifier) | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::auth::infrastructure` |
| `src-tauri/src/domains/settings/domain/**` + `service/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::settings` |
| `src-tauri/src/domains/settings/infrastructure/**` | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::settings::infrastructure` |
| `src/features/auth/**`, `src/features/admin/**`, `src/lib/schemas/{auth,user,setting}.ts`, `src/lib/format/{numerals,money}.ts`, `src/lib/i18n/first-launch.ts`, `src/stores/{auth,idle}-store.ts` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/{auth,admin}/**,src/lib/schemas/{auth,user,setting}.ts,src/lib/format/{numerals,money}.ts,src/lib/i18n/first-launch.ts,src/stores/{auth,idle}-store.ts"` |
| `src/pages/auth/**`, `src/pages/admin/users/**`, `src/pages/admin/settings.tsx`, `src/pages/index/redirect.tsx`, `src/components/auth/**`, `src/components/admin/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/auth/**,src/pages/admin/users/**,src/pages/admin/settings.tsx,src/pages/index/redirect.tsx,src/components/auth/**,src/components/admin/**"` |
| `sync-server/src/app/domains/auth/domain/**` + `service/**` | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/users/domain/**` + `service/**` (the `/sync/push` users-acceptance path with role-gated `password_hash` rules) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/domains/settings/domain/**` + `service/**` (the `/sync/push` settings-acceptance path with manual conflict detection) | >= 90% lines | `pnpm --filter sync-server test:coverage` |
| `sync-server/src/app/auth/presentation/**` (`/auth/login`, `/auth/refresh`, `/auth/logout`, `/auth/change-password`, `/auth/jwks`) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests

- File: `src-tauri/tests/auth_phase02.rs` (NEW).
- Auxiliary file: `src-tauri/tests/users_phase02.rs` (NEW -- the CRUD + soft-delete flow + token-revocation hook deserves its own file).
- Auxiliary file: `src-tauri/tests/settings_phase02.rs` (NEW).

**Scenarios in `auth_phase02.rs`:**

| Scenario | Asserts |
|-|-|
| `login_online_returns_loginresult_mode_online_and_caches_stronghold_hash` | Mock server returns 200 with `{accessToken, refreshToken, user, role, publicKey}`. After login: `AppState.user_context` populated; stronghold key `creds/test@example.com` contains an Argon2id hash; mode = `Online`. Audit row written with `action = 'login'`. |
| `login_offline_fallback_succeeds_when_server_unreachable_but_cache_present` | Disable network (wiremock down); pre-seed `creds/test@example.com` with the Argon2id hash of `"password123"`. Login with correct password -> `LoginResult { mode: Offline }`. `UserContext` populated from local `users` row. No new audit row (audit is online-only for login -- pin this contract). |
| `login_offline_fails_when_password_does_not_match_cache` | Disable network; cache exists; wrong password -> `Err(AuthError::Invalid)`. Stronghold cache NOT cleared. |
| `login_offline_fails_when_no_cache_exists` | Disable network; no cache -> `Err(AuthError::OfflineNoCachedCredentials)`. |
| `login_online_invalid_creds_does_NOT_fall_back_to_offline` | Server returns 401; assert no offline attempt is made; assert `Err(AuthError::Invalid)`. Per §4 step 4. |
| `refresh_rotates_tokens_atomically_in_one_transaction` | Pre-seed a refresh token; call `auth::refresh`; server stub asserts that the SAME tx revokes the presented token AND issues a new one. No window where both are valid simultaneously. Per §4 server step 1-3 + §4.refresh-token-persistence-semantics. |
| `refresh_revoked_token_returns_401` | Call refresh twice with the same presented token; second call -> 401 with `error.code = 'AUTH_INVALID_REFRESH'`. |
| `refresh_expired_token_returns_401_with_distinct_code` | Pre-seed a token with `expires_at = now - 1s`; refresh -> 401 with `error.code = 'AUTH_EXPIRED_REFRESH'`. The two codes are distinct (per phase-09 §3 error-handler reach). |
| `change_password_online_revokes_all_refresh_tokens_for_user` | Pre-seed 3 refresh tokens for the user; call `auth::change_password`; assert server-side `RefreshTokenRepo::revoke_all_for_user(user_id)` was invoked; all 3 tokens are revoked. Per §4 server `changePassword` step 3. |
| `change_password_updates_stronghold_cache_with_new_hash` | Call change_password; assert `creds/<email>` in stronghold now Argon2id-verifies the new password. Old password no longer authenticates offline. |
| `change_password_audits_password_change_action` | Audit row written with `action = 'password_change'`, `entity = 'users'`, `entity_id = user.id`, `delta` containing `[REDACTED]` for both `before` and `after` (per §1.1 RedactionLayer). |
| `lock_preserves_user_context_and_settings_cache` | Per §7.14: call `auth::lock`; assert `AppState.user_context.is_some()` (preserved with `locked: true`); `settings_cache` is untouched. UI route changes to `/lock` but the user is not signed out. |
| `unlock_with_correct_password_clears_locked_flag` | After lock, call `auth::unlock(correct_password)`; verifies against stronghold creds; clears locked flag; user resumes the same session. |
| `unlock_with_wrong_password_returns_invalid_and_keeps_locked` | Wrong password -> `Err(AuthError::Invalid)`; the locked flag stays true; the user cannot proceed. |
| `logout_clears_user_context_and_settings_cache_and_revokes_token` | Per §7.14: `auth::logout` clears both; server-side the refresh token is revoked (single, the current one); outbox is NOT cleared (queued ops preserved). |
| `bootstrap_jwt_key_at_app_start_compares_against_stronghold_pin` | Per §7.10: spawn `lib.rs::setup`; assert `bootstrap_jwt_key()` runs; assert `jwt/publicKey` in stronghold matches the server's `/auth/jwks` response. |
| `bootstrap_jwt_key_refuses_startup_when_pin_mismatched` | Pre-seed a different key in stronghold; assert startup fails with `Err(AuthError::JwtKeyPinMismatch)`. Per §7.10. |
| `bootstrap_jwt_key_one_time_override_via_reset_jwt_pin_flag` | Set CLI flag `--reset-jwt-pin`; assert the pin is replaced; one-time only (the flag is not respected on the next boot). |
| `auth_session_expired_pauses_pushes_but_preserves_outbox` | Per §7.25: force two consecutive 401s; assert `auth:session_expired` event fires; assert engine pauses pushes; assert outbox row count is preserved across the pause. |
| `lock_on_suspend_dispatches_immediately_on_blur` | Per §7.26: simulate `tauri://blur`; after 60s of focus loss, `auth::lock` fires automatically; the user is locked even if the idle timer hasn't reached `idle_lock_minutes`. |
| `idle_lock_uses_settings_idle_lock_minutes` | Pre-seed settings `idle_lock_minutes = 5`; simulate 5min of no activity; assert `auth::lock` fires. Boundary: 4min59s does NOT lock. |
| `argon2id_hash_uses_recommended_params` | Inspect the hash header: `m=19456,t=2,p=1` (or whatever the project pins). Documented in `src-tauri/src/domains/auth/domain/argon2_params.rs` as constants. |
| `redaction_layer_scrubs_password_field_in_login_event_stream` | Per phase-01 §7.14 + §1.1 invariant: emit a `login` event; capture; assert raw password bytes never appear; `password = "[REDACTED]"`. |
| `migration_002_creates_users_and_settings_idempotent` | Run `002_users_settings.sql` twice on fresh DB and on populated DB. Tables, indexes, CHECK constraints match. Per §1 rebuild path: the audit_log FK rebuild only runs if the FK is missing. |
| `migration_002_seeds_v1_required_settings_idempotently` | First run inserts the 10 required keys (including `thermal_width` and `thermal_printer_name` from §7.1). Second run is a no-op (`INSERT OR IGNORE`). |
| `migration_002_rebuilds_audit_log_with_actor_user_id_fk` | After migration, `PRAGMA foreign_key_list(audit_log)` shows the FK from `actor_user_id -> users(id)`. Pre-existing audit rows preserved. Per §1 modified-tables. |
| `users_email_unique_partial_index_blocks_duplicate_active_emails` | INSERT two users with the same email AND `deleted_at IS NULL` -> the second hits `SQLITE_CONSTRAINT_UNIQUE`. INSERT a third with the same email but `deleted_at != null` is allowed. Per §1 `users_email_unique`. |

**Scenarios in `users_phase02.rs`:**

| Scenario | Asserts |
|-|-|
| `users_create_superadmin_gated_returns_forbidden_for_receptionist` | Caller role = receptionist; `users::create(...)` -> `AppError::Forbidden`. Per §4 step 1 + §7.28. |
| `users_create_superadmin_succeeds_and_writes_audit_row` | Superadmin caller; assert user row + audit row `action='create'`. |
| `users_create_normalizes_email_to_lowercase_on_create_path` | Per §7.6: `"Test@Example.COM"` stored as `"test@example.com"`. |
| `users_update_normalizes_email_to_lowercase_on_update_path` | Per §7.6: update email to `"NEW@x.io"` -> stored as `"new@x.io"`. |
| `users_update_email_to_existing_active_user_email_returns_conflict` | Two users; rename one to the other's email -> `Err(UserError::EmailTaken)`. |
| `users_soft_delete_sets_deleted_at_and_is_active_false_atomically` | Per §7.5: both flags set in one tx; audit row `action='soft_delete'`; outbox enqueued. |
| `users_soft_delete_triggers_server_side_token_revocation_on_push_apply` | Server stub asserts `RefreshTokenRepo::revoke_all_for_user(user.id)` invoked when applying the pushed soft-delete. Per §7.5. |
| `users_reset_password_superadmin_only` | Receptionist caller -> `Err(Forbidden)`; superadmin succeeds. Audit `action='password_change'`. |
| `users_reset_password_revokes_all_refresh_tokens_for_target_user_on_server` | Server stub asserts revocation. The target user is signed out across every device. Per §4 step 3 + §7.27. |
| `users_list_response_strips_password_hash_at_ipc_boundary` | Per §7.20: serialize the IPC response; `serde_json::to_value().get("password_hash") == None`. The hash never leaves the Rust process. |
| `users_create_first_admin_bypasses_role_gate_when_zero_users_exist` | Per §7.21: fresh DB; `users::create_first_admin(input)` succeeds without a superadmin caller. Audit row `action='bootstrap_admin'`. |
| `users_create_first_admin_errors_first_admin_exists_when_any_user_row_present` | Pre-seed one user; second call -> `Err(UserError::FirstAdminExists)`. Idempotent: never creates a second bootstrap admin. |
| `users_create_first_admin_auto_logs_in_after_creation` | Per §7.21: on success, the IPC returns a `LoginResult` so the frontend skips the login page. |
| `users_sync_pull_apply_clears_stronghold_cache_when_password_hash_changes_for_cached_user` | Per §7.27: pull a `users` row whose `password_hash` differs from the local row; assert `creds/<email>` is deleted from stronghold. Next offline login fails for that user. |
| `users_sync_pull_apply_does_not_clear_cache_when_other_fields_change` | Pull a row whose `name` changed but `password_hash` is identical; stronghold cache untouched. |

**Scenarios in `settings_phase02.rs`:**

| Scenario | Asserts |
|-|-|
| `settings_update_superadmin_only` | Per §7.3: receptionist + accountant callers -> `Err(Forbidden)`; superadmin succeeds. |
| `settings_update_audits_with_before_after_value_and_value_type` | Audit row's `delta` contains `{ value: { from, to }, value_type: ... }`. Per §4 step 2. |
| `settings_update_emits_settings_changed_event` | Per §7.4: after commit, a `settings:changed` event fires with payload `{ key, old_value, new_value, changed_at }`. Phase-05 `<SettingsChangedBanner>` consumes this. |
| `settings_update_dye_cost_propagates_to_active_drafts_via_event` | Frontend test rig listens for `settings:changed`; after setting `dye_cost_iqd` changes, the active visit-draft store sees the recomputed total candidate. Phase-05 owns the banner; phase-02 owns the event emit. |
| `settings_soft_delete_rejects_all_10_required_keys` | Per §7.2: iterate over all 10 keys; each delete attempt -> `Err(SettingsError::RequiredKeyImmutable)`. |
| `settings_update_thermal_width_rejects_invalid_value` | `value = 64` -> `Err(SettingsError::ThermalWidthInvalid)`. Per §7.1. |
| `settings_manual_conflict_policy_parks_when_concurrent_edits` | Per §7.19: simulate two devices' pushes for the same `settings.key`; server returns 409 `CONFLICT_PARKED`; outbox row's `parked` flag flips to 1. |
| `settings_sync_pull_rejects_required_key_with_deleted_at_not_null` | Per §7.33: pull a `settings` row for `dye_cost_iqd` with `deleted_at != null`; client drops the row; logs WARN; toast `errors:settings.required_key`. The required key stays alive. |
| `settings_key_unique_partial_index_blocks_duplicate_active_keys` | Two settings rows with same `(entity_id, key)` and `deleted_at IS NULL` -> second insert hits the unique constraint. |

### §2.2 Tauri IPC handler tests

One test per command. Happy + at least one error path.

| Command | Happy-path test | Error-path test |
|-|-|-|
| `auth_login` | `login_online_returns_loginresult_with_serialized_user_response` -> assert `password_hash` field absent in the IPC response JSON. | `login_invalid_creds_returns_typed_app_error` -> `AppError::Auth(AuthError::Invalid)`. |
| `auth_refresh` | `refresh_returns_new_access_and_refresh_token_pair` -> server stub returns 200; assert response has both fields. | `refresh_rejects_revoked_token_with_app_error` -> `AppError::Auth(AuthError::InvalidRefresh)`. |
| `auth_logout` | `logout_clears_user_context_and_returns_unit` -> | (no error path; idempotent) |
| `auth_change_password` | `change_password_online_succeeds_and_updates_cache` -> | `change_password_offline_returns_typed_error` -> `AppError::Auth(AuthError::OfflineNotAllowed)`. |
| `auth_current_user` | `current_user_returns_userresponse_when_signed_in` -> assert `UserResponse` shape. | `current_user_returns_none_when_signed_out` -> `None` (`Option::None`). |
| `auth_lock` | `lock_sets_locked_true_and_emits_event` -> | (no error path) |
| `auth_unlock` | `unlock_succeeds_with_correct_password` -> | `unlock_rejects_wrong_password_via_typed_error` -> `AppError::Auth(AuthError::Invalid)`. |
| `users_list` | `list_returns_array_of_user_responses_excluding_password_hash` -> | `list_returns_not_authenticated_when_no_session` -> `AppError::NotAuthenticated`. |
| `users_get` | `get_returns_userresponse_by_id` -> | `get_returns_not_found_for_unknown_id` -> `AppError::NotFound`. |
| `users_create` | `create_returns_persisted_userresponse_with_audit` -> | `create_rejects_non_superadmin_via_forbidden` -> `AppError::Forbidden`. |
| `users_update` | `update_returns_userresponse_with_updated_fields` -> | `update_rejects_non_superadmin_via_forbidden` -> mirror. |
| `users_soft_delete` | `soft_delete_returns_unit_and_marks_row_deleted` -> | `soft_delete_returns_not_found_for_unknown_id` -> `AppError::NotFound`. |
| `users_reset_password` | `reset_password_returns_unit_and_writes_audit` -> | `reset_password_rejects_non_superadmin` -> `AppError::Forbidden`. |
| `users_create_first_admin` | `create_first_admin_returns_loginresult_when_zero_users_exist` -> | `create_first_admin_returns_first_admin_exists_when_any_user_present` -> per §7.21. |
| `settings_list` | `list_returns_all_seeded_settings` -> assert 10 required keys. | (read-only; no error path) |
| `settings_get` | `get_returns_setting_for_known_key` -> | `get_returns_none_for_unknown_key_in_v1` -> `None`. |
| `settings_update` | `update_returns_updated_setting_and_emits_settings_changed_event` -> | `update_rejects_non_superadmin_via_forbidden` -> per §7.3. |

All IPC tests construct `AppState` directly. Each test asserts the serialized error shape, not the Rust enum -- the frontend only sees the JSON.

### §2.3 Sync server route handlers

File: `sync-server/test/auth/auth-phase02.test.ts` (NEW) + `sync-server/test/sync/users-and-settings-phase02.test.ts` (NEW).

DB: real Prisma test DB via `testcontainers`; per-test teardown.

| Route | Test | Asserts |
|-|-|-|
| `POST /auth/login` | `login_returns_jwt_and_refresh_for_valid_creds` | 200 + `{ accessToken, refreshToken, user, role, publicKey }`. JWT has claims `{ sub, role, entityId, deviceId, iat, exp }`; `exp - iat == 900` (15 min). |
| `POST /auth/login` | `login_rejects_invalid_password_with_401` | 401 + `error.code = 'AUTH_INVALID'`. |
| `POST /auth/login` | `login_rejects_unknown_email_with_401_not_404` | Same 401 -- never reveal whether the email exists (timing-safe). |
| `POST /auth/login` | `login_persists_refresh_token_hash_not_plaintext` | The persisted `RefreshToken.tokenHash` is sha256 of the plaintext; the plaintext does NOT appear in the table. |
| `POST /auth/refresh` | `refresh_rotates_atomically_revoking_old_token` | Pre-seed a refresh; call refresh; assert in one tx: old row's `revokedAt` is set AND new row inserted. Tested via prisma `$queryRaw` against the WAL log. |
| `POST /auth/refresh` | `refresh_rejects_revoked_token` | Replay -> 401. |
| `POST /auth/refresh` | `refresh_rejects_expired_token` | Pre-seed with `expiresAt = now - 1s` -> 401 with `AUTH_EXPIRED_REFRESH`. |
| `POST /auth/logout` | `logout_revokes_presented_refresh_token` | After logout, the token cannot be refreshed (401). |
| `POST /auth/change-password` | `change_password_revokes_all_refresh_tokens_for_user` | Pre-seed 3 tokens; call change-password; assert all 3 have `revokedAt` set. Per §4 step 3. |
| `POST /auth/change-password` | `change_password_rejects_wrong_old_password` | 400 + `error.code = 'AUTH_INVALID'`. |
| `GET /auth/jwks` | `jwks_returns_public_key_in_jwk_format` | Per §7.10: the bootstrap path reads this. Pure read; cacheable. |
| `POST /sync/push` (users) | `push_users_create_requires_superadmin_role_claim` | Per §7.24: a receptionist JWT pushing a `users` row with action `create` -> 403 with `FORBIDDEN`. |
| `POST /sync/push` (users) | `push_users_create_accepts_password_hash_field_when_actor_is_superadmin` | Per §7.24 variant: payload INCLUDES `password_hash`; server accepts only when JWT.role = superadmin. |
| `POST /sync/push` (users) | `push_users_update_rejects_password_hash_field` | Per §7.24: profile-edit payload MUST NOT include `password_hash`. TypeBox `Type.Never` rejects it. |
| `POST /sync/push` (users) | `push_users_lww_tiebreak_by_origin_device_id_lex` | Two updates with identical `updatedAt`, different `originDeviceId` -> lex-smaller wins. Per §7.23. |
| `POST /sync/push` (users) | `push_users_soft_delete_triggers_revoke_all_refresh_tokens_for_user_on_server` | Per §7.5: on apply, all refresh tokens for the soft-deleted user are revoked. |
| `POST /sync/push` (users) | `push_users_pull_response_excludes_password_hash` | Pull returns the row but the JSON omits the hash. |
| `POST /sync/push` (settings) | `push_settings_first_write_for_key_succeeds` | New `settings.dye_cost_iqd` row -> 200 applied. |
| `POST /sync/push` (settings) | `push_settings_concurrent_version_diverge_parks_409` | Per §7.19: existing row has `version=5`; incoming has `version=5` with different `updatedAt` -> 409 + `ConflictParked` row inserted. |
| `POST /sync/push` (settings) | `push_settings_with_deleted_at_for_required_key_rejected_422` | Per §7.33: payload `{ key: 'dye_cost_iqd', deletedAt: <ts> }` -> 422 `SETTINGS_REQUIRED_KEY_IMMUTABLE`. |
| `POST /sync/push` (settings) | `push_settings_requires_superadmin_role_claim` | Per §7.28: non-superadmin -> 403. |
| `POST /sync/push` (settings) | `push_settings_value_type_int_with_text_value_rejected_422` | Type mismatch -> 422 with `details.field='value'`. |
| `GET /sync/pull` (users) | `pull_users_returns_rows_without_password_hash` | Per §7.24: the response payload validates against `UserPullSchema` which has `Type.Never` for `password_hash`. |
| `GET /sync/pull` (users + settings) | `pull_excludes_other_tenants_users_and_settings` | Tenant guard via JWT `entityId`. |
| `GET /sync/pull` (users + settings) | `pull_sets_pulled_at_per_phase_02_section_7_17` | `pulledAt` populated after each successful batch. |

### §2.4 React Query mutation / query flows

`src/features/auth/__tests__/queries.test.tsx` + `src/features/admin/users/__tests__/queries.test.tsx` + `src/features/admin/settings/__tests__/queries.test.tsx`. Mocked IPC.

RTL invariant: every component / hook test that renders DOM runs in both `dir=ltr` AND `dir=rtl`. `describe.each([['ltr'],['rtl']])`.

| Hook | Test | Asserts |
|-|-|-|
| `useCurrentUser` | `returns_inmemory_user_not_network_backed` | No IPC call beyond `auth::current_user`; the data lives in `useAuthStore`. |
| `useLogin` (mutation) | `dispatches_auth_login_and_routes_to_role_default` | Per §4 frontend: receptionist -> `/reception`; accountant -> `/accounting`; superadmin -> `/admin/users` (per §7.7). |
| `useLogin` | `routes_to_no_access_when_role_unknown` | -- |
| `useLogin` | `shows_offline_indicator_when_mode_offline` | -- |
| `useUsersList` | `caches_under_users_list_key_and_returns_userresponse_shape` | Assert no `password_hash` field in response. |
| `useUserCreate` | `invalidates_users_list_key_after_create` | -- |
| `useUserSoftDelete` | `shows_user_delete_confirm_modal_before_dispatch` | Per §7.15: the modal renders before the mutation fires. |
| `useUserResetPassword` | `surfaces_reset_password_modal_and_dispatches_on_submit` | Per §7.31: `<ResetPasswordModal>` opens; submit dispatches `users::reset_password`. |
| `useSettings` | `returns_settings_bundle_with_typed_values_per_value_type` | `dye_cost_iqd` -> `Int`; `arabic_numerals` -> `Bool`. |
| `useSettingUpdate` | `invalidates_settings_all_key_and_emits_pricing_recompute_to_active_drafts` | -- |

Components covered separately (each runs `describe.each([['ltr'],['rtl']])`):
- `<LoginForm>` validates via `LoginSchema`; submits to `auth::login`; shows "Offline session" indicator on `mode: 'offline'`.
- `<NoAccessPage>` renders the "Contact your administrator" message in both locales.
- `<LockScreen>` traps focus; Escape does NOT close; only correct password unlocks.
- `<IdleWatcher>` resets timer on mousemove/keydown/click/touchstart.
- `<UsersListRowActions>` icon-button row (edit, reset-password, soft-delete) gated by `<RequireRole roles={['superadmin']}>`.
- `<UserDeleteConfirm>` localized strings; confirm dispatches `users::soft_delete`.
- `<ResetPasswordModal>` Zod min-8 input; confirm dispatches mutation; renders toast "Password reset; user must sign in again on every device."
- `<SettingsForm>` per-key widget bindings (per §7.22): number inputs for cost / pct / minutes; switch for `arabic_numerals`; select for `thermal_width`; combobox for `thermal_printer_name`; text for currency / clinic-display-name.
- `<RootRedirect>` (`/`) reads `useCurrentUser()` and `<Navigate replace>` to the role-default route. Per §7.7.
- `<RequireRole>` renders children when role matches; otherwise `<Navigate replace to="/no-access" />`. Per §7.8.
- `<UserMenu>` Last-synced timestamp renders via `useSyncStatus().lastPulledAt`. Red dot when `last_pushed_at > 5min` and outbox non-empty. Per §7.13.
- `<FirstLaunchSetupModal>` for the very first admin creation (per §7.21 + phase-01 §7.22 cross-coupling).

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /auth/login` (request) | `LoginBodySchema` | `fixtures/payloads/login-body-canonical.json`. MUST validate. |
| `POST /auth/login` (response) | `LoginResponseSchema` | Captured live. Validates `accessToken`, `refreshToken`, `user` (UserResponse shape -- no password_hash), `role` enum, `publicKey`. |
| `POST /auth/refresh` (request) | `RefreshBodySchema` | `{ refreshToken }`. |
| `POST /auth/refresh` (response) | `RefreshResponseSchema` | New pair. |
| `POST /auth/logout` (request) | `RefreshBodySchema` | -- |
| `POST /auth/change-password` (request) | `ChangePasswordBodySchema` | `{ oldPassword, newPassword }`. |
| `GET /auth/jwks` (response) | `JwksResponseSchema` | RFC 7517 JWK Set format. |
| `POST /sync/push` (users request) | `UserCreatePushSchema` (password_hash REQUIRED) + `UserUpdatePushSchema` (password_hash FORBIDDEN via `Type.Never`) per §7.24 | `fixtures/payloads/user-create-push.json`, `user-update-push.json`. Each MUST validate. |
| `POST /sync/push` (users request, negative) | `UserUpdatePushSchema` | `fixtures/payloads/user-update-push-with-password-hash.json` MUST fail Ajv (Type.Never on password_hash). |
| `POST /sync/push` (settings request) | `SettingPushSchema` | `fixtures/payloads/setting-push-int-dye-cost.json`, `setting-push-bool-arabic-numerals.json`, `setting-push-text-currency-symbol.json`. Each MUST validate. |
| `POST /sync/push` (settings request, negative) | `SettingPushSchema` | `setting-push-required-key-deleted.json` MUST fail with `SETTINGS_REQUIRED_KEY_IMMUTABLE`. |
| `GET /sync/pull` (users response) | `UserPullSchema` (password_hash via `Type.Never`) | Captured live. The pulled row's JSON has NO password_hash key. |

### §3.2 IPC shape contract

| IPC command | Rust struct | TS schema |
|-|-|-|
| `auth_login` | `LoginResult { user: UserResponse, role, mode }` | `LoginResultSchema = z.object({ user: UserResponseSchema, role: RoleEnum, mode: z.enum(['online','offline']) })` |
| `auth_refresh` | `TokenPair` | `TokenPairSchema = z.object({ accessToken: z.string(), refreshToken: z.string() })` |
| `auth_logout` | `()` | `z.void()` |
| `auth_change_password` | `()` | `z.void()` |
| `auth_current_user` | `Option<UserResponse>` | `UserResponseSchema.nullable()` |
| `auth_lock` | `()` | `z.void()` |
| `auth_unlock` | `()` | `z.void()` |
| `users_list` | `Vec<UserResponse>` | `z.array(UserResponseSchema)` -- never includes `password_hash`. |
| `users_get` | `UserResponse` | `UserResponseSchema` |
| `users_create` | `UserResponse` | `UserResponseSchema` |
| `users_update` | `UserResponse` | `UserResponseSchema` |
| `users_soft_delete` | `()` | `z.void()` |
| `users_reset_password` | `()` | `z.void()` |
| `users_create_first_admin` | `LoginResult` | `LoginResultSchema` |
| `settings_list` | `Vec<Setting>` | `z.array(SettingSchema)` |
| `settings_get` | `Option<Setting>` | `SettingSchema.nullable()` |
| `settings_update` | `Setting` | `SettingSchema` |
| (Error envelope -- fixed) | `AppError` (with new variants `AuthError`, `UserError`, `SettingsError`) | `AppErrorSchema` -- shared schema. New `kind` values: `Auth`, `Forbidden`, `Validation`, `Conflict`. |

### §3.3 Sync envelope contract

- **Push payload conforms.** Per §7.24 conditional inclusion: `users` push payloads either INCLUDE `password_hash` (create / reset_password) or EXCLUDE it (update). Two distinct TypeBox schemas; Rust mirrors via two distinct struct types.
- **Pull payload conforms.** Server's `UserPullSchema` JSON output omits `password_hash`; the client's mirrored Zod schema rejects payloads that include it.
- **Conflict-resolution policy registry agrees.** Per §4 Sync Semantics: `('users', 'last-write-wins')` with `originDeviceId` lex tiebreak per §7.23; `('settings', 'manual')` with conflict-parked unconditionally on version divergence per §7.19.
- **Versioned envelope.** `envelope_version: 1` for all push payloads. Stub `999` rejected.
- **Snapshot files**:
  - `expected/sync/user-create-push-canonical.json.sha256` (includes password_hash branch)
  - `expected/sync/user-update-push-canonical.json.sha256` (excludes password_hash branch)
  - `expected/sync/user-pull-row-canonical.json.sha256` (server response, no password_hash)
  - `expected/sync/setting-push-canonical.json.sha256`
  - `expected/sync/setting-pull-row-canonical.json.sha256`

---

## §4 E2E Tests (Pyramid Layer 4)

Specs live under `e2e/specs/auth/` and `e2e/specs/admin/`. Every selector is `data-testid`.

### §4.1 Happy-path flows

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `first-launch-bootstrap-and-login.e2e.ts` | Fresh install | 1) Boot. 2) Assert redirect to `/setup/first-run` (per §7.21). 3) Enter superadmin email + name + password. 4) Submit. 5) Assert auto-login + redirect to `/admin/users`. | One user in DB; one audit row `action='bootstrap_admin'`. |
| `login-online-then-offline-relaunch.e2e.ts` | Pre-bootstrapped clinic | 1) Login online with superadmin creds. 2) Verify `/admin/users` reachable. 3) Quit. 4) Disable network. 5) Relaunch. 6) Login with same creds. | `mode: offline` indicator visible near `<UserMenu>`. App is fully usable. |
| `change-password-online-and-relogin-offline.e2e.ts` | Logged-in superadmin | 1) Open `<ChangePasswordModal>`; submit old + new. 2) Verify success. 3) Quit. 4) Disable network. 5) Login with NEW password offline. | Login succeeds; stronghold cache updated. Old password is REJECTED offline. |
| `idle-lock-and-unlock.e2e.ts` | Logged-in user | 1) Set `settings.idle_lock_minutes = 1` (test override). 2) Wait 61s with no activity. 3) Assert `/lock` page renders. 4) Enter password. 5) Submit. | User resumes the previous route. `UserContext` preserved. |
| `users-crud-as-superadmin.e2e.ts` | Mariam (superadmin) | 1) Navigate to `/admin/users`. 2) Create a new receptionist. 3) Edit name. 4) Reset password. 5) Soft-delete. | Each step writes one audit row. Soft-deleted user disappears from default list. |
| `settings-update-superadmin-only.e2e.ts` | Mariam | 1) `/admin/settings`. 2) Change `dye_cost_iqd` from 10000 to 12000. 3) Save. | Audit row written with before/after delta. `settings:changed` event fires. |
| `no-access-route-for-unknown-role.e2e.ts` | Synthetic user with `role: null` | Login -> redirected to `/no-access`. | `<RequireRole>` does not even attempt to render `/admin/*`. |
| `language-toggle-persists-across-restart.e2e.ts` | Any | 1) Switch to `ar`. 2) Quit. 3) Relaunch. | App opens in `ar` (locale persisted). |
| `arabic_numerals_setting_renders_eastern_indic_digits.e2e.ts` | Mariam | 1) Toggle `arabic_numerals` setting on. 2) Navigate to `/admin/settings`. 3) Verify `dye_cost_iqd` value displays `"١٠٬٠٠٠ د.ع"`. | Per §7.12 + §7.30 formatter helpers. |

### §4.2 Failure-path flows

- **`login-online-invalid-creds-does-not-fall-back-offline.e2e.ts`** -- Server reachable; submit wrong password; assert `AuthError::Invalid` toast; assert no offline-mode indicator appears.
- **`refresh-rotation-revokes-old-token.e2e.ts`** -- Use a test-only IPC to retrieve the current refresh token; force a refresh; verify second use of the old token returns 401 from the server.
- **`session-expired-after-second-401-pauses-pushes-preserves-outbox.e2e.ts`** -- Force two consecutive 401s; assert `<SyncPill>` shows `error`; outbox count preserved; route to `/login` (per §7.25).
- **`required-key-soft-delete-rejected-at-three-layers.e2e.ts`** -- Per §7.2 + §7.33: (a) UI does not expose a delete-required-setting button. (b) If raw IPC is fired, returns `Err(SettingsError::RequiredKeyImmutable)`. (c) If a peer device pushes such a row, server rejects 422.
- **`users-create-receptionist-tries-create-another-user-blocked.e2e.ts`** -- Login as receptionist; navigate to `/admin/users` -> `<RequireRole>` redirects to `/no-access`. Per §7.36 (phase-03's `/admin/*` guard) -- but phase-02 owns the component.
- **`settings-concurrent-edit-parks-conflict.e2e.ts`** -- Two devices edit `settings.dye_cost_iqd` offline; both reconnect; one's push receives 409 + `ConflictParked` row inserted server-side.

### §4.3 Multi-device flows (`MULTI_DEVICE=true`)

| Spec | Scenario | Pass criteria |
|-|-|-|
| `two-device-user-lww.e2e.ts` | Device A edits user X's name "Asma A."; Device B edits "Asma B." 1s later. Both reconnect. | Server keeps Device B's (later updatedAt). Both devices converge to "Asma B." after pull. |
| `two-device-user-lww-tiebreak.e2e.ts` | Identical `updatedAt` (clock-skew rig). | Lex-smaller `originDeviceId` wins. Per §7.23. |
| `two-device-settings-manual-conflict.e2e.ts` | Device A sets `dye_cost_iqd = 12000`; Device B sets `dye_cost_iqd = 15000`. Both offline. Both reconnect. | A's push wins (first to arrive); B's push returns 409 + `ConflictParked`. Per §7.19. UI on B shows a conflict indicator. |
| `two-device-soft-delete-user-revokes-tokens-globally.e2e.ts` | Device A soft-deletes user X. Device A reconnects. Device B (where user X was logged in) attempts a sync. | Device B's next `/sync/push` returns 401; user X is signed out on Device B; outbox preserved for re-login. Per §7.5. |
| `two-device-password-change-propagates-and-clears-cache.e2e.ts` | Superadmin on Device A resets user X's password. Device A reconnects. Device B pulls. | Per §7.27: Device B's stronghold `creds/<email>` deleted; next offline login for user X fails. |

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase

- **Visual: `/login` in both locales.** Eyebrow rule on the right (RTL) or left (LTR); password input shows the password-visibility toggle; submit button is the single crimson primary per design-system §9.
- **Lock-screen modality.** Lock screen traps focus; only password input is focusable; Escape does NOT close; user cannot navigate away.
- **Password-input keyboard.** Toggle password visibility with the eye icon; Caps Lock indicator renders (`a11y.icons.caps_lock_on`); Tab order: email -> password -> visibility-toggle -> submit.
- **Idle-lock UX.** With `idle_lock_minutes = 5`, simulate 5 min of inactivity; verify the lock screen renders with a soft fade-in, not a jarring switch.
- **`<SettingsForm>` per-key widgets.** Per §7.22: verify each widget renders correctly; thermal-width select shows only 32 / 48; thermal-printer combobox populates from `settings::list_printers` (forward-ref to phase-05).
- **Bootstrap flow visual.** Fresh install; `<FirstLaunchSetupModal>` renders centred; verify the i18n strings exist in both locales (per phase-09 §3 receipt).

### §5.2 Cross-references to `personas.md`

Phase 02 surfaces are exercised end-to-end by:
- `personas.md` -> **P3 Mariam the Superadmin** -> steps 1-3 (login, navigate `/admin/users` + `/admin/settings`, observe the audit log). Required for §8 DoD.
- `personas.md` -> **P1 Asma the Accountant** -> step 2 (login online with role-default redirect to `/accounting`). Reinforcement.
- `personas.md` -> **P2 Mehdi the Receptionist** -> step 2 (login + role-default redirect to `/reception`). Reinforcement.

**Canonical: P3 Mariam the Superadmin.** P3 MUST pass for §8 DoD.

---

## §6 Edge Case Coverage (8 mandatory categories)

### §6.1 Time / Timezone

- **JWT `iat` / `exp` use UTC.** Server issues `iat = Date.now() / 1000`; the test asserts `iat + 900 === exp` regardless of timezone. The Rust verifier compares against UTC.
- **Refresh-token `expiresAt` UTC.** Stored as Postgres `timestamptz`; the 30-day TTL is computed against UTC; never against local time.
- **Idle-lock timer uses monotonic local time.** `last_activity_at` is captured via `performance.now()` or `Instant::now()` (monotonic), NOT `Date::now()` (wall clock). Clock changes don't false-trigger or skip the lock.
- **Clock skew vs server for password-change.** Device 5 min ahead of server: password-change request includes a `client_time` claim; server tolerates +- 5 min skew; outside that range -> 400 with `error.code='CLOCK_SKEW'`. (Cross-reference `security.md` for the formal tolerance.)
- **DST defensive.** Same CI `grep` test from phase-01 §6.1 forbids `chrono_tz::Tz::Baghdad` in `domains/auth/` and `domains/settings/`.

### §6.2 i18n & RTL

- **en/ar swap on every phase-02 route.** `/login`, `/no-access`, `/lock`, `/admin/users`, `/admin/users/:id`, `/admin/settings`, `/setup/first-run`. Every visible string from `auth.*`, `admin.*`, or `errors:*`. Asserted by §2.4 + phase-08 §7.9 i18n lint (forward-ref).
- **Arabic-Indic numerals on settings values.** `dye_cost_iqd: 10000` renders as `"١٠٬٠٠٠ د.ع"` when `arabic_numerals=true`. Per §7.12 + §7.30.
- **RTL layout invariants.** `<SettingsForm>` field labels above inputs in both directions; the `<UserMenu>` last-synced timestamp leads its value in both directions; eyebrow rule on the leading edge.
- **Mixed-direction email + ASCII UUIDs.** Audit row delta containing an Arabic name change + an ASCII UUID renders without bidi mangling.
- **First-launch ar-forcing detector.** Per §7.11: fresh install with English OS locale still opens in `ar`. Asserted in `detectInitialLocale` test + the §4.1 `first-launch-bootstrap-and-login.e2e.ts` spec.

### §6.3 Offline & Network

- **Full offline login.** `login-online-then-offline-relaunch.e2e.ts` (§4.1). All `users::*` and `settings::*` read commands work offline; only `auth::change_password` is online-only.
- **Intermittent connection during push.** Per phase-01 pattern: push 5 user-edit ops; drop mid-3rd; engine resumes from op 3.
- **Token expiry mid-sync.** `session-expired-after-second-401-pauses-pushes-preserves-outbox.e2e.ts` (§4.2). Per §7.25.
- **Server returns 5xx during `/auth/login`.** The IPC surfaces `AppError::Sync(NetworkUnavailable)`; the UI offers a retry button; falls back to offline login if creds-cache exists.
- **Network drops mid-change-password.** Half-applied state: stronghold cache MUST NOT be updated until the server confirms 200. The test asserts the cache is unchanged on 5xx.
- **Server unreachable during `/auth/jwks` fetch at boot.** Per §7.10: if the fetch fails AND a stronghold pin exists, boot succeeds with a WARN log; if no pin exists (fresh install), boot fails clearly.

### §6.4 Concurrency & Conflicts

- **2-device same user (`last-write-wins`).** `two-device-user-lww.e2e.ts` (§4.3). Later `updatedAt` wins; tiebreak on `originDeviceId` lex per §7.23.
- **2-device same setting (`manual`).** `two-device-settings-manual-conflict.e2e.ts` (§4.3). 409 + ConflictParked.
- **3-device chain on `users` LWW.** Devices A, B, C all rename the same user offline; reconnect in random order; deterministic convergence on the highest `updatedAt`.
- **Conflict policy invocation.** Assert the policy registry: `('users', 'last-write-wins')`, `('settings', 'manual')`.
- **Conflict resolver round-trip.** Phase-02 conflicts are parked; resolution is owned by phase-08's UI. Phase-02 verifies that the `ConflictParked` row exists; the round-trip (parked -> resolve -> audit row + outbox unpark + re-push) is in phase-08-test.
- **Delete-vs-edit on users.** Per phase-01 §7.16: an incoming user edit at T1 against a local soft-delete at T2 -> soft-delete wins (deletion later). Asserted in `users_phase02.rs`.

### §6.5 Crash & Recovery

- **SIGKILL during `users::create` transaction.** Spawn the binary; fire `users::create`; kill the process between (a) the audit-log insert and (b) the users insert. Reopen; assert: no audit row, no users row, no outbox row. Tx atomicity holds.
- **SIGKILL during refresh-token rotation.** Server-side: kill the process between (a) revoking the old token and (b) issuing the new one. Postgres tx rolls back both; the user can re-present the old token (which is still valid because the revoke rolled back).
- **SQLite WAL after crash during settings update.** Kill the binary while the WAL has uncommitted frames. Reopen; assert recovery is clean, no orphan WAL files.
- **Stronghold cache crash recovery.** Kill mid-`change_password` after server commit but before stronghold update -> on next login, server-side hash is new; stronghold still has the old hash; offline login with new password fails (cache stale). The fix is a server-online relogin which re-caches. Document the recovery path.
- **Crash during bootstrap.** Kill during `users::create_first_admin`; on reopen, no user exists; the bootstrap modal renders again. Idempotent.

### §6.6 Scale & Performance

- **10 users CRUD.** Typical clinic has < 10 users; the list returns in < 10ms p99. Asserted in `perf_users_list_at_10`.
- **10 settings.** The bundle fetches in < 5ms p99.
- **Stronghold cache write/read.** Single key write < 10ms; read < 5ms. The cache is on-disk; cold-start read is slower (< 30ms p99 first time).
- **Login round-trip.** End-to-end (login -> server verify -> JWT issue -> stronghold cache -> AppState populate) < 500ms p95 online; < 100ms p95 offline.
- **Argon2id verify.** Per the pinned params from §2.1: verify takes ~50-100ms (deliberately slow). Asserted in `perf_argon2id_verify`.
- **Refresh round-trip.** < 200ms p95 (server-side tx is fast).

### §6.7 Security & Permissions

- **Role bypass at three layers (UI + IPC + server).** Per §7.28: receptionist tries `users::create` via dev-tools dispatch -> IPC returns `AppError::Forbidden`. Server-side: `/sync/push` with `users` row from a non-superadmin JWT -> 403. Test all 3 boundaries.
- **JWT tampering on `users` push.** Alter `role` claim from `receptionist` to `superadmin`; server verifies RS256, signature fails -> 401. Cross-cutting in `security.md`.
- **Refresh-token theft.** A stolen refresh token used after rotation -> 401. The presented (revoked) token cannot be used; the new (unstolen) token works. Asserted in §2.3.
- **Refresh-token replay.** Same token used twice in rapid succession: the second use sees `revokedAt != null` -> 401. Asserted in §2.3. Full matrix in `security.md`.
- **Password not logged.** Per §1.1 + phase-01 §7.14: emit `auth::login` event; capture event stream; assert raw password bytes never appear; the field is `[REDACTED]`.
- **`password_hash` never crosses IPC boundary.** Per §7.20: every IPC response (list, get, create, update) strips the hash. Type-level proof via `UserResponse` struct.
- **`password_hash` excluded from `users::update` push.** Per §7.24: TypeBox `Type.Never` rejects update payloads with the field. Defence in depth -- a compromised client can't smuggle a hash change through the update path.
- **Stronghold creds key derivation.** The cache key is `creds/<email>`, NOT `creds/<email>:<password>`. Per §1.1 helper.
- **Soft-delete bypass.** Soft-delete a user; raw `SELECT * FROM users WHERE id = ?` shows the row (tombstone, not hard delete); the `users::list` IPC excludes it.
- **JWT pinning on app boot.** Per §7.10: a different key in stronghold than the server's current public key -> refuses to start (defends against MITM-replaced server).

### §6.8 Data Integrity

- **Migration replay forward.** `002_users_settings.sql` idempotent on fresh DB + on populated DB. The audit_log FK rebuild only runs if the FK is missing (idempotent check on `PRAGMA foreign_key_list`).
- **Migration replay against populated DB.** Pre-load phase-01 baseline + 1 user; replay 002; user row preserved.
- **FK enforcement.** Insert audit row with `actor_user_id` pointing to a non-existent user -> FK violation. Per §1 modified-table.
- **`users_email_unique` partial index.** Two users with same email AND `deleted_at IS NULL` -> second insert blocked. A third user with same email but `deleted_at != null` is allowed.
- **`settings_key` partial index.** Same shape.
- **CHECK constraint on `users.role`.** Insert `role = 'shareholder'` -> CHECK violation.
- **CHECK constraint on `settings.value_type`.** Insert `value_type = 'json'` -> CHECK violation.
- **`sync_version` monotonicity.** Every mutation increments `version` by exactly 1. Asserted in `version_increments_per_users_mutation` + `..._per_settings_mutation`.
- **Bootstrap idempotency.** Per §7.21: `users::create_first_admin` can only succeed once. Subsequent calls -> `FirstAdminExists`. The server-side `Prisma seed.ts` is idempotent too.

---

## §7 Performance SLOs (this phase's surfaces)

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|
| Tauri (SQLite) | `users::list` over 10 users | < 5 ms p99 | yes | `perf_users_list_at_10` | Default single-record SLO. |
| Tauri (SQLite) | `settings::list` over 10 keys | < 5 ms p99 | yes | `perf_settings_list_at_10` | Default. |
| Tauri (SQLite) | `users::create` full tx (with `with_audit`) | < 30 ms p99 | no (tighter than §9's 200ms lock SLO because users-create is a single-row mutation; no fan-out) | `perf_users_create_typical_under_30ms` | High-frequency surface during admin setup. |
| Tauri (SQLite) | `settings::update` full tx | < 30 ms p99 | no | `perf_settings_update_typical_under_30ms` | Same rationale. |
| Tauri (Stronghold) | Argon2id verify (offline login) | 50-100 ms (deliberately slow) | -- | `perf_argon2id_verify_pinned_params` | The slowness is the security property; lower is a bug. |
| Tauri (IPC) | `auth::login` online round-trip | < 500 ms p95 | -- | `perf_auth_login_online_round_trip` | Network + Argon2id + JWT issue + stronghold write. |
| Tauri (IPC) | `auth::login` offline round-trip | < 100 ms p95 | -- | `perf_auth_login_offline_round_trip` | Stronghold read + Argon2id verify. |
| Tauri (IPC) | `auth::refresh` round-trip | < 200 ms p95 | -- | `perf_auth_refresh_round_trip` | -- |
| Tauri (IPC) | `auth::lock` / `auth::unlock` | < 50 ms p99 | -- | `perf_auth_lock_unlock_under_50ms` | UI-visible; must be snappy. |
| Sync server (Postgres) | `/auth/login` handler | < 200 ms p95 | yes | `perf_server_login_under_200ms` | Per §9 default + Argon2id verify. |
| Sync server (Postgres) | `/auth/refresh` handler | < 100 ms p95 | no (tighter than login because no Argon2id) | `perf_server_refresh_under_100ms` | -- |
| Sync server (Postgres) | `/sync/push` 50-op mixed users + settings batch | < 200 ms p95 | yes | `perf_server_push_50_users_settings_batch` | Per §9. |
| Frontend | `<LoginForm>` first paint cold | < 200 ms | -- | `perf_login_form_cold_paint` | The first thing the user sees. |
| Frontend | `<AdminUsersPage>` first paint with 10 rows | < 150 ms | -- | `perf_admin_users_paint` | -- |
| Frontend | `<SettingsForm>` first paint | < 200 ms | -- | `perf_settings_form_paint` | Many widgets but small dataset. |

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI (`cargo test -p app_lib --lib` + `vitest run --project unit`).
- [ ] All §2 integration tests green in CI:
  - `cargo test --test auth_phase02 --test users_phase02 --test settings_phase02`
  - IPC handler tests for all 19 commands listed in §2.2.
  - `pnpm --filter sync-server test -- auth/auth-phase02 sync/users-and-settings-phase02`
  - `vitest run --project integration`
- [ ] All §3 contract tests green in CI (`pnpm test:contract`).
- [ ] All §4 E2E tests green in CI on linux-x86_64 (`pnpm test:e2e -- auth/ admin/`); multi-device specs green with `MULTI_DEVICE=true`.
- [ ] §5 persona script **P3 Mariam the Superadmin** runs end-to-end and passes.
- [ ] §6 all eight edge categories addressed.
- [ ] §7 SLOs met for every row.
- [ ] Coverage gates met per §1.3.
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed:
  - `expected/sync/user-create-push-canonical.json.sha256`
  - `expected/sync/user-update-push-canonical.json.sha256`
  - `expected/sync/user-pull-row-canonical.json.sha256`
  - `expected/sync/setting-push-canonical.json.sha256`
  - `expected/sync/setting-pull-row-canonical.json.sha256`
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green.

**Persona run record:**

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): **P3 Mariam the Superadmin** | -- | -- | -- | -- |
| P1 Asma the Accountant (reinforcement) | -- | -- | -- | Optional, exercises accountant role-default + RTL Arabic-Indic. |
| P2 Mehdi the Receptionist (reinforcement) | -- | -- | -- | Optional, exercises receptionist role-default + idle-lock. |

---

## §9 Gap Analysis Pass 1 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-1.md`](gap-analysis-pass-1.md). The `Target test section` line names the existing §X.Y subsection that should incorporate the new test row(s); the additions are kept here during Pass 2 verification, then merged into their target sections during test authoring. When Pass 2 re-runs, every gap below must show as covered.

### §9.1 P02-G01 -- `auth:refreshed` event emission on successful refresh (HIGH)

- **Source:** phase-02.md §4 Tauri `AuthService::refresh` step 2
- **Target test section:** §2.1 `auth_phase02.rs`
- **Category:** Missing Integration Test

§2.1 already exercises the 401 branch of `AuthService::refresh` (`refresh_revoked_token_returns_401`, `refresh_expired_token_returns_401_with_distinct_code`) but never asserts that the 200 success path emits the `auth:refreshed` event the build spec mandates. Without this row, a regression that silently drops the event would still leave §2.1 green while breaking every downstream subscriber (`useCurrentUser` cache-bust, `<UserMenu>` last-synced refresh).

| Scenario | Asserts |
|-|-|
| `refresh_200_emits_auth_refreshed_event_with_new_pair` | Pre-seed a valid refresh token; wiremock returns 200 with new `{accessToken, refreshToken}`; subscribe to `auth:refreshed` via `tauri::test::mock_app().listen_global`; call `auth::refresh`; assert exactly ONE event fires with payload `{ refreshed_at: <ISO ts> }`; assert no event fires when the 401 branch runs. |

### §9.2 P02-G02 -- TypeBox audit-action union on `/sync/push` (HIGH)

- **Source:** phase-02.md §7.18 audit action TypeBox union
- **Target test section:** §3.1
- **Category:** Missing Contract Test

§3.1 covers `LoginBodySchema`, `LoginResponseSchema`, `UserCreatePushSchema`, and the settings push family, but the TypeBox literal-union enforcement for `audit_log.action` on `/sync/push` is never validated -- only the SQLite-side CHECK constraint (§2.1 migration tests). A server-only push of an audit row with `action = 'nuke'` should be rejected at the TypeBox layer before it ever touches Prisma.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (audit_log request) | `AuditLogPushSchema` (`action: Type.Union([Type.Literal('login'), Type.Literal('password_change'), Type.Literal('create'), Type.Literal('update'), Type.Literal('soft_delete'), Type.Literal('bootstrap_admin')])`) | `fixtures/payloads/audit-log-push-known-actions.json` MUST validate for every action value; `fixtures/payloads/audit-log-push-unknown-action.json` (`action: 'nuke'`) MUST fail Ajv with the union-discriminator error. |

### §9.3 P02-G03 -- `useUserUpdate` mutation hook coverage (HIGH)

- **Source:** phase-02.md §3 Frontend hooks `useUserUpdate`
- **Target test section:** §2.4
- **Category:** Missing Unit Test

§2.4 lists `useUserCreate`, `useUserSoftDelete`, `useUserResetPassword`, but omits `useUserUpdate`. The build spec lists all four mutations as first-class React Query hooks; the rename / role-swap / `is_active` toggle path has no asserted cache-invalidation contract.

| Hook | Test | Asserts |
|-|-|-|
| `useUserUpdate` | `dispatches_users_update_and_invalidates_users_list_and_detail_keys` | Mock IPC returns the updated `UserResponse`; assert the mutation invalidates both `['users','list']` and `['users','detail', id]` keys; assert no `password_hash` field on the optimistic cache write; runs under `describe.each([['ltr'],['rtl']])`. |
| `useUserUpdate` | `surfaces_forbidden_error_toast_when_non_superadmin_caller` | Mock IPC returns `AppError::Forbidden`; assert a toast renders via `errors:auth.forbidden`; assert neither cache key is invalidated. |

### §9.4 P02-G04 -- `deviceId` round-trip through login + refresh (HIGH)

- **Source:** phase-02.md §3 `RefreshToken.deviceId` + §4 login step 4
- **Target test section:** §2.3
- **Category:** Missing Integration Test

The Prisma model carries `RefreshToken.deviceId`, but no §2.3 row asserts the client's submitted `deviceId` persists onto the row at login and is echoed back unchanged through the refresh rotation. Drift here would break multi-device revoke + the per-device "session list" UI planned for phase-08.

| Route | Test | Asserts |
|-|-|-|
| `POST /auth/login` | `login_persists_device_id_onto_refresh_token_row` | Client posts `{email, password, deviceId: 'device-A-uuid'}`; assert the inserted `RefreshToken.deviceId == 'device-A-uuid'`; a second login from the same user with a different `deviceId` inserts a SECOND row (no collision). |
| `POST /auth/refresh` | `refresh_propagates_device_id_from_presented_token_to_new_row` | Pre-seed a refresh row with `deviceId = 'device-A-uuid'`; call refresh; assert the new row also has `deviceId = 'device-A-uuid'`; the field is NOT taken from the request body. |
| `POST /auth/refresh` | `refresh_rejects_device_id_mismatch_in_request_body` | Pre-seeded `deviceId = 'device-A'`; request body claims `deviceId = 'device-B'` -> 401 `AUTH_DEVICE_MISMATCH`. |

### §9.5 P02-G05 -- Refresh-token 30-day TTL invariant (MEDIUM)

- **Source:** phase-02.md §4 server `AuthService::login` step 4 ("30-day lifetime")
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§2.3 asserts the access-token 15-min TTL (`exp - iat == 900`) but never the 30-day refresh-token TTL. A regression that pins refresh-tokens to 30 minutes or 30 hours would still leave §2.3 green while silently logging users out daily.

| Route | Test | Asserts |
|-|-|-|
| `POST /auth/login` | `login_persists_refresh_token_with_30_day_ttl` | Capture insert; assert `expiresAt.getTime() - createdAt.getTime() === 30 * 24 * 60 * 60 * 1000` exactly (no skew tolerance -- the constant is hard-coded). |
| `POST /auth/refresh` | `refresh_resets_ttl_to_30_days_from_now_on_rotation` | Pre-seed a token created 5 days ago; rotate; assert the NEW row's `expiresAt` is 30 days from the rotation moment, NOT 25 days from now (the rotation refreshes the lifetime). |

### §9.6 P02-G06 -- Atomic UserContext + settings_cache replace on re-login (MEDIUM)

- **Source:** phase-02.md §7.14 atomic UserContext+settings_cache replace
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1 covers `logout_clears_user_context_and_settings_cache_and_revokes_token`, but the build spec requires the inverse invariant: re-login replaces BOTH fields under a single write lock. A race where `UserContext` flips to user B while `settings_cache` still belongs to user A would leak cross-tenant settings into the UI.

| Scenario | Asserts |
|-|-|
| `relogin_replaces_user_context_and_settings_cache_under_single_write_lock` | User A logged in; settings_cache populated for tenant A. Log out, log in as user B (different tenant). Spawn a parallel reader that polls `AppState` in a tight loop during the relogin. Assert no observed snapshot has `user_context.entity_id = A` AND `settings_cache.entity_id = B` (or vice versa). Both fields flip in the same `RwLock::write` scope. |

### §9.7 P02-G07 -- Hard-coded IQD grep as CI gate (MEDIUM)

- **Source:** phase-02.md §7.30 IQD grep
- **Target test section:** §6.2 / §8 DoD
- **Category:** Missing Coverage Gate

§7.30 mandates that no `'د.ع'` literal appears in `src/` outside `src/lib/format/money.ts` (the helper reads the symbol from settings). The grep is documented but never wired as a CI check or added to the §8 DoD checklist. Without enforcement, the rule silently drifts.

Append to §6.2:

- **Hard-coded IQD literal grep.** CI step `pnpm lint:iqd-literal` runs `! grep -RInE "['\"]د\\.ع['\"]" src/ --exclude=src/lib/format/money.ts`; non-zero exit fails the build. Asserted by a fixture test that introduces a violating line and confirms the grep fails.

Append to §8:

- [ ] `pnpm lint:iqd-literal` green (no hard-coded `'د.ع'` literal outside `src/lib/format/money.ts`).

### §9.8 P02-G08 -- Client-side RS256 JWT signature verification (MEDIUM)

- **Source:** phase-02.md §5 `jsonwebtoken` client-side RS256
- **Target test section:** §3.2 / §2.1
- **Category:** Missing Contract Test

§2.1 covers `bootstrap_jwt_key_*` scenarios (the pin lifecycle) but never asserts the runtime verification path: every JWT returned from a sync call is RS256-verified against the pinned stronghold key before its claims are trusted. A regression that accepts unsigned tokens would still pass §2.1.

| Scenario | Asserts |
|-|-|
| `jwt_verifier_accepts_token_signed_with_pinned_public_key` | Use `jsonwebtoken::encode` to mint a token with the test private key; verify via the production `JwtVerifier::verify`; assert claims round-trip; assert the algorithm header is `RS256`. |
| `jwt_verifier_rejects_token_with_wrong_signature` | Mint a token with a different private key; `JwtVerifier::verify` -> `Err(AuthError::JwtSignatureInvalid)`. |
| `jwt_verifier_rejects_token_with_alg_none_header` | Hand-craft a token with `"alg": "none"`; verify -> `Err(AuthError::JwtAlgorithmRejected)`. Defends against the classic alg-confusion attack. |
| `jwt_verifier_rejects_token_with_hs256_header_using_public_key_as_secret` | Hand-craft an HS256 token signed with the public-key bytes; verify -> `Err(AuthError::JwtAlgorithmRejected)`. The verifier MUST pin algorithm to `RS256`. |

### §9.9 P02-G09 -- `useCurrentUser` cache-bust on auth + settings events (MEDIUM)

- **Source:** phase-02.md §3 `auth::current_user` cache-bust contract
- **Target test section:** §2.4
- **Category:** Missing Integration Test

§2.4 asserts `useCurrentUser` returns the in-memory user, but never that it subscribes to `auth:refreshed` and `settings:changed` to invalidate its cache. Without this, a profile-edit (name change pushed from another device + pulled here) leaves a stale `<UserMenu>` until manual refresh.

| Hook | Test | Asserts |
|-|-|-|
| `useCurrentUser` | `invalidates_when_auth_refreshed_event_fires` | Mount the hook; emit `auth:refreshed` via the IPC test harness; assert the query key `['auth','current-user']` is marked stale; assert a refetch occurs and the new `UserResponse` lands in the cache. |
| `useCurrentUser` | `invalidates_when_settings_changed_event_fires` | Mount the hook; emit `settings:changed` with `{ key: 'arabic_numerals' }`; assert the query refetches (because role-default routing and number rendering depend on settings). |
| `useCurrentUser` | `unsubscribes_on_unmount` | Mount + unmount; emit `auth:refreshed` after unmount; assert no refetch is triggered (no listener leak). |

### §9.10 P02-G10 -- `touchstart` activity reset in E2E (LOW)

- **Source:** phase-02.md §4 `<IdleWatcher>` touchstart
- **Target test section:** §4 / §6.3
- **Category:** Missing E2E Scenario

§2.4 lists `touchstart` among the events that reset `<IdleWatcher>`, but no E2E spec exercises a touch-input path. The covered E2E specs use mousemove / keydown only; on a touch-only kiosk a regression that drops `touchstart` from the listener list would silently re-introduce idle-lock during active use.

Append to §4.1:

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `idle-watcher-resets-on-touchstart.e2e.ts` | Mehdi (touchscreen kiosk) | 1) Set `settings.idle_lock_minutes = 1`. 2) Dispatch synthetic `touchstart` every 30s for 90s. 3) After 90s, assert NO lock screen. 4) Stop touch events; wait 65s. 5) Assert lock screen renders. | The lock fires from idle, not from absence of mouse/keyboard. Touch alone keeps the session alive. |

### §9.11 P02-G11 -- Red-dot threshold visual review (LOW)

- **Source:** phase-02.md §7.13 `<UserMenu>` red dot
- **Target test section:** §5.1
- **Category:** Missing Persona / Manual Step

§7.13 specifies a red dot on `<UserMenu>` when `last_pushed_at > 5min` AND the outbox is non-empty. §2.4 component test covers the logic, but no manual script eyeballs the visual treatment (size, color match to `--crimson`, off-by-one on the 5-minute boundary).

Append to §5.1:

- **`<UserMenu>` red-dot threshold visual review.** Force `last_pushed_at = now - 4min`; assert NO red dot rendered. Tick to `now - 5min01s` with outbox depth >= 1; assert red dot renders at 6px in `--crimson`, positioned top-right of the avatar per design-system §5.4. Empty outbox at >= 5min: still no dot. Both directions (`ltr` + `rtl`) -- dot mirrors to the leading corner.

### §9.12 P02-G12 -- Canonical login response envelope snapshot (LOW)

- **Source:** phase-02.md §3 `LoginResponseSchema`
- **Target test section:** §3.3 / §8
- **Category:** Missing Snapshot

§3.3 commits five sync-envelope snapshots but no canonical snapshot of the `POST /auth/login` response envelope. Future contract drift (a new field, a renamed key, a re-typed enum) would not surface as a snapshot diff -- only as an Ajv failure on the live route, which is silenced when the schema is updated in lockstep without review.

Append to §3.3:

- `expected/auth/login-response-canonical.json.sha256` -- captured from the canonical superadmin login against the seeded clinical-day fixture; covers `accessToken` (JWT structural shape only, not the signature), `refreshToken` (opaque), `user` (`UserResponse` with no `password_hash`), `role`, and `publicKey` (JWK Set entry). Regeneration requires §16 PR justification.

Append to §8:

- [ ] `expected/auth/login-response-canonical.json.sha256` committed and matched by the live `POST /auth/login` response.

---

## §10 Gap Analysis Pass 2 Additions

Each subsection below encodes one gap from [`gap-analysis-pass-2.md`](gap-analysis-pass-2.md). The format mirrors §9 exactly: one subsection per gap, with `Source` / `Target test section` / `Category` triplet and a scenario table whose rows are author-ready. The `Target test section` line names the existing §X.Y the new row(s) merge into during test authoring; the additions remain here as the audit trail until a clean Pass 3 verifies them.

### §10.1 P02-G13 -- `UserResetPasswordPushSchema` variant on `/sync/push` (HIGH)

- **Source:** phase-02.md §7.24 `users` push asymmetric `password_hash` rule
- **Target test section:** §3.1 / §3.3
- **Category:** Missing Contract Test

§3.1 of the test plan covers `UserCreatePushSchema` (which embeds `password_hash`) and the generic `user-update-push` shape (which forbids it), but the third legal variant -- the push emitted by `users::reset_password` -- is never schema-validated. §7.24 makes the rule explicit: `password_hash` is REQUIRED on `users::create` AND `users::reset_password` envelopes and FORBIDDEN on `users::update`. A regression that accepted a `reset_password` push without the hash (or worse, treated the omitted hash as "no change") would silently break credential rotation across devices.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (users `reset_password` op) | `UserResetPasswordPushSchema` (`{ op: Type.Literal('reset_password'), id: Type.String({ format: 'uuid' }), password_hash: Type.String({ minLength: 60 }), updated_at: Type.String({ format: 'date-time' }), op_id: Type.String({ format: 'uuid' }) }`) | `fixtures/payloads/user-reset-password-push-valid.json` MUST validate. `fixtures/payloads/user-reset-password-push-missing-hash.json` (no `password_hash`) MUST fail Ajv with `required` error on `password_hash`. `fixtures/payloads/user-reset-password-push-empty-hash.json` (`password_hash: ""`) MUST fail Ajv with `minLength` error. |
| `POST /sync/push` (defence-in-depth role gate) | `users_reset_password_push_rejects_non_superadmin_jwt` | Even with a valid `UserResetPasswordPushSchema` body, a JWT carrying `role: 'receptionist'` returns 403 per §7.24 + §7.28. |

### §10.2 P02-G14 -- 12-value closed audit-action union enforcement (HIGH)

- **Source:** phase-02.md §7.18 closed audit-action union
- **Target test section:** §9.2 / §3.1
- **Category:** Missing Contract Test

§9.2 (Pass 1) widened the audit-action union test, but it still enumerates only 6 of the 12 legal literals (`login, password_change, create, update, soft_delete, bootstrap_admin`). §7.18 closes the union at exactly 12 values; the missing six (`logout, lock, void, clock_in, clock_out, conflict_resolve, vacuum`) are emitted by phase-02 (`logout`, `lock`), phase-03 (`void`), phase-04 (`clock_in`, `clock_out`), and phase-08 (`conflict_resolve`, `vacuum`). Without coverage, any of these could be silently dropped from the TypeBox union and only fail at the entity boundary -- after a year of audit rows.

| Route | Schema id | Sample payload |
|-|-|-|
| `POST /sync/push` (audit_log request, exhaustive union) | `AuditLogPushSchema` 12-literal coverage | Add to `fixtures/payloads/audit-log-push-known-actions.json` ONE entry per literal in `{login, logout, lock, password_change, create, update, soft_delete, void, clock_in, clock_out, conflict_resolve, vacuum}`. Ajv validates each. A 13th payload with `action: 'bootstrap_admin'` ALSO validates (it is the 12th legal value per §7.21 -- correct the count if mismatched; the canonical list lives in `src-tauri/src/audit/action.rs::AuditAction::from_str`). |
| `POST /sync/push` (negative) | `audit_log_push_rejects_thirteenth_unknown_action` | A payload with `action: 'export'` (a phase-07 candidate) MUST fail Ajv with the union-discriminator error. The union is closed; adding a value requires updating both `src-tauri/src/audit/action.rs` and `sync-server/src/audit/action-schema.ts` together. |

### §10.3 P02-G15 -- Server Prisma `seed.ts` BOOTSTRAP_SUPERADMIN integration test (HIGH)

- **Source:** phase-02.md §7.21 first-launch superadmin bootstrap UX
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§7.21 specifies that the server's `prisma/seed.ts` reads `BOOTSTRAP_SUPERADMIN_EMAIL`, `BOOTSTRAP_SUPERADMIN_PASSWORD`, and `BOOTSTRAP_TENANT_ID` from env, inserts ONE superadmin with an Argon2id hash, and is idempotent (no-op when any superadmin row already exists). `Dockerfile.dev` runs `pnpm prisma db seed` after `migrate-deploy`. §2.3 tests `/auth/login` happy paths but never drives the seed itself, so a regression that (a) skipped the existence check, (b) used a weaker hash, or (c) re-created the user on every container restart with a new ID would pass §2.3 while breaking the bootstrap contract.

| Route / script | Test | Asserts |
|-|-|-|
| `pnpm prisma db seed` | `seed_creates_single_superadmin_from_env_with_argon2id_hash` | Empty Prisma test DB; set env `BOOTSTRAP_SUPERADMIN_EMAIL=root@idc`, `BOOTSTRAP_SUPERADMIN_PASSWORD=Root!1234`, `BOOTSTRAP_TENANT_ID=<uuid>`; run `seed.ts`. Assert: ONE `User` row with `role='superadmin'`, `email='root@idc'`, `passwordHash` starts with `$argon2id$`. The hash verifies against the plaintext via `argon2.verify`. `tenantId` matches the env. `pulledAt` is NULL (server-only). |
| `pnpm prisma db seed` | `seed_is_idempotent_when_superadmin_already_exists` | Pre-seed a superadmin (different email + tenant). Run `seed.ts` with the same env as the previous test. Assert: NO new row inserted; the pre-existing row is untouched (hash, email, ID, tenant all unchanged). Exit code is 0. |
| `pnpm prisma db seed` | `seed_fails_loudly_when_env_missing` | Unset `BOOTSTRAP_SUPERADMIN_PASSWORD`; run `seed.ts`. Process exits non-zero with a message naming the missing env var. No partial row inserted (transaction rollback). |

### §10.4 P02-G16 -- `settings::set_locale` IPC happy + error paths (HIGH)

- **Source:** phase-02.md §7.28 IPC role-gate symmetry
- **Target test section:** §2.2
- **Category:** Missing Integration Test

§7.28 lists `settings::set_locale` as a superadmin-gated command alongside `settings::update`, but the §2.2 IPC table never enumerates it. The frontend uses it from the locale picker; without an integration row, a regression that (a) dropped the role gate, (b) skipped the locale-allowlist check (`en` | `ar`), or (c) failed to emit `settings:changed` would not be caught. The build spec mandates the same gate as `settings::update`: `require_role(&app_state, &[Role::Superadmin])?`.

| Command | Test | Asserts |
|-|-|-|
| `settings::set_locale` | `set_locale_happy_path_superadmin_persists_and_emits_event` | Superadmin context; call with `{ locale: 'ar' }`. Assert: `settings` row for key `locale` upserts to `'ar'`; `settings:changed` event fires exactly once with payload `{ key: 'locale', value: 'ar' }`; audit row written with `action='update'`, `entity='settings'`, `entity_id=<locale row id>`. |
| `settings::set_locale` | `set_locale_rejects_non_superadmin_caller` | Receptionist context; call with `{ locale: 'ar' }`. Returns `AppError::Auth(AuthError::Forbidden)`. No row mutation; no event emission; no audit row. |
| `settings::set_locale` | `set_locale_rejects_unknown_locale_value` | Superadmin context; call with `{ locale: 'fr' }`. Returns `AppError::Validation("locale must be 'en' or 'ar'")`. No row mutation; no event. |

### §10.5 P02-G17 -- Client `AuthService::login` writes `action='login'` audit row (HIGH)

- **Source:** phase-02.md §4 Tauri `AuthService::login` step 2
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§4 step 2 mandates that a successful login writes an audit row with `action='login'` (the value freshly added to the audit-action union by this phase). Existing §2.1 scenarios assert "an audit row is written" generically but never inspect the `action` column, so a regression that wrote `action='create'` (the closest legal pre-phase-02 value) would still leave §2.1 green while making the audit log useless for login-attempt forensics.

| Scenario | Asserts |
|-|-|
| `login_200_writes_audit_row_with_action_login_and_actor_user_id` | Pre-seed user; wiremock returns 200 with valid `{accessToken, refreshToken, user, publicKey}`. Call `auth::login(email, password)`. Inspect the newest `audit_log` row: `action='login'` (exact string), `actor_user_id = user.id`, `entity='users'`, `entity_id = user.id`, `delta` JSON deserializes to `{ method: 'password' }` (or `{}` if the spec stays minimal -- pin to the literal §4 step 2 contract). Created at the same logical instant as the `UserContext` flip. |
| `login_401_writes_no_audit_row` | Wiremock returns 401. Call `auth::login`. Assert: audit log unchanged (failed logins are NOT audited at this layer; the server's `/auth/login` route owns that record). |

### §10.6 P02-G18 -- `auth::logout` writes `action='logout'` audit row (HIGH)

- **Source:** phase-02.md §7.18 closed audit-action union (`logout`) + §7.14 UserContext rotation on logout
- **Target test section:** §2.1 / §2.3
- **Category:** Missing Integration Test

The `logout` literal sits in the closed audit-action union (§7.18) but no test asserts it actually gets emitted. §7.14 specifies that `auth::logout` clears `UserContext`, revokes the refresh token, and clears `settings_cache`; the audit row is the only persistent trace that the session ended cleanly (vs. crashed or idle-locked). Without coverage, a regression that wrote `action='soft_delete'` or no row at all would pass current tests.

| Scenario | Asserts |
|-|-|
| `logout_writes_audit_row_with_action_logout_before_clearing_context` | Logged-in user; call `auth::logout`. Inspect `audit_log`: ONE new row with `action='logout'` (exact string), `actor_user_id = <user before logout>`, `entity='users'`, `entity_id = <user id>`. The row is committed BEFORE `UserContext` flips to `None` (audit-first ordering -- if the audit write fails the logout is aborted; assert with a forced SQLite error injection). |
| `logout_server_route_writes_logout_audit_row_via_acceptPush` | Server-side mirror: after the client pushes the `logout` audit row to `/sync/push`, the row persists in Postgres with `action='logout'`; TypeBox `AuditLogPushSchema` accepts the literal (covered by §10.2 but worth a positive end-to-end assertion here too). |

### §10.7 P02-G19 -- `tauri-plugin-os` and `jsonwebtoken` registered in `lib.rs::run()` (MEDIUM)

- **Source:** phase-02.md §5 plugin registrations + §7.26 lock-on-suspend
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§5 declares two new plugin registrations for this phase: `tauri-plugin-os` (used by §7.26 to dispatch `auth::lock` on suspend/resume) and `jsonwebtoken` (used by §10.8 for client-side RS256 verification). Both are easy to drop from `lib.rs::run()` after a refactor (no compile error -- the plugins are just builder calls). The §5 verification step `cd src-tauri && cargo test` will not catch a missing plugin unless a test explicitly asks the runtime whether the plugin is wired.

| Scenario | Asserts |
|-|-|
| `tauri_plugin_os_is_registered_at_runtime` | Boot a `tauri::test::mock_app()` built via the production `tauri::Builder` path used by `lib.rs::run()`. Call `app.plugin_state::<tauri_plugin_os::OsState>()` (or the canonical accessor); assert `Some(_)`. A regression that removed `.plugin(tauri_plugin_os::init())` returns `None` and fails the test. |
| `jsonwebtoken_verifier_is_constructable_via_app_state` | Boot the production app builder; resolve the `JwtVerifier` from `AppState`; assert it can be cloned and that `algorithm()` returns `RS256` (defence-in-depth on top of §10.8's algorithm-pin tests). |

### §10.8 P02-G20 -- Server `@fastify/jwt` RS256 keypair registration (MEDIUM)

- **Source:** phase-02.md §5 Fastify plugins (`@fastify/jwt` with loaded RS256 key pair)
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§5 server line: "`@fastify/jwt` configured with the loaded RS256 key pair." Existing §2.3 tests assert auth route behaviours (login, refresh, etc.) but never reach into the Fastify instance to assert the plugin is registered AND configured with the RS256 keys (vs. defaulting to HS256, which would silently weaken every signature). A misconfiguration that fell back to HS256 with a placeholder secret would pass every behavioural test until a client tried to verify a signature with the published JWK.

| Route | Test | Asserts |
|-|-|-|
| Boot (`buildApp()`) | `fastify_jwt_plugin_registered_with_rs256_keypair` | After `await app.ready()`, assert `app.hasDecorator('jwt') === true`; sign a test payload via `app.jwt.sign({ sub: 'test' })`; decode the resulting JWT header without verifying signature; assert `alg === 'RS256'`, `typ === 'JWT'`. Verify the signature with the public key loaded from disk; assert verification succeeds. |
| Boot (`buildApp()`) | `fastify_jwt_rejects_hs256_token` | Sign a test token externally with HS256 using a known secret; call any protected route with that token; assert 401 with code `AUTH_ALG_MISMATCH` (the server's defence-in-depth mirror of §10.8 client-side `jwt_verifier_rejects_token_with_hs256_header_using_public_key_as_secret`). |

### §10.9 P02-G21 -- Successful login does NOT mutate `jwt/publicKey` in stronghold (MEDIUM)

- **Source:** phase-02.md §7.10 JWT public-key fetch-and-pin at app start
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§7.10 makes `bootstrap_jwt_key` the SOLE writer of the `jwt/publicKey` stronghold entry. A regression that overwrote the pin from the `publicKey` field on every `/auth/login` response would silently break the pin-on-first-launch security model: a malicious sync server could rotate the pinned key by simply returning a different `publicKey` blob on login. Existing §2.1 covers `bootstrap_jwt_key_*` lifecycle but never asserts the inverse invariant for login.

| Scenario | Asserts |
|-|-|
| `login_does_not_overwrite_pinned_jwt_public_key_when_response_carries_different_key` | Pre-pin a known key K1 via `bootstrap_jwt_key`. Wiremock returns a `/auth/login` 200 whose `publicKey` field is a DIFFERENT key K2. Call `auth::login`. Inspect stronghold `jwt/publicKey`: bytes equal K1 (unchanged). The login itself succeeds (the JWT in the response was signed by K1; K2 is informational only and discarded). |
| `login_does_not_overwrite_pinned_jwt_public_key_when_response_omits_key_field` | Pre-pin K1. Wiremock returns a login 200 with `publicKey` omitted. Stronghold still holds K1; the login succeeds. |
| `login_refuses_when_response_jwt_signature_does_not_match_pinned_key` | Pre-pin K1. Wiremock returns a login 200 whose `accessToken` was signed by K2. Login returns `AppError::Auth(AuthError::JwtSignatureInvalid)`; stronghold still holds K1; `UserContext` unchanged. |

### §10.10 P02-G22 -- `thermal_printer_name` membership validation on save (MEDIUM)

- **Source:** phase-02.md §7.1 `thermal_printer_name` settings seed
- **Target test section:** §1.1 / §2.1
- **Category:** Missing Unit Test

§7.1 specifies that `thermal_printer_name` is free text validated against the result of `settings::list_printers()` on save (empty string = "use OS default"). Existing tests cover the seed and the required-key delete protection, but no test exercises the membership check on update. A regression that accepted any string would let a misconfigured printer name persist, surfacing only when phase-05 tries to print and the OS rejects the name.

| Module / IPC | Test | Asserts |
|-|-|-|
| `settings::update` (Rust integration) | `update_thermal_printer_name_accepts_empty_string` | Superadmin context. `settings::update { key: 'thermal_printer_name', value: '', valueType: 'text' }`. Persists; no error; no membership check invoked (empty = OS default per §7.1). |
| `settings::update` (Rust integration) | `update_thermal_printer_name_accepts_value_listed_by_list_printers` | Mock `settings::list_printers()` to return `["EPSON-TM-T20", "Generic / Text Only"]`. Update with `value: 'EPSON-TM-T20'` -- persists. |
| `settings::update` (Rust integration) | `update_thermal_printer_name_rejects_value_not_in_list_printers` | Mock returns `["EPSON-TM-T20"]`. Update with `value: 'HP-LaserJet'` -> `AppError::Validation("thermal_printer_name must be one of: EPSON-TM-T20 (or empty string)")`. Row unchanged. |
| `SettingsService::update` (Rust unit) | `thermal_printer_name_membership_check_short_circuits_on_empty_value` | Pure unit test on the service: pass `value=""`; assert `list_printers()` is NOT called (no I/O on the empty path). Pass `value="X"`; assert `list_printers()` IS called exactly once. |

### §10.11 P02-G23 -- `<SettingsForm>` atomic multi-key save (MEDIUM)

- **Source:** phase-02.md §7.22 `<SettingsForm>` value-type widget bindings
- **Target test section:** §4.1

- **Category:** Missing E2E Scenario

§7.22 specifies value-type widget bindings in `<SettingsForm>` but the existing tests only cover single-key updates via `useSettingUpdate`. The build spec's intent (a single form save mutating N keys in one transaction) is never exercised: if the user changes `arabic_numerals` AND `currency_symbol` AND `idle_lock_minutes` in one submission and the third write fails (e.g., role gate denial mid-batch), the first two MUST roll back. A regression that fired N independent mutations sequentially would leak half-applied state.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| `settings-form-atomic-multi-key-save.e2e.ts` | P3 Mariam (superadmin) | 1) Open `/admin/settings`. 2) Toggle `arabic_numerals` from off to on; change `currency_symbol` from `د.ع` to `IQD`; change `idle_lock_minutes` from 5 to 10. 3) Submit. | One IPC call (single `settings::update_batch` or transactional wrapper -- pin to the §7.22 contract). All three rows mutate atomically; on success, a single `settings:changed` event fires with `{ keys: ['arabic_numerals','currency_symbol','idle_lock_minutes'] }`; the form's dirty state resets. |
| `settings-form-atomic-multi-key-save-rollback.e2e.ts` | P3 Mariam | 1) Same starting state. 2) Inject a forced failure on the third write (e.g., set `idle_lock_minutes` to an invalid value `-1` post-validation via a debug hook, or mock the IPC to return `AppError::Validation` on the third key). 3) Submit. | NO settings row changed (`arabic_numerals` still off, `currency_symbol` still `د.ع`, `idle_lock_minutes` still 5). A single error toast renders; the form retains dirty state so the user can fix the invalid value. No `settings:changed` event fires. |

### §10.12 P02-G24 -- `users` `op_id` idempotency on `/sync/push` (MEDIUM)

- **Source:** phase-02.md §4 Sync Semantics `users` `last-write-wins` `op_id` + §7.18 ProcessedOp cache
- **Target test section:** §2.3
- **Category:** Missing Integration Test

§4 lists `users` as `last-write-wins` keyed by `op_id`; §7.18 (server-side settings flow, but the ProcessedOp pattern applies to all syncable entities) specifies the cache-and-replay envelope: `ProcessedOp.has(opId) -> return cached envelope`. The `settings` integration tests exercise this for the settings entity (§9.x or elsewhere), but no test asserts the same idempotency for `users`. A duplicate `users::create` push with the same `op_id` (e.g., client retried after a transient 500) MUST return the cached envelope, NOT insert a second row.

| Route | Test | Asserts |
|-|-|-|
| `POST /sync/push` (users entity) | `push_users_create_replay_same_op_id_returns_cached_envelope_without_insert` | First push: `{ op: 'create', op_id: <uuid-A>, user: {..., password_hash: '<argon2 hash>'} }` with superadmin JWT. Inserts; returns 200 with response envelope E1. Second push with IDENTICAL body and same `op_id`. Server returns 200 with response envelope BYTE-EQUAL to E1; `User` table still has exactly ONE row with that ID. Audit log shows exactly ONE `action='create'` row (not two). |
| `POST /sync/push` (users entity) | `push_users_reset_password_replay_same_op_id_returns_cached_envelope` | First push: `{ op: 'reset_password', op_id: <uuid-B>, id: <user-id>, password_hash: '<new hash>' }`. Returns 200 with envelope E1; `passwordHash` rotated. Second push with same `op_id` and ORIGINAL hash. Server returns 200 with envelope E1; `passwordHash` is UNCHANGED from the first rotation (the replay does NOT overwrite with the original hash). |
| `POST /sync/push` (users entity) | `push_users_update_different_op_id_same_payload_creates_new_audit_entry` | Negative control: same payload, DIFFERENT `op_id`. Returns a fresh envelope; audit log shows TWO `update` rows (the idempotency key is `op_id`, not payload hash). |

### §10.13 P02-G25 -- `SettingSchema` value coerced by `valueType` round-trip (MEDIUM)

- **Source:** phase-02.md §3 Frontend `SettingSchema`
- **Target test section:** §1.2
- **Category:** Missing Unit Test

§3 declares `SettingSchema` as `{ key, value, valueType }` "with value coerced by `valueType`." The CHECK constraint pins `valueType IN ('int','decimal','text','bool')`, so the Zod schema must coerce the stored string `value` to `number | number | string | boolean` per row. Without a coercion round-trip test, a regression that left `value` as `string` for all rows would silently break every numeric KPI rendering (`idle_lock_minutes` arithmetic, `dye_cost_iqd` formatting, etc.).

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/setting.ts` | `setting_schema_coerces_int_value_to_number` | Parse `{ key: 'idle_lock_minutes', value: '5', valueType: 'int' }`. Result `value` is `5` (typeof `'number'`, `Number.isInteger` true). Round-trip: serialize and re-parse; round-trip stable. |
| `src/lib/schemas/setting.ts` | `setting_schema_coerces_decimal_value_to_number_with_fraction_preserved` | Parse `{ key: 'dye_cost_iqd', value: '12500.75', valueType: 'decimal' }`. Result `value === 12500.75` (typeof `'number'`). Round-trip preserves the fraction; no floating-point loss beyond `Number.EPSILON`. |
| `src/lib/schemas/setting.ts` | `setting_schema_leaves_text_value_as_string` | Parse `{ key: 'clinic_display_name_en', value: 'IDC', valueType: 'text' }`. Result `value === 'IDC'` (typeof `'string'`). Empty string is permitted. |
| `src/lib/schemas/setting.ts` | `setting_schema_coerces_bool_value_from_canonical_strings` | Parse `{ key: 'arabic_numerals', value: 'true', valueType: 'bool' }` -> `value === true`. `value: 'false'` -> `false`. `value: '1'` and `'0'` MUST be rejected (only the literal `'true' | 'false'` are coerced; the SQLite layer stores those exact strings). Throws `ZodError` with a message naming the offending value. |
| `src/lib/schemas/setting.ts` | `setting_schema_rejects_mismatched_value_for_value_type` | `{ key: 'idle_lock_minutes', value: 'five', valueType: 'int' }` -> `ZodError`; `{ value: 'true', valueType: 'int' }` -> `ZodError`. The coercion is type-strict; invalid strings fail at parse time, not at first arithmetic use. |

### §10.14 P02-G26 -- `user-reset-password-push-canonical.json.sha256` snapshot (LOW)

- **Source:** phase-02.md §3.3 + §7.24 reset_password push envelope
- **Target test section:** §3.3 / §8
- **Category:** Missing Snapshot

§3.3 commits canonical push/pull snapshots for the user-create, user-update, and settings flows, but the third legal users-push variant -- the `users::reset_password` envelope -- has no snapshot. Per §10.1 + §7.24 it is structurally distinct: smaller than `user-create-push` (no `email` / `role` / `is_active` -- only `id`, `password_hash`, `updated_at`, `op_id`, `op: 'reset_password'`) and forbids fields permitted on `user-update-push`. A renderer-side or serializer-side change that merged the two shapes would slip through unless the snapshot exists.

Append to §3.3:

- `expected/sync/user-reset-password-push-canonical.json.sha256` -- captured from a canonical superadmin-issued `users::reset_password` op against the seeded clinical-day fixture; covers the exact field set `{ op: 'reset_password', id, password_hash, updated_at, op_id }` (no extras, no nulls). Regeneration requires §16 PR justification AND a cross-check that §10.1's `UserResetPasswordPushSchema` Ajv tests still pass against the new sample.

Append to §8:

- [ ] `expected/sync/user-reset-password-push-canonical.json.sha256` committed; live `users::reset_password` outbox push canonicalizes to the same hash.

---

## §11 Gap Analysis Pass 3 Additions

These rows encode the 8 Phase-02 gaps surfaced by [`gap-analysis-pass-3.md`](gap-analysis-pass-3.md) (P02-G27 through P02-G34). Pass 3 re-compared the build spec against the UNION of §1-§6 + §9 + §10; these are the remaining true gaps.

### §11.1 P02-G27 -- User<->OperatorShift back-relations enable prisma generate (HIGH)

- **Source:** phase-02.md §7.29 -- "Add `shiftsCheckedIn OperatorShift[] @relation('CheckIn')` and `shiftsCheckedOut OperatorShift[] @relation('CheckOut')` inverse fields on `User`".
- **Target test section:** §2.3
- **Category:** Missing Server Model

Missing inverse fields fail `prisma generate` with an explicit error, but no §2.3 row exercises generation against the phase-02 schema. A regression that dropped one of the two inverse fields would only be caught downstream during the first phase-04 server boot.

| Scenario | Asserts |
|-|-|
| `prisma_generate_succeeds_with_user_operatorshift_back_relations` | After applying the phase-02 schema additions, run `pnpm --filter sync-server prisma generate` in the test harness. Assert exit code 0, no `error: Field ... is missing an opposite relation field` messages in stderr. Snapshot the generated `prisma/client/index.d.ts` and assert it contains `shiftsCheckedIn: OperatorShift[]` and `shiftsCheckedOut: OperatorShift[]` properties on the `User` model interface. Per §7.29. |

### §11.2 P02-G28 -- users::get self-or-superadmin gate (HIGH)

- **Source:** phase-02.md §7.28 -- "users::get: self or superadmin".
- **Target test section:** §2.2
- **Category:** Missing IPC Handler Test

The explicit rule carve-out is not asserted. A receptionist could enumerate other users by id.

| Command | Test | Asserts |
|-|-|-|
| `users::get` | `get_returns_self_for_any_role` | Receptionist calls `users::get { id: own_id }` -> `Ok(user)` with the requester's record. |
| `users::get` | `get_returns_forbidden_for_other_user_when_caller_is_receptionist` | Receptionist calls `users::get { id: other_user_id }` -> `AppError::Forbidden`; no row leaked. Same for accountant role. |
| `users::get` | `get_returns_any_user_for_superadmin` | Superadmin calls `users::get { id: any_user_id }` -> `Ok(user)` regardless of self vs other. Per §7.28. |

### §11.3 P02-G29 -- users::list filtering rule (HIGH)

- **Source:** phase-02.md §7.28 -- "users::list: only excludes inactive unless superadmin".
- **Target test section:** §2.2
- **Category:** Missing IPC Handler Test

| Command | Test | Asserts |
|-|-|-|
| `users::list` | `list_excludes_inactive_for_receptionist` | Receptionist calls `users::list {}` -> result contains only `is_active=true` rows. With `{ includeInactive: true }` -> same result (the flag is silently ignored, NOT honoured). |
| `users::list` | `list_excludes_inactive_for_accountant` | Same as receptionist. |
| `users::list` | `list_honours_include_inactive_for_superadmin` | Superadmin calls `users::list { includeInactive: true }` -> result contains both active AND inactive rows. With `{}` (default) -> only active rows. Per §7.28. |

### §11.4 P02-G30 -- useUser(id) read hook (HIGH)

- **Source:** phase-02.md §3 Frontend React Query keys table -- `useUser(id)` returning `User` with key `['users','detail', id]`.
- **Target test section:** §2.4
- **Category:** Missing Unit Test

| Hook | Test | Asserts |
|-|-|-|
| `useUser(id)` | `fetches_user_via_users_get_with_correct_cache_key` | Mount the hook with a known user id; the IPC mock receives `users_get { id }`; the hook resolves to a `User` matching the response. Cache key MUST be `['users','detail', <id>]`. |
| `useUser(id)` | `invalidates_when_useUserUpdate_or_softDelete_resolves` | After `useUserUpdate(id).mutate(...)` resolves, the next `useUser(id)` read fetches fresh from IPC (not cache). Same for soft_delete. |

### §11.5 P02-G31 -- AuthService::change_password offline-required branch (MEDIUM)

- **Source:** phase-02.md §4 Tauri `AuthService::change_password` step 1 -- "Online required: return `OfflineNotAllowed` if network status is offline".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.2 has a stub returning `OfflineNotAllowed`; §2.1 doesn't drive the actual offline branch.

| Scenario | Asserts |
|-|-|
| `change_password_returns_offline_not_allowed_when_network_offline` | Seed `NetworkStatusProvider` as offline. Call `AuthService::change_password { current_password, new_password }`. Assert: (a) returns `AppError::OfflineNotAllowed`; (b) no HTTP call attempted (network mock receives 0 requests); (c) password_hash in stronghold UNCHANGED; (d) no refresh-token rotation; (e) `audit_log` row NOT written. |

### §11.6 P02-G32 -- UserService::update audit-delta shape (MEDIUM)

- **Source:** phase-02.md §7.6 -- "UserService::update wraps in `with_audit('update')` and captures a field-level delta".
- **Target test section:** §2.1
- **Category:** Missing Integration Test

§2.1 covers `users_update_normalizes_email_to_lowercase` and similar functional rows but doesn't inspect the audit delta JSON shape.

| Scenario | Asserts |
|-|-|
| `update_audit_row_carries_field_level_delta_shape` | Seed a user with `name='Old'`, `phone='+9647500000000'`, `role='receptionist'`. Update name + phone in one call (leave role unchanged). Inspect the resulting `audit_log` row: `delta` JSON MUST equal `{ name: { from: 'Old', to: 'New' }, phone: { from: '+9647500000000', to: '+9647511111111' } }` -- ONLY changed fields appear; unchanged fields omitted; each entry is the `{ from, to }` shape per §7.6. |

### §11.7 P02-G33 -- SIGKILL during refresh-token rotation (MEDIUM)

- **Source:** phase-02.md §6.5 -- "Crash & Recovery: SIGKILL during refresh-token rotation; assert old token still valid".
- **Target test section:** §6.5
- **Category:** Missing Edge Coverage

The narrative bullet exists; no test row drives the server-side mid-tx kill.

| Scenario | Asserts |
|-|-|
| `server_kill_mid_refresh_rotation_leaves_old_token_valid` | Issue `/auth/login` with credentials, capture `refreshToken_v1`. Inject a `process.kill(process.pid, 'SIGKILL')` hook into the `RefreshTokenRotation` service AFTER the new token row is INSERTed but BEFORE the old token row's `revokedAt` is set. POST `/auth/refresh { refreshToken: refreshToken_v1 }`; assert process dies. Restart the server; POST `/auth/refresh { refreshToken: refreshToken_v1 }` again -> 200 with a fresh access+refresh pair (the rotation tx rolled back; old token still valid). Per §6.5 + §4 server rotation atomicity. |

### §11.8 P02-G34 -- First-launch ar-forcing visual review (LOW)

- **Source:** phase-02.md §4.1 ar-forcing detector + §6.2 first-launch.
- **Target test section:** §5.1
- **Category:** Missing Persona / Manual Step

The unit test asserts string equality, not pixel layout.

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|
| Manual visual review: first-launch ar-forcing splash | P3 Mariam (fresh install) | 1) Delete the app data directory (or fresh-install on a clean VM with `LANG=ar_IQ.UTF-8` or any non-en system locale). 2) Boot the app; observe the bootstrap/login splash BEFORE any user interaction. 3) Verify: page direction is `rtl`; eyebrow rule sits on the right; form labels and placeholder text render in Arabic; date displays use Arabic-Indic numerals if `arabic_numerals` defaulted on. 4) Switch system locale to `en_US.UTF-8`; re-launch; observe the splash defaults to `ltr` with English copy. | Visual layout matches design-system §12 RTL conventions on first launch with no `settings_cache` entry; `detectInitialLocale` selection is reflected in the actual rendered chrome, not just the i18n string. |

---

## §12 Gap Analysis Pass 4 Additions

These rows encode the 4 Phase-02 gaps surfaced by [`gap-analysis-pass-4.md`](gap-analysis-pass-4.md) (P02-G35 through P02-G38). Pass 4 re-compared the build spec against the UNION of §1-§6 + §9 + §10 + §11; these are the remaining true gaps.

### §12.1 P02-G35 -- Pull-apply preserves local password_hash (HIGH)

- **Source:** phase-02.md §7.24 -- "Pull payload from server EXCLUDES `password_hash` for all consumers. Local row retains its existing hash."
- **Target test section:** §2.1
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `users_pull_apply_preserves_local_password_hash_byte_for_byte` | Seed a local user `U` with `password_hash='$argon2id$v=19$...$ORIGINAL_HASH'`. Issue `/sync/pull?since=0` (server has no `password_hash` field in the response). Apply the pulled row. SELECT the local user row; assert `password_hash` is BYTE-FOR-BYTE EQUAL to the original. Compare separately: a regression that wrote `NULL` or empty string into `password_hash` on pull-apply would leave the user unable to log in offline. Per §7.24. |

### §12.2 P02-G36 -- Settings seed default-value table parity (MEDIUM)

- **Source:** phase-02.md §1 settings seed default values.
- **Target test section:** §2.1
- **Category:** Missing Integration Test

| Scenario | Asserts |
|-|-|
| `settings_seed_default_values_match_build_spec_table` | Apply migration 002 to a fresh DB. SELECT each of the 10 seeded keys from `settings`; assert the VALUE column matches the §1 table exactly: `dye_cost_iqd=10000`, `report_cost_iqd=10000`, `internal_doctor_pct=30`, `idle_lock_minutes=10`, `arabic_numerals='false'`, `currency_symbol='د.ع'`, `thermal_width='32'`, `thermal_printer_name=''`, `clinic_display_name_en=''`, `clinic_display_name_ar=''`. Per §1 settings seed. |

### §12.3 P02-G37 -- Case-insensitive email login lookup (MEDIUM)

- **Source:** phase-02.md §3 Server `AuthService::login` step 1 + §4 Tauri `User::try_new` email lowercasing.
- **Target test section:** §2.3
- **Category:** Missing Integration Test

| Route | Test | Asserts |
|-|-|-|
| `POST /auth/login` | `login_succeeds_with_mixed_case_email_per_lowercase_lookup` | Seed `User { email: 'test@example.com', password_hash: <argon2(P)> }`. POST `/auth/login { email: 'Test@Example.COM', password: P }`. Assert 200 with valid tokens. POST with `email: 'TEST@EXAMPLE.COM'` -- same result. The server-side lookup `getByEmail(entityId, email.toLowerCase())` MUST normalize before the FK match. Per §3 login step 1 + §4 normalization. |

### §12.4 P02-G38 -- ResetPasswordSchema min-8 parity (LOW)

- **Source:** phase-02.md §3 Tauri `users::reset_password` + §3 Frontend `LoginSchema.password.min(8)`.
- **Target test section:** §1.2 / §2.2
- **Category:** Missing Unit Test

| Module | Test | Asserts |
|-|-|-|
| `src/lib/schemas/user.ts` `ResetPasswordSchema` | `reset_password_schema_enforces_newPassword_min_8` | Assert `ResetPasswordSchema.safeParse({ id: 'u-1', newPassword: 'x' })` returns success: false with the same error shape as `LoginSchema` on a sub-8-char password. Test boundary at 7 chars (fails) and 8 chars (succeeds). |
| `users::reset_password` IPC | `reset_password_ipc_rejects_newPassword_below_8_chars` | IPC call with `newPassword='x'` returns `AppError::Validation`; no audit row written; no password change. Mirrors §1.2 Zod assertion at the IPC boundary. |
