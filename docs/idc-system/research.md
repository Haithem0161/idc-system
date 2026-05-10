# IDC System — Research & Decisions Log

This file is the canonical record of the design decisions that shape the IDC plan. Each row dates the decision, the conclusion, and the rationale (what alternative was rejected and why). Phase files cite this log instead of re-arguing.

## Domain Snapshot

**The center.** المجمع العراقي التخصصي — a single-site Iraqi medical imaging center. Two minimum workstations (reception, accounting). Network is opportunistic. Iraqi staff are Arabic-first.

**The work.** Receptionist registers patients, picks the check the patient is here for, captures any external doctor, dye/report flags, picks the operator at lock, prints a receipt. Accountant reads back the day's records as financial reports. Inventory auto-decrements on dye consumption. Superadmin manages all reference data.

**The constraint.** The center owns the box. They are not Business OS customers (yet). The desktop app is the source of truth; the sync server exists for cross-device replication and backup, not as a primary API.

## Decisions Log

| Date | # | Decision | Rationale (alternative rejected) |
|-|-|-|-|
| 2026-05-07 | D-001 | One `doctors` row per person; pricing/cut on `doctor_check_pricing` join (`doctor_id`, `check_type_id`, `check_subtype_id?`, `price_override_iqd?`, `cut_kind`, `cut_value`). | Same person appears across check types. Putting price+cut on the doctor row would either duplicate the doctor record or force per-check fields onto it. Junction table is normalised and reports cleanly. |
| 2026-05-07 | D-002 | Receptionist picks operator at lock from currently-clocked-in operators whose specialty covers the visit's check type. Lock fails with `LockError::NoQualifiedOperator` if the qualifying set is empty. | Auto-attribution by round-robin or by-name silently mis-credits payroll. Explicit pick is auditable and refutable. |
| 2026-05-07 | D-003 | Operators are records only — no login. Receptionist clocks them in/out from a Reception sub-tab. | Operator login adds an entire auth surface for staff who only need to be paid, and matches the user's stated workflow. Re-evaluated at Horizon 2 (PRD §11.2). |
| 2026-05-07 | D-004 | Sync server v1 = full bidirectional replication within one center + nightly Postgres backup. Single tenant, but `entity_id` column kept on every syncable table for forward compat. | Multi-tenant SaaS is overkill for one center; building it lightly today (just the column + middleware shape) avoids a painful migration if a second center signs on. |
| 2026-05-07 | D-005 | Empty doctor field = "house" (in-house). The center keeps `settings.internal_doctor_pct` percent of the price. | A "house" pseudo-doctor row would muddle reports; absence-as-meaning is simpler and matches the user's spec verbatim. |
| 2026-05-07 | D-006 | Operator base cut = single `base_cut_per_check_iqd` per operator. Doubled when `dye = 1`. Same number regardless of check type. | Per-(operator, check_type) rates were considered (more flexible) but the user explicitly chose flat per-operator. Re-evaluated at Horizon 2. |
| 2026-05-07 | D-007 | `check_types.has_subtypes` flag. If `1`: `base_price_iqd` MUST be NULL; subtypes carry the price. If `0`: `base_price_iqd` is required; no subtypes allowed. SQLite `CHECK` constraint enforces XOR. | Allowing both at once produces "which price wins?" ambiguity. Hard XOR removes it. |
| 2026-05-07 | D-008 | Inventory: items + per-(check_type OR subtype) consumption map with quantities + on-dye-only flag. Auto-decrement on visit lock as `inventory_adjustments` rows. Manual receive/writeoff/correction. Materialized `quantity_on_hand` recomputed in the same transaction. Low-stock badge in sidebar. | Manual decrement-after-the-fact is forgotten. Auto-decrement at lock matches user explicit choice. Ledger pattern (rows, not field edits) preserves audit. |
| 2026-05-07 | D-009 | Roles v1: **Superadmin**, **Receptionist**, **Accountant**. No `Operator` role. | Operator login deferred (D-003). Three roles cover every action surface in v1. |
| 2026-05-07 | D-010 | Void: superadmin-only. Soft-deletes the visit, reverses derived cuts, writes offsetting `inventory_adjustments` rows referencing the same `visit_id`, audit `void` row. No separate refund record. | A separate refund ledger is more accurate but adds a model. Void as full reversal is the simplest correct path; partial refund moves to Horizon 1 (PRD §11.1). |
| 2026-05-07 | D-011 | Receipt printed at lock: A5 PDF + thermal text alternative. Bilingual; layout mirrors for RTL. | Auto-printing by default rejected — jammed-printer-driven lock failures are worse than a manual confirm. Print dialog offers PDF; thermal printer driven by separate command. |
| 2026-05-07 | D-012 | Bilingual UI: Arabic default + English toggle. RTL applied via `<html dir="rtl">` and Tailwind v4 logical-property utilities. App boots in `ar` on first launch / cleared storage. Language toggle persists per-device in `tauri-plugin-store` (NOT synced). 100% of UI strings live in `ar.json` / `en.json`. | Iraqi staff are Arabic-first. Detecting OS locale on first launch was rejected because some devices ship with English defaults — Arabic forced by the app. |
| 2026-05-10 | D-013 | **Per-check Reception** (PRD V0.1.1). The Reception landing page is a Checks Grid; each check has its own workspace; a visit is exactly one check. `visit_lines` is removed; check fields (`check_type_id`, `check_subtype_id`, `doctor_id`, `operator_id`, `dye`, `report`) are inlined onto `visits` along with all `*_snapshot_iqd` columns. | The original multi-line model assumed multiple checks per visit, which doesn't match Iraqi medical-center workflow (one patient = one check). Multi-check bookings (rare) move to Horizon 1. |
| 2026-05-10 | D-014 | IDs are **client-generated UUID v7** for every syncable entity. The sync server validates ID format and rejects collisions across tenants but never generates IDs. | Server-generated IDs require a network round-trip on every create, which breaks offline-first. UUID v7 carries time ordering, so locally-generated IDs sort correctly without a server. |
| 2026-05-10 | D-015 | Local audit retention 90 days; server retention indefinite. Daily vacuum job soft-deletes audit rows older than 90d that have `dirty = 0`. Server-backed query when local window is exceeded. | Indefinite local retention bloats SQLite. 90 days covers daily-close reconciliation and any near-term audit. The server is the system of record for older queries. |
| 2026-05-10 | D-016 | Sync conflict policies: `users` LWW; `audit_log` additive-only; `visits` manual; `settings` manual; `operator_shifts`, `inventory_adjustments` additive-only; everything else (catalog data) LWW. | Financial records (`visits`) and global tunables (`settings`) cannot silently merge — they go to a human resolver. Append-only ledgers (`audit_log`, `operator_shifts`, `inventory_adjustments`) never need conflict resolution. Catalog data (rare edits, admin-only) is LWW. |
| 2026-05-10 | D-017 | Money math at lock (PRD §6.1.10): `total = price + dye_cost + report_cost`. `price` resolution priority: `doctor_check_pricing.price_override_iqd` (if doctor set + override exists) → `check_subtypes.price_iqd` (if subtype) → `check_types.base_price_iqd`. Doctor cut basis = `price` (excludes dye/report). Operator cut = `operators.base_cut_per_check_iqd * (dye ? 2 : 1)`. House cut = `floor(price * settings.internal_doctor_pct / 100)`. All values stored as `*_snapshot_iqd` columns at lock; never recomputed for locked visits. | Lock-then-snapshot prevents retroactive admin-edit havoc on accounting. Storing the math result, not the formula, makes accounting reconciliation trivial. |
| 2026-05-10 | D-018 | **Strict sequential phase delivery** (chosen by user 2026-05-10). Phase 2 (sync server) does NOT start until Phase 1 (Tauri spine) is complete, even though the surfaces are disjoint. | Two-track parallel delivery was offered. User chose sequential to keep dependency graph linear and to allow a single contributor to own the full app. |
| 2026-05-10 | D-019 | Receipt persistence: `$APPDATA/idc-system/receipts/<YYYY>/<MM>/<visit-id>.pdf` and `…/thermal/<visit-id>.txt`. No upload to a Document Center service in v1. Backup is via the sync-server nightly snapshot. | A central receipt archive (Horizon 1) requires file-storage infrastructure that doesn't exist yet. Local persistence + nightly DB backup covers the "patient asks for a duplicate" flow. |
| 2026-05-10 | D-020 | Patient identity v1: quadripartite Arabic name only. Returning patient = a new `visits` row referencing a new `patients` row (no dedupe). The `patients` table is referenced from `visits` so future identity work can dedupe without a schema migration. | Phone capture + dedupe is Horizon 1 (PRD §11.1). v1 keeps the name-only model the user spec'd. |
| 2026-05-10 | D-021 | Multi-user on one device: single SQLite file with logical scoping by `actor_user_id` on every audited write. Login swap mid-day is supported by rotating the in-memory `UserContext`. No per-user database. | Per-user SQLite files require attaching/detaching mid-day, multiplying conflict surface. One DB + audit-by-actor is simpler and the constraints already enforce integrity. |
| 2026-05-10 | D-022 | Shadcn baseline installed in Phase 1: `button`, `card`, `input`, `table`, `tabs`, `dialog`, `badge`, `toast`, `skeleton`, `form`, `select`, `radio`, `checkbox`, `alert`, `separator`, `scroll-area`. Each component verified for RTL behavior at install. | Bulk-installing all of shadcn pollutes `src/components/ui/` with components that are never used. Per-need install is the rule for later phases. |
| 2026-05-10 | D-023 | Tauri capabilities (Phase 1): `fs:scope: $APPDATA/idc-system/receipts/**`, `fs:scope: $APPDATA/idc-system/logs/**`, `dialog:save`, `dialog:open`, `store:default`, `stronghold:default`, `os:default`. NO bare `http` capability — sync HTTP goes through `reqwest` linked into the Rust backend. | Granting `http` to the webview is a CSP nightmare. Funneling all network through Rust gives one place to log + retry + auth. |
| 2026-05-10 | D-024 | Logging: `tracing` with a JSON file appender at `$APPDATA/idc-system/logs/`. Custom layer redacts PII fields (patient name, password, token) from `info!`-level log lines; full payloads only at `debug!` behind a feature flag. | Capturing PII at `info!` level violates basic privacy hygiene. Redacting at the layer level keeps every call site clean. |
| 2026-05-10 | D-025 | Receptionist permissions on inventory: `receive` and `writeoff` adjustments allowed; `count_correction` blocked (superadmin-only). | Count corrections re-base reality. Limiting them to one role gives a single point of accountability when stocks drift. |
| 2026-05-10 | D-026 | Daily Close v1 = on-demand local artifact (printable PDF). The Horizon-1 plan signs the daily close with the server's RS256 key as a separate `daily_close` entity. | Server signing requires a round-trip and a model. v1 ships the workflow without the artifact, and the user can re-run the close any time without state corruption. |
| 2026-05-10 | D-027 | Patient and doctor name search uses **SQLite FTS5** (`patients_fts`, `doctors_fts`). Inventory items, check types, subtypes use `LIKE`-prefix queries (cardinality is small). | FTS5 setup costs an index per field; not worth it for a 50-row check_types table. |
| 2026-05-10 | D-028 | Lock-screen idle timeout: 10 minutes default, configurable via `settings.idle_lock_minutes`. Re-auth works offline using the cached Argon2id hash. | A hard-coded 10 minutes is cheaper, but Iraqi clinics vary in flow; making it a setting leaves room for per-center tuning. |
| 2026-05-10 | D-029 | Snapshot policy on price changes: locked visits NEVER recompute. Drafts show a "prices updated — recompute?" banner; the receptionist confirms. | Auto-recomputing drafts is surprising; never-recomputing both is rigid. Banner-then-confirm splits the difference. |
| 2026-05-10 | D-030 | Pre-push validation is mandatory and matches CI: `pnpm lint`, `pnpm build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, plus sync-server lint/build/tests once that surface exists. Scripted at `tools/pre-push-check.sh` (Phase 10 lands the script; the rule applies from Phase 1). | "Run it in CI only" wastes a 7-minute CI cycle on local-fixable mistakes. The rule is to run locally first; the script makes it one command. |
| 2026-05-10 | D-031 | **MFA is out of scope for v1.** Threat model: single-tenant + physical access to two PCs in one center. The auth.md `/auth/mfa` route exists but is implemented as a 501 stub in Phase 2 to keep the surface forward-compatible without committing to MFA UI in v1. | Adding MFA to v1 doubles the auth surface for a threat the deployment topology already mitigates. Reconsidered at Horizon 2 alongside true multi-tenant. |

## Open Questions (carried forward)

These are not blockers but should be answered by the time their phase lands:

| Q-# | Question | Latest land phase | Notes |
|-|-|-|-|
| Q-001 | How should the receipt thermal-text template wrap multi-word Arabic check names without breaking ligatures? | Phase 5 | Test with `tauri-plugin-shell` printer driver during P5 verification. |
| Q-002 | Does the daily Postgres `pg_dump` job need encryption-at-rest beyond what Postgres provides? | Phase 10 | Defer to user decision in P10; pin a default of "yes, AES-256 dump file" if no answer by then. |
| Q-003 | Is `settings.arabic_numerals = true` ever the default for Arabic clinics, or always Western digits per Iraqi invoicing convention? | Phase 3 | Default `false` per PRD §10.6; revisit if a customer disagrees. |
| Q-004 | Is offline-login MFA needed (PRD §5.5 references RS256 + cached creds; MFA is mentioned in `auth.md`)? | Phase 1 | Default to "no MFA in v1" — single-tenant + physical-access threat model. Document explicitly. |
| Q-005 | Do voided receipts re-print with a watermark or a separate "VOIDED" banner? | Phase 8 | Default to a top-banner "ملغي / VOIDED" plus a watermark behind the line items. |

## References

These are the rule files and external docs the plan cites. Phase files paste-quote from them where prescriptive.

### Internal rules (`.claude/rules/`)
- [`offline-first.md`](/home/haithem/Projects/idc-system/.claude/rules/offline-first.md) — Standard sync columns, outbox shape, conflict-resolution policies, sync engine lifecycle.
- [`auth.md`](/home/haithem/Projects/idc-system/.claude/rules/auth.md) — RS256 JWT, refresh rotation, Argon2id offline-login cache, lock screen.
- [`sync-server.md`](/home/haithem/Projects/idc-system/.claude/rules/sync-server.md) — Fastify plugin layout, TypeBox conventions, TENANT_MODELS, Swagger pattern.
- [`ddd.md`](/home/haithem/Projects/idc-system/.claude/rules/ddd.md) — Domain/Infrastructure/Presentation layering across surfaces.
- [`tauri.md`](/home/haithem/Projects/idc-system/.claude/rules/tauri.md) + [`rust.md`](/home/haithem/Projects/idc-system/.claude/rules/rust.md) — Capabilities, AppState, tracing, thiserror, sqlx.
- [`frontend.md`](/home/haithem/Projects/idc-system/.claude/rules/frontend.md) — React 19, Vite, Tailwind v4, shadcn, i18n, RTL, Zustand, React Query.
- [`docker.md`](/home/haithem/Projects/idc-system/.claude/rules/docker.md) — Compose patterns, Prisma auto-sync.
- [`dev-workflow.md`](/home/haithem/Projects/idc-system/.claude/rules/dev-workflow.md) — 10-step development loop, pre-push validation, package install rules.
- [`planning.md`](/home/haithem/Projects/idc-system/.claude/rules/planning.md) — Plan file structure, phase template, gap analysis methodology (this file's authority).
- [`prd-writing.md`](/home/haithem/Projects/idc-system/.claude/rules/prd-writing.md) — PRD authority for the spec this plan implements.

### Spec
- [`PRD-V0.1.0.md`](./PRD-V0.1.0.md) — Active version V0.1.1 (2026-05-10). The product requirements; this plan delivers it.

### External
- Tauri v2 docs (via Context7 `mcp__context7__query-docs` per `dev-workflow.md`).
- React Router v7 docs (Context7).
- TanStack Query v5 docs (Context7).
- Prisma + Postgres docs (Context7).
- Fastify v5 + plugins (Context7).
- TypeBox docs (Context7).
- react-i18next (Context7).
- Argon2 / argon2-rs crate docs (Context7).
- UUID v7 RFC 9562.
- SQLite FTS5 docs.
