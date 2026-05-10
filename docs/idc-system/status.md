# IDC System — Implementation Status

**Last updated:** 2026-05-10 (Pass 0 — phase files pending)
**Plan:** `docs/idc-system/roadmap.md` · `docs/idc-system/research.md`
**Spec:** `docs/idc-system/PRD-V0.1.0.md` (V0.1.1)

## 1. Phase Status Table

| # | Phase | Surfaces | Status | Started | Completed | Local Tables Added | Server Models Added | IPC Commands Added | Routes Added | Services Added |
|-|-|-|-|-|-|-|-|-|-|-|
| 1 | Tauri Spine | Frontend, Tauri/Rust | Not Started | — | — | 0 / 5 | 0 / 0 | 0 / ~10 | 0 / 0 | 0 / 8 |
| 2 | Sync Server Foundation | Sync Server | Not Started | — | — | 0 / 0 | 0 / 4 | 0 / 0 | 0 / 8 | 0 / 7 |
| 3 | Reference Data & Admin CRUD | All | Not Started | — | — | 0 / 8 (+2 FTS) | 0 / 8 | 0 / ~24 | 0 / ~16 | 0 / 5 |
| 4 | Operator Shifts | All | Not Started | — | — | 0 / 1 | 0 / 1 | 0 / 4 | 0 / 4 | 0 / 1 |
| 5 | Reception + Lock | All | Not Started | — | — | 0 / 1 | 0 / 1 | 0 / 8 | 0 / 4 | 0 / 4 |
| 6 | Inventory + Auto-Decrement | All | Not Started | — | — | 0 / 3 | 0 / 3 | 0 / 6 | 0 / 6 | 0 / 3 |
| 7 | Accounting + Daily Close | All | Not Started | — | — | 0 / 0 | 0 / 0 | 0 / 5 | 0 / 4 | 0 / 2 |
| 8 | Void Workflow | All | Not Started | — | — | 0 / 0 | 0 / 0 | 0 / 1 | 0 / 1 | 0 / 1 |
| 9 | Audit + FTS + Vacuum | All | Not Started | — | — | 0 / 0 | 0 / 0 | 0 / 2 | 0 / 1 | 0 / 2 |
| 10 | Backup + Ops + Verify | All | Not Started | — | — | 0 / 0 | 0 / 0 | 0 / 2 | 0 / 2 | 0 / 2 |

**Status enum:** `Not Started` · `In Progress` · `Blocked` · `Completed` · `Verified`.

A phase is `Completed` when its Verification section passes. It becomes `Verified` only after the corresponding gap-analysis pass logs zero gaps for it.

## 2. Cumulative Totals

| Metric | Before | Current | Target | Source |
|-|-|-|-|-|
| Local SQLite tables (PRD entities) | 0 | 0 | 15 | PRD §6.1 |
| Local-only tables | 0 | 0 | 6 (`outbox`, `sync_state`, `_migrations`, `visit_daily_rollup`, `sync_conflicts`, `backup_state`) | offline-first.md + roadmap |
| FTS5 virtual tables | 0 | 0 | 3 (`doctors_fts`, `patients_fts`, `audit_log_fts`) | research D-027 + P9 |
| Server Prisma models (PRD entities) | 0 | 0 | 15 | PRD §6.1 |
| Server-only Prisma models | 0 | 0 | 3 (`RefreshToken`, `Session`, `BackupArtifact`) | research D-018 + P10 |
| Routed pages | 0 | 0 | ~38 (29 PRD §3.1 primary + ~9 auxiliary) | PRD §3.2 + phases |
| Tauri IPC commands | 1 stub (`_example`) | 0 | ~91 (P1=10, P3=46, P4=5, P5=9, P6=10, P7=6, P8=1, P9=6, P10=4) | per-phase summaries |
| Sync-server HTTP routes | 1 stub (`/`) | 0 | ~75 (auth 4 + sync 3 + healthz 1 + ref-data 37 + shifts 5 + visits 4 + inventory 11 + reports 4 + audit 1 + backup 4 + MFA stub 1) | per-phase summaries |
| Domain services across surfaces | 0 | 0 | ~25 | roadmap §"Business Engine Inventory" |
| i18n namespaces | 2 stubs | 0 | 9 (`common`, `auth`, `reception`, `accounting`, `inventory`, `admin`, `audit`, `errors`, `receipts`) | PRD §10.6 |
| Shadcn components installed | 0 | 0 | 16 baseline + per-need | research D-022 |
| TENANT_MODELS list size | 0 | 0 | 15 | roadmap §"New Server Entities" |
| Pre-push validation script | absent | absent | present (P10) | research D-030 |

The "Before" column is the repo state at the start of Phase 1. "Current" tracks live progress. "Target" is the v0.1.0 ship target.

## 3. Gap Analysis Summary

### Pass 0 — pre-write
- **Date:** 2026-05-10
- **Status:** Complete (n/a phase files now exist).

### Pass 1 — initial
- **Date:** 2026-05-10
- **Status:** Complete.
- **Gap count:** 14.
- **Severity distribution:** 0 CRITICAL · 0 HIGH · 4 MEDIUM · 10 LOW.
- **Category distribution:** Missing Logic 6 · Missing Integration 4 · Missing Verification 2 · Missing Setup 2.
- **Distribution by phase:** P1=2, P2=3, P3=3, P4=1, P5=3, P6=2, P7=3, P8=0, P9=0, P10=3.
- All gaps filed as Section 7.x entries in the originating phase files with severity, category, and remediation steps.

### Pass 2 — iterative
- **Date:** 2026-05-10
- **Status:** Complete.
- **Gap count:** 0 (no new gaps after Pass-1 remediations were folded in).

### Pass V — initial final verification (author-led)
- **Date:** 2026-05-10
- **Status:** Reported `gaps: []` but Pass-V+ disproved this. The author was biased toward seeing the plan work.

### Pass V+ — independent verification (external-style audit)
- **Date:** 2026-05-10
- **Method:** two parallel verification agents (schema parity + workflow/UX coverage + counter math).
- **Status:** Complete; 16 real gaps surfaced, all remediated in-place.
- **Severity distribution:** 2 CRITICAL · 2 HIGH · 8 MEDIUM · 4 LOW.
- **Top fixes:**
  - P6 `inventory_items` was single-language `name`+`notes`; restored bilingual `name_ar`/`name_en` per PRD §6.1.12.
  - P5 `Visit` Prisma referenced `inventoryAdjustments` that didn't exist yet; relation deferred to P6 §2 "Existing models updated".
  - P4 `notes` → `note` per PRD §6.1.8.
  - Settings seed gained the missing `clinic_display_name_en`.
  - Counter math reconciled: 24 local objects (was 18), 18 server models (was 17), ~91 IPC (was ~55), ~75 server routes (was ~12), ~38 pages (was 29).
  - Pass-V+ entries added to P1 §7.3 (in-memory conflict queue), P4 §7.0 (operator shifts page columns/states), P9 §7.1-§7.3 (`with_audit` lint, CHECK widening, in-memory→table migration).
- **Output:** [`PHASES-1-10-VERIFICATION.md`](./PHASES-1-10-VERIFICATION.md) rewritten as the Pass-V+ report.

### Pass V++ — post-remediation re-audit
- **Date:** pending.
- **Status:** Recommended before declaring the plan implementation-ready: re-run the two verification agents to confirm zero remaining gaps.

## 4. Blockers & Notes

### Active blockers
None.

### Notes

- **Strict sequential delivery** (research D-018). Phase 2 cannot start until Phase 1 is `Completed`. The dependency graph is linear; do not re-order without re-running gap analysis.
- **Auto-pull rule for Prisma changes.** Per `docker.md`, after editing `sync-server/prisma/schema.prisma`, restart the `sync-server` container (`docker compose restart sync-server`). After adding a Postgres extension or `init-custom-sql.sql` change, run the SQL with `docker exec`.
- **Pre-push validation** (research D-030) runs locally before every push from Phase 1 onward, even though the script `tools/pre-push-check.sh` doesn't ship until Phase 10. The composite command is documented in `dev-workflow.md` and reproduced at the bottom of every phase file's Verification section.
- **Frontend summary cadence.** Per `planning.md`, `frontend-summary.md` is updated after each phase, never batched. Reviewers reject any PR that completes a phase without bumping that file.
- **No Claude authorship** on commits (CLAUDE.md). Commits appear as solely human-made; no `Co-Authored-By: Claude` lines, no Anthropic emails.
- **Context7 first** for every library used (CLAUDE.md). Phase files do not commit to Tauri / sqlx / Prisma / Fastify / TypeBox APIs without a recent Context7 lookup.

## 5. Change Log

| Date | Change |
|-|-|
| 2026-05-10 | File created. All 10 phases at `Not Started`. Plan locked per `roadmap.md` and decisions in `research.md`. |
| 2026-05-10 | Pass 1 gap analysis: 14 gaps filed (4 MEDIUM, 10 LOW); each phase's Section 7 records remediations. |
| 2026-05-10 | Pass 2 sweep: 0 new gaps. |
| 2026-05-10 | Pass V (author-led) `PHASES-1-10-VERIFICATION.md` written: `status: complete`, `score: 25/25`, `gaps: []`. |
| 2026-05-10 | Pass V+ (independent agents) found 16 real gaps including 2 CRITICAL — proves Pass V was biased. Remediated in-place: P6 inventory bilingual restored; P5 Visit Prisma deferred relation cleaned up; P6 §2 explicit deltas added; P4 `note` singular; P3 seed completed; counter math fixed across roadmap/status/P10. Verification report rewritten to reflect Pass-V+. Plan **NOT YET** implementation-ready until Pass V++ confirms 0 remaining gaps. |
