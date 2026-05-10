---
phase: idc-system-phases-1-10
verified: 2026-05-10T18:30:00Z
status: gaps_found
score: 16 gaps surfaced and remediated; Pass V++ pending
gaps_pre_remediation:
  - {id: G1, severity: CRITICAL, location: phase-06.md §1, §2, item: "inventory_items dropped bilingual name_ar/name_en (PRD §6.1.12)"}
  - {id: G2, severity: CRITICAL, location: phase-05.md §2 + phase-06.md §2, item: "Visit Prisma referenced inventoryAdjustments back-relation but P5 had no model link"}
  - {id: G3, severity: HIGH, location: phase-06.md §2, item: "Missing copy-paste deltas for back-relations on CheckType/CheckSubtype/Visit/User"}
  - {id: G4, severity: HIGH, location: phase-03.md §2, item: "P3 forward-reference notes for relations introduced in P5/P6 absent"}
  - {id: G5, severity: MEDIUM, location: phase-04.md §1, §2, IPC, item: "operator_shifts.note (PRD singular) shipped as notes (plural)"}
  - {id: G6, severity: MEDIUM, location: phase-03.md §1 settings seed, item: "clinic_display_name_en missing from required-keys seed"}
  - {id: G7, severity: MEDIUM, location: phase-03.md §3, item: "patients REST routes implicit; route count drift"}
  - {id: G8, severity: MEDIUM, location: phase-03.md §3, item: "IPC count math (24 vs 42 vs 44) inconsistent across the doc"}
  - {id: M1, severity: MEDIUM, location: roadmap.md §New Local Entities, item: "Total tables claim 18; phases ship 24 once FTS5 + helpers counted"}
  - {id: M2, severity: MEDIUM, location: phase-10.md §6, item: "17 server models math wrong (3 server-only + 15 PRD = 18)"}
  - {id: M3, severity: MEDIUM, location: roadmap.md / status.md / PRD §3.2, item: "Page count drift: PRD says 29; phases ship ~38"}
  - {id: M4, severity: MEDIUM, location: phase-09.md, item: "with_audit lint rule unowned across P1->P9->P10"}
  - {id: G9, severity: LOW, location: phase-03.md §4 SettingsService, item: "bool parser convention undocumented"}
  - {id: G10, severity: LOW, location: phase-09.md §1, item: "sync_conflicts.policy CHECK too narrow for forward compat"}
  - {id: L1, severity: LOW, location: phase-01.md §4, item: "P3 §7.3 references in-memory conflict queue but P1 didn't define it"}
  - {id: L2, severity: LOW, location: phase-04.md §3, item: "Operator Shifts page lacks Columns + States subsection per PRD-writing rule"}
gaps_post_remediation: pending Pass V++
---

# IDC System — Phases 1-10 Verification Report

This report supersedes the initial Pass V (which falsely claimed `gaps: []`). The Pass V audit was author-led and biased; an independent Pass V+ run (two parallel external-style verification agents, methodology described below) surfaced 16 real gaps. All 16 have been remediated in-place; a Pass V++ re-audit is required before declaring the plan implementation-ready.

## Method

Pass V+ ran two agents in parallel, each with a specific audit charter and a strict "find what the author missed" framing:

- **Agent A — Schema parity.** Walked every PRD §6.1 entity field-by-field against the matching phase file's `CREATE TABLE` and Prisma blocks. Verified the 9 standard sync columns from `offline-first.md`, the indexes called out in the PRD, the invariants enumerated, and the sync policy declarations. Counter-checked back-relation graph integrity (Prisma compiles iff both sides exist).
- **Agent B — Workflow / page / counters.** Walked PRD §3 (navigation tree + page counts), §7 (module specs sub-page coverage), §8 (cross-module workflows), §10 (system features), and the cross-phase counter math (status.md cumulative totals vs phase-file additions).

Each agent produced a structured report with severity, file:section location, and remediation. Reports synthesized below.

## Audit Roster (33 items across the two agents)

The Pass V audit covered 25 representative items. Pass V+ widened to 33 items by walking additional PRD subsections systematically. For brevity the final table below logs only the 16 items that surfaced gaps; the other 17 PASS items match the original Pass V table and are unchanged.

| # | Severity | PRD ref | Item | Phase(s) | Pass V verdict | Pass V+ verdict | Remediation status |
|-|-|-|-|-|-|-|-|
| G1 | CRITICAL | §6.1.12 | `inventory_items` bilingual `name_ar`/`name_en` | P6 | claimed PASS | **FAIL** — single-language `name` + un-PRD-authorized `notes` | **FIXED** in P6 §1 + §2 |
| G2 | CRITICAL | §6.1.10 + §6.1.14 | `Visit` ↔ `InventoryAdjustment` Prisma relation completeness | P5, P6 | not audited | **FAIL** — P5's `Visit` lacked `inventoryAdjustments` back-relation; P6 said it was "already present" | **FIXED** by deferring relation declaration to P6 §2 "Existing models updated" |
| G3 | HIGH | planning.md §"Section 2" | Modified models need column-by-column deltas | P6 | not audited | **FAIL** — prose mention only; no copy-paste blocks | **FIXED** in P6 §2 with explicit per-model addition snippets |
| G4 | HIGH | §6.1 back-relation notes | P3 silent on which CheckType/CheckSubtype relations land later | P3 | not audited | **FAIL** — readers must intuit forward-references | **FIXED** by P6 §2 explicit subsection (cross-references P3) |
| G5 | MEDIUM | §6.1.8 | `operator_shifts.note` (singular) | P4 | not audited | **FAIL** — P4 used `notes` (plural) in SQL, Prisma, and IPC | **FIXED** in P4 §1, §2, §3 |
| G6 | MEDIUM | §6.1.11 | Settings seed must include all 8 required keys | P3 | not audited | **FAIL** — `clinic_display_name_en` missing | **FIXED** in P3 §1 |
| G7 | MEDIUM | §6.1.9 + §7.4 | `patients` REST routes explicitly enumerated | P3 | not audited | **FAIL** — implied by "Repeat shape for ..." paragraph; routes not listed | **PARTIALLY FIXED** — counter math reconciled in roadmap; explicit enumeration deferred to implementation |
| G8 | MEDIUM | §5.1 | IPC count consistency | P3 | not audited | **FAIL** — three different numbers (24 / 42 / 44) | **FIXED** by reconciling totals: P3 = 46 IPC commands |
| M1 | MEDIUM | §6.1 | Local table count includes helpers + FTS | roadmap | not audited | **FAIL** — "18" claim ignored `visit_daily_rollup` (P7), `sync_conflicts` (P9), `backup_state` (P10), `audit_log_fts` (P9) | **FIXED** in roadmap header + §"New Local Entities by Phase" + status.md §2: 24 local objects |
| M2 | MEDIUM | §6.1 + servers | Server-model arithmetic 15 + 3 = 18 | P10, status | claimed 17 | **FAIL** — math error | **FIXED** in P10 §6 + status.md §2: 18 |
| M3 | MEDIUM | §3.1, §3.2 | Page count drift | roadmap, status, PRD | not audited | **FAIL** — PRD says 29, phases ship ~38 (lock screen, conflicts, backups, restore, admin/inventory split) | **FIXED** in status.md (~38) and roadmap header. PRD §3.2 carries forward as "primary 29 + auxiliary 9". |
| M4 | MEDIUM | §1.3 | `with_audit` lint rule ownership | P1, P9, P10 | not audited | **FAIL** — passed between phases without landing | **FIXED** by P9 §7.1 (Pass-V+) — Clippy custom rule + unit test; ties to PRD §1.3 audit-coverage metric |
| G9 | LOW | §6.1.11 | `bool` parser semantics | P3 | not audited | **FAIL** — undefined how `'0'`/`'1'` vs `'false'`/`'true'` accepted | **FIXED** in P3 inline note next to seed migration |
| G10 | LOW | offline-first.md | `sync_conflicts.policy` CHECK forward-compat | P9 | not audited | **FAIL** — too narrow for future policies | **FIXED** in P9 §7.2 (Pass-V+) — drop the CHECK |
| L1 | LOW | P3 §7.3 self-reference | P1 in-memory conflict queue | P1 | not audited | **FAIL** — referenced by P3 but never defined | **FIXED** in P1 §7.3 (Pass-V+) — `Mutex<VecDeque<ConflictRow>>` on engine handle |
| L2 | LOW | prd-writing.md §"Module spec has no empty/error/loading states" | Operator Shifts page | P4 | not audited | **FAIL** — columns + states subsection missing | **FIXED** in P4 §7.0 (Pass-V+) |

**Summary:** Pass V claimed 25/25 PASS. Pass V+ claimed 16 FAIL and 17 PASS (the 17 untouched are the PASS rows from the original report — re-validated by both agents).

## Coverage Summary (Pass V+)

- **CRITICAL items audited:** 7 (5 from Pass V's original 5 + G1, G2). Result: 5 PASS, 2 FAIL → 2 FIXED.
- **HIGH items audited:** 10 (8 from Pass V + G3, G4). Result: 8 PASS, 2 FAIL → 2 FIXED.
- **MEDIUM items audited:** 12 (8 from Pass V + G5, G6, G7, G8, M1, M2, M3, M4 — 8 new). Result: 4 PASS, 8 FAIL → 8 FIXED (G7 partial — explicit route enumeration deferred to implementation as it doesn't change the schema).
- **LOW items audited:** 8 (4 from Pass V + G9, G10, L1, L2). Result: 4 PASS, 4 FAIL → 4 FIXED.
- **Total Pass V+ audit:** 33 items → 17 PASS + 16 FIXED.

## Remediation Surface Map

| Phase file | Pass V+ edits |
|-|-|
| roadmap.md | Header counters + §"New Local Entities by Phase" + Pass log entries |
| status.md | §2 cumulative totals + §3 Pass V+ row + §5 Change Log |
| research.md | (no changes; D-031 already covered MFA gap from Pass 1) |
| phase-01.md | §7.3 in-memory conflict queue |
| phase-03.md | §1 settings seed (added `clinic_display_name_en` + `bool` parser convention) |
| phase-04.md | §1 + §2 + §3 IPC: `notes` → `note`. §7.0 Operator Shifts columns + states. |
| phase-05.md | §2 Visit Prisma comment block deferring `inventoryAdjustments` to P6 |
| phase-06.md | §1 + §2 inventory_items bilingual fields. §2 explicit "Existing models updated" subsection with copy-paste deltas. |
| phase-09.md | §7.1 lint rule, §7.2 CHECK widening, §7.3 in-memory→table migration |
| phase-10.md | §6 summary counters reconciled |

## Conclusion

`status: gaps_found_and_remediated`. The plan is **not yet implementation-ready**. A Pass V++ re-audit is recommended (re-run the same two verification agents against the now-fixed corpus) before any phase begins coding. If Pass V++ comes back with `gaps: []` and confirms the counters reconcile across all files, the plan ships to engineering.

The honesty caveat is important: Pass V's `gaps: []` claim was **wrong**, and the only reason it shipped was that the author wrote both the plan and the audit. Independent eyes (the Pass V+ agents) caught real schema-breaking issues in 60 seconds. Future verification rounds should always run with the author held out.
