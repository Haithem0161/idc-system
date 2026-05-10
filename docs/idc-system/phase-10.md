# Phase 10: Backup, Operations & Final Verification

**Goal:** Ship the operational layer — sync-server nightly Postgres backup, on-demand client restore, pre-push validation script, CI workflow, tracing PII redaction layer — and run the final verification pass per `planning.md`. Mark the plan implementation-ready.

**Surfaces:** Frontend | Tauri/Rust | Sync Server
**Dependencies:** Phase 9.
**Complexity:** M
**PRD references:** §1.4 (out-of-scope rationale for advanced ops), §10.4 (audit retention), §11.1 (Horizon-1 receipt archive).
**Decisions consumed:** D-024 (PII redaction), D-030 (pre-push validation script).
**Open questions resolved:** Q-002 (backup encryption — defaults to AES-256 dump file unless user overrides).

---

## Section 1: Local Schema Changes (Tauri SQLite)

### Migration `019_backup_state.sql`

```sql
-- Local-only. Records the most-recent successful restore for ops visibility.
CREATE TABLE IF NOT EXISTS backup_state (
  key             TEXT PRIMARY KEY,         -- 'last_restore_at', 'last_restore_source', 'last_check_at'
  value           TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);
```

No new domain entities.

---

## Section 2: Server Schema Changes (Prisma / Postgres)

### `BackupArtifact` (server-only)

```prisma
model BackupArtifact {
  id              String    @id @default(uuid())
  filename        String    @unique
  sizeBytes       BigInt    @map("size_bytes")
  algorithm       String                                          // 'pgdump-aes256-gcm'
  createdAt       DateTime  @default(now()) @map("created_at") @db.Timestamptz
  expiresAt       DateTime? @map("expires_at") @db.Timestamptz    // retention 30 days default
  checksumSha256  String    @map("checksum_sha256")
  uploadedBy      String?   @map("uploaded_by")                   // ops user id; null = automated job
  notes           String?

  @@index([createdAt])
  @@map("backup_artifacts")
}
```

Server-only; not synced; not in TENANT_MODELS.

---

## Section 3: DDD Implementation

### Frontend (React)

#### New routes (admin-only)

| Path | File | Description |
|-|-|-|
| `/admin/backups` | `src/pages/admin/backups.tsx` | List of `backup_artifacts` from the server; on-demand snapshot button. |
| `/admin/restore` | `src/pages/admin/restore.tsx` | Restore wizard: pick artifact → confirm → restore. Destructive — confirm twice with typed ID. |

#### React Query hooks
- `useBackupArtifacts()` — server list.
- `useTriggerSnapshot()` — mutation.
- `useRestore()` — mutation; UI is deliberately friction-heavy.

#### i18n
`admin.json` namespace gains the backup + restore strings (~30 keys).

### Tauri/Rust

#### Domain `backup/`

`src-tauri/src/domains/backup/services/backup_service.rs`:
- `list_artifacts()` — calls `GET /backups`.
- `trigger_snapshot(notes?: String)` — calls `POST /backups`.
- `restore(artifact_id, password)` — downloads + decrypts + applies.

#### Tauri commands

| Command | Args | Returns | Description |
|-|-|-|-|
| `backup_list` | `()` | `Vec<BackupArtifact>` | From server. |
| `backup_trigger_snapshot` | `{ notes?: String }` | `Uuid` | Server runs `pg_dump`; returns artifact id. |
| `backup_restore` | `{ artifact_id: Uuid, decrypt_password: String }` | `RestoreReport` | Destructive; confirms twice in UI. |
| `tracing_redact_check` | `{ candidate: String }` | `String` | Diagnostic: returns the candidate after running the redaction layer. |

4 IPC commands.

### Sync Server (Fastify)

Domain `backup/`. Routes:

| Method | Path | Description |
|-|-|-|
| `GET` | `/backups` | List artifacts (paginated, newest first). Auth: superadmin only. |
| `POST` | `/backups` | Triggers `pg_dump` on the server; encrypts with AES-256-GCM (key from env); writes file to `BACKUP_DIR`; inserts `backup_artifacts` row. |
| `GET` | `/backups/:id/download` | Streams the encrypted dump file. Requires a one-time download token issued by the trigger response. |
| `DELETE` | `/backups/:id` | Removes both the row and the file (admin curation). |

4 routes.

A nightly `node-cron` job (registered in `sync-server/src/app/plugins/cron.ts`) runs the snapshot at 02:00 server time, retains 30 days, deletes older.

---

## Section 4: Business Logic

### Server-side snapshot

Step sequence:
1. Spawn `pg_dump --format=custom --no-owner --no-acl $DATABASE_URL` to a temp file.
2. Compute SHA-256.
3. AES-256-GCM encrypt with key from `BACKUP_ENCRYPTION_KEY` env (32 bytes, base64).
4. Move encrypted file to `$BACKUP_DIR/<timestamp>-<rand>.dump.enc`.
5. Insert `BackupArtifact` row.
6. Return id + a one-time download token (15-min expiry).

### Client-side restore

Step sequence (destructive — confirms twice):
1. UI prompts for the artifact and the decrypt password.
2. Tauri downloads the artifact via `GET /backups/:id/download` using the one-time token.
3. SHA-256 verify.
4. AES-256-GCM decrypt.
5. UI confirms a second time, asking the user to type the artifact's date.
6. Server-mode restore: the desktop calls `POST /backups/:id/apply-on-server` (admin-only); the server runs `pg_restore --clean --if-exists`. Local desktop SQLite is **not** wiped automatically; the user is instructed to clear local app data and re-pull from server, OR the desktop's `MigrationRunner` runs a one-off "local reset" (drop + recreate all tables, then pull).
7. Update `backup_state.last_restore_at`.

### Pre-push validation script

File: `tools/pre-push-check.sh`.

```bash
#!/usr/bin/env bash
set -euo pipefail
echo "==> pnpm lint"
pnpm lint
echo "==> pnpm build"
pnpm build
echo "==> Rust"
(cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
echo "==> Sync server"
(cd sync-server && pnpm lint && pnpm typecheck && pnpm test)
echo "==> All green"
```

Hooked into git's pre-push via `husky` (committed at `.husky/pre-push`). Only runs when `BYPASS_PRE_PUSH != 1` (bypass discouraged; reserve for emergency cherry-picks).

### CI workflow

File: `.github/workflows/ci.yml`. Same commands as the script; runs on PRs to `main`. Caches `~/.cargo` and `~/.pnpm-store`.

### PII redaction layer (Rust)

File: `src-tauri/src/observability/redact.rs`. Implements `tracing_subscriber::Layer`. On every event:
1. Walk fields.
2. For each field whose key is in `REDACTED_KEYS = [password, password_hash, token, refresh_token, patient_name]`, replace value with `"<redacted>"` at `INFO` level and below.
3. Pass through unchanged at `DEBUG` and below ONLY if the `verbose-logs` Cargo feature is enabled.

Wired into the global subscriber registered at app boot.

---

## Section 5: Infrastructure Updates

### TENANT_MODELS additions
None (BackupArtifact is server-only).

### Audit triggers
None (backups don't audit; the act is logged via the standard `tracing` pipeline).

### Local SQLite indexes
None.

### Tauri capabilities
- `fs:scope: $APPDATA/idc-system/restore-cache/**` for the temp-decrypt staging dir.
- No `shell:execute` additions — `pg_restore` runs server-side, not client-side.

### New Tauri plugins
None.

### Sync server: new env vars
- `BACKUP_ENCRYPTION_KEY` (32-byte base64; required).
- `BACKUP_DIR` (default `/var/lib/idc-backups`).
- `BACKUP_RETENTION_DAYS` (default 30).
- `BACKUP_CRON_SCHEDULE` (default `0 2 * * *`).

### Husky
`pnpm dlx husky-init && pnpm install`. `.husky/pre-push` calls the script. `.husky/pre-commit` is empty by default; adopters may add lint-staged later.

### CI workflow file
Committed at `.github/workflows/ci.yml`.

---

## Section 6: Verification

1. **Phase-10 own verification.**
   - Lint / build / test pass on all surfaces.
   - Trigger snapshot via `/admin/backups`; observe `backup_artifacts` row + encrypted file on disk.
   - Restore wizard flow: pick artifact → enter decrypt password → confirm twice → server restores; row counts match pre-snapshot baseline.
   - PII redaction: `cargo test` includes a test that `tracing_redact_check("password=hunter2")` returns `password=<redacted>`.
   - Pre-push hook fires on `git push`; refusing on `cargo clippy` failure.
   - CI run on a PR shows green checks.

2. **Final Verification Pass — Pass V (per `planning.md`).**

   This is the gauntlet that closes the plan. Output: `docs/idc-system/PHASES-1-10-VERIFICATION.md` with YAML frontmatter.

   Audit ~25 representative items mixing CRITICAL / HIGH / MEDIUM / LOW. Each item must verify:
   - Complete local SQL schema (if local).
   - Complete Prisma schema (if server).
   - Route or IPC command table entry.
   - Service method signature.
   - Business logic description.
   - Sync rule declared.

   Sample audit rows (non-exhaustive — full set in the verification report):

   | # | Severity | PRD ref | Item | Verifies |
   |-|-|-|-|-|
   | 1 | CRITICAL | §6.1.1 | `users` table | P1 §1; P2 §2 |
   | 2 | CRITICAL | §6.1.10 | `visits` snapshot completeness | P5 §1, §4 (lock workflow); CHECK constraint enforced |
   | 3 | CRITICAL | §8.1 | Lock workflow — operator eligibility | P5 §4 step 2 |
   | 4 | CRITICAL | §8.2 | Void workflow — inventory rollback | P8 §4 step 6 |
   | 5 | HIGH | §4.4 | Inventory consumption ledger materialization | P6 §4 `recompute_quantity_on_hand` |
   | 6 | HIGH | §10.6 | `name_ar` required, `name_en` optional everywhere | P3 entities |
   | 7 | HIGH | §6.1.10 invariant 5 | `dye = 1` requires `check_types.dye_supported = 1` | P3 invariants |
   | 8 | HIGH | §10.8 | Sync pill states `idle/pushing/pulling/offline/error` | P1 §3 SyncPill |
   | 9 | HIGH | §5.5 | Offline-login Argon2id cache | P1 AuthService + P2 PasswordService |
   | 10 | MEDIUM | §7.1.5 | Operator clock-in / out | P4 §4 |
   | 11 | MEDIUM | §7.2.5 | Daily Close artifact | P7 §4 |
   | 12 | MEDIUM | §7.5 | Audit page filters | P9 §3 |
   | 13 | MEDIUM | §8.5 | Pricing change banner on drafts | P3 settings + P5 visit form |
   | 14 | LOW | §10.6 | `arabic_numerals` setting toggle | P3 settings |
   | 15 | LOW | §3.3 | macOS-System-Settings sub-nav | P3 admin shell |

   For each gap found, file a Section 7.x in the originating phase file with severity + category + remediation. Re-run Pass V until clean.

3. **Roadmap + Status update.**
   - `roadmap.md` Section 8 (Gap Analysis Additions): final pass count.
   - `status.md` row 10 → `Completed`. All counters reflect final tallies.

### What this phase does NOT verify (scope boundaries)
- The Horizon-1 features in PRD §11.1 (patient dedupe, scheduler, SMS, refunds, server-signed daily close, COGS, bulk import) — these are explicitly out of v1.
- Multi-tenant cutover (Horizon 2).
- Mobile companion (Horizon 2).
- Clinical reporting / DICOM (Horizon 2/3).

### Summary update
Bump `status.md` row 10 to `Completed`. Cumulative counters reflect the entire plan: **24 local objects** (15 PRD entities + 6 local-only `outbox`, `sync_state`, `_migrations`, `visit_daily_rollup`, `sync_conflicts`, `backup_state` + 3 FTS5 virtual tables `doctors_fts`, `patients_fts`, `audit_log_fts`); **18 server Prisma models** (15 PRD + 3 server-only `RefreshToken`, `Session`, `BackupArtifact`); **~91 IPC commands**; **~75 sync-server HTTP routes** (incl. 1 stub for `/auth/mfa`); **~25 services across surfaces**. Add the backup/restore routes + hooks to `frontend-summary.md`.

When `PHASES-1-10-VERIFICATION.md` reports `status: complete` and `gaps: []`, the plan is implementation-ready and v0.1.0 is shippable.

---

## Section 7: PRD Gap Additions

### 7.1 Success-metric instrumentation — MEDIUM
**Gap:** PRD §1.3 lists six quantitative success metrics (visit lock p95 < 30s, sync replication p95 < 5s, audit coverage 100%, accounting reconciliation diff = 0 IQD, i18n coverage = 100%, RTL layout regression count = 0). Phase 10 verification doesn't include the instrumentation that lets ops measure them.
**Category:** Missing Verification.
**Remediation:** Land in Phase 10:
- **Visit lock p95.** Tauri side emits `tracing::info!(target: "metrics", visit_lock_ms = elapsed)` on every successful lock; the daily backup script ships the log to a `metrics/` Postgres table the accountant can query. Verification: lock 100 visits in dev; p95 < 30s.
- **Sync replication p95.** Server stamps `pulledAt` per row; client pulls record `(updatedAt -> applyAt)` delta; instrumented same way. Verification: edit reference data on device A; observe device B p95 < 5s after reconnect.
- **Audit coverage.** Static scan: every domain service write is wrapped in `with_audit`. The Phase-9 lint rule (referenced in P1 §6 verification 7) is the enforcement mechanism. Land the lint rule in Phase 10 if it didn't make Phase 9.
- **Reconciliation diff = 0.** Daily-close artifact computes `revenue - sum(cuts)` and `quantity_on_hand_recompute - quantity_on_hand_materialized`; both must be 0. Verification asserts this on the seeded test data.
- **i18n coverage = 100%.** `i18next-parser` scan in CI; fails build on any unkeyed string.
- **RTL regression count = 0.** Screenshot-diff test (Playwright + visual regression) for the top 10 screens; baseline locked at end of P10.

### 7.2 Receipts directory housekeeping — LOW
**Gap:** Receipts persist forever under `$APPDATA/idc-system/receipts/<YYYY>/<MM>/`. No retention policy; disk fills over time.
**Category:** Missing Operation.
**Remediation:** Phase 10 ships a small janitor task:
- Once a week, walk `$APPDATA/idc-system/receipts/`; for any file whose `<visit-id>` references a `voided` or `deleted` visit, leave it (audit needs); for any file whose owning visit's `created_at` is more than 365 days old AND that visit is `voided`, the file is moved to `archive/` (not deleted).
- Hard rotation deferred to Horizon-1 (the central receipt archive).

### 7.3 Backup/restore on the local SQLite — LOW
**Gap:** Phase 10 covers server-side Postgres backup but the local SQLite per-device file isn't backed up at all. If a device dies before its outbox drains, the unsynced rows are lost.
**Category:** Missing Operation.
**Remediation:** Document the trade-off in Phase 10:
- Local SQLite **is** backed up indirectly: every committed mutation is in the outbox and rides to the server within seconds. Once the outbox is empty, the local file is reproducible from a server pull.
- Critical caveat: the receipts/ folder is local-only and is NOT in the server backup (in v1). Recommended: encourage staff to keep paper receipts as a secondary record. Horizon 1 ships the central receipt archive (PRD §11.1).
- For paranoid setups: a `backup_local_db()` IPC zips `$APPDATA/idc-system/idc.db*` (WAL files included) to a user-picked path; not exposed in UI by default.
