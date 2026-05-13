# Phase NN: <Name> -- Test Plan

> Template. Copy to `../phase-NN-test.md` and replace every `<placeholder>` and `TODO`.
> See `.claude/rules/testing.md` §3 for the section schema and §11 for the DoD this plan satisfies.

**Proves:** <One sentence: what user-facing capability this plan verifies. Lift from `docs/idc-system/phase-NN.md` Goal.>

**Surfaces under test:** Frontend | Tauri/Rust | Sync Server | All
**Dependencies (other test plans):** Phase X test, Phase Y test (or "None")
**Test Data:** <which factories, which fixtures>
**Tool prerequisites:** <e.g. `cargo-llvm-cov` (Rust coverage), `vitest` + `@testing-library/react` + `jsdom` + `@vitest/coverage-v8` (frontend), `webdriverio` + `tauri-driver` (E2E), `ajv@8` + `ajv-formats` + `@apidevtools/json-schema-ref-parser` (contract), or "none new -- inherits from phase-XX-test">
**Out of scope (cross-cutting tests):** <list anything this phase touches but tests in a cross-cutting plan: `security.md` (e.g. refresh-token replay), `sync-conflicts.md` (3xN matrix), `i18n-rtl.md` (page-by-page), or another phase test. Use `none` if everything is in-scope.>
**Cross-phase commands:** <list any IPC commands registered in this phase's modules but conceptually owned by another phase, with a pointer to the test plan that covers them. Example: `shifts_lines_run_today` -- registered in phase-04 `lib.rs`, tested in `phase-05-test.md` §2.2. Use `none` when no such commands exist.>

---

## §1 Unit Tests (Pyramid Layer 1)

### §1.1 Rust domain services
| Module | Tests | Asserts |
|-|-|-|
| `src-tauri/src/domains/<x>/domain/services/<y>.rs` | `<test_name>` | <one-line assertion> |

### §1.2 TS pure functions / value objects
| Module | Tests | Asserts |
|-|-|-|
| `src/domains/<x>/services/<y>.ts` | `<test_name>` | <one-line assertion> |

### §1.3 Coverage targets

Each row maps a path glob to its threshold (per `.claude/rules/testing.md` §8) and the tool invocation that enforces it. Add a row for every source path this phase touched.

| Path glob | Threshold | Tool invocation |
|-|-|-|
| `src-tauri/src/domains/<x>/domain/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::<x>::domain` |
| `src-tauri/src/domains/<x>/service/**` | >= 90% lines | `cargo llvm-cov --lib --fail-under-lines 90 -- domains::<x>::service` |
| `src-tauri/src/domains/<x>/infrastructure/**` | >= 75% lines | `cargo llvm-cov --lib --fail-under-lines 75 -- domains::<x>::infrastructure` |
| `src-tauri/src/sync/**` (if this phase added sync code) | >= 95% lines | `cargo llvm-cov --lib --fail-under-lines 95 -- sync` |
| `src/features/<x>/**`, `src/lib/schemas/<x>.ts`, `src/lib/**` | >= 90% lines | `vitest --coverage --coverage.thresholds.lines=90 --coverage.include="src/features/<x>/**"` |
| `src/pages/**`, `src/components/<x>/**` | >= 60% lines | `vitest --coverage --coverage.thresholds.lines=60 --coverage.include="src/pages/**,src/components/<x>/**"` |
| `sync-server/src/app/domains/<x>/domain/**` (if any) | >= 90% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |
| `sync-server/src/app/domains/<x>/presentation/**` (if any) | >= 85% lines | `pnpm --filter sync-server test:coverage -- --reporter=lcov` |

Drop rows that don't apply to this phase. Do NOT relax a threshold silently -- a documented override requires §8 sign-off.

---

## §2 Integration Tests (Pyramid Layer 2)

### §2.1 Rust integration tests
- File: `src-tauri/tests/<entity>_phaseNN.rs` (continue the existing `sync_phase01.rs`, `shifts_phase04.rs` naming).
- Scenarios:
  - <happy path>
  - <error path>
  - <transaction rollback>

### §2.2 Tauri IPC handler tests
One test per command in this phase. Happy + at least one error path.

For any **cross-phase command** listed in the header, add a `(cross-ref)` row pointing at the owning phase plan instead of writing tests here. Example:
`| <command> | Owned by Phase-X test (introduced in phase-NN §7.Y). Cross-referenced. | (cross-ref) |`

| Command | Happy-path test | Error-path test |
|-|-|-|

### §2.3 Sync server route handlers
- DB: real Prisma test DB; per-test teardown.

| Route | Test | Asserts |
|-|-|-|

### §2.4 React Query mutation/query flows
- Mocked IPC. Assert cache invalidation, optimistic update, rollback on error.
- **RTL invariant (mandatory):** every component / hook test that renders DOM MUST run in both `dir=ltr` AND `dir=rtl`. Use `describe.each([['ltr'], ['rtl']])(...)` and assert layout invariants per `.claude/rules/design-system.md` §12. A test that runs only LTR is incomplete (per `.claude/rules/testing.md` §14 anti-pattern "RTL never tested").

| Hook | Test | Asserts |
|-|-|-|

---

## §3 Contract Tests (Pyramid Layer 3)

### §3.1 Swagger response validation
Every route this phase added. Ajv against `/documentation/json`.

| Route | Schema id | Sample payload |
|-|-|-|

### §3.2 IPC shape contract
Diff Rust `serde` JSON shape vs TS `Zod` / `Type` declaration. Fail on drift.

The last row is FIXED -- every phase that adds an IPC command also exercises the shared error envelope. Do not remove it.

| IPC command | Rust struct | TS schema |
|-|-|-|
| <each command from §2.2> | <Rust return type> | <Zod schema> |
| (Error envelope -- fixed) | `AppError` serialized via `Serialize` impl | `AppErrorSchema = z.object({ kind: z.enum([...]), message: z.string() })` -- one shared schema referenced by every command's error path. |

### §3.3 Sync envelope contract
- Push payload conforms to versioned envelope.
- Pull payload conforms.
- Each entity in this phase declares its conflict-resolution policy and the test asserts the declared policy matches expectation.

---

## §4 E2E Tests (Pyramid Layer 4)

### §4.1 Happy-path flows
WebdriverIO specs driving the built binary. One spec per major flow. Selectors are `data-testid` only (per `.claude/rules/testing.md` §14 anti-pattern "brittle CSS-selector E2E").

| Spec | Persona | Steps | Pass criteria |
|-|-|-|-|

### §4.2 Failure-path flows
- Offline at step N -- workflow continues, outbox grows.
- Token expiry mid-sync -- refresh, resume.
- Conflict triggered -- parked, surfaced in `/sync/conflicts`.

### §4.3 Multi-device flows
Set `MULTI_DEVICE=true`. Two binaries, shared sync server.

| Spec | Scenario | Pass criteria |
|-|-|-|

---

## §5 Manual / Persona Scripts (Pyramid Layer 5)

### §5.1 Scripts owned by this phase
Manual steps not yet automatable (visual, print, hardware).

- <step>: expected outcome.

### §5.2 Cross-references to personas.md
Persona scripts that exercise this phase's surfaces end-to-end:
- `personas.md` -> `<Persona Name>` -> step <N>.

---

## §6 Edge Case Coverage (8 mandatory categories)

> Every subsection must be filled. Acceptable forms:
> - A concrete test or scenario (preferred).
> - `N/A -- <one-line reason>` when the phase genuinely has no surface for the category.
> - `N/A -- owned by <cross-cutting plan or other phase test>` when the surface exists but is tested elsewhere (the cross-cutting `security.md`, `i18n-rtl.md`, `sync-conflicts.md`, or another phase plan). Match exactly the values declared in the header's `Out of scope` line.
> Empty is forbidden.

### §6.1 Time / Timezone
- Asia/Baghdad fixed offset; daily-close boundary at local midnight.
- Clock skew vs server: <test>.
- DST transition (Iraq does not observe DST -- test that the code does not assume DST anyway): <test>.

### §6.2 i18n & RTL
- en/ar swap on every route this phase added.
- Arabic-Indic numerals (`arabic_numerals: true` setting) on every numeric column.
- RTL layout invariants (eyebrow rule, table number alignment, status pill dots).

### §6.3 Offline & Network
- Full offline mode.
- Intermittent connection (drop mid-push).
- Token expiry mid-sync.
- Server returns 5xx.

### §6.4 Concurrency & Conflicts
- 2-device same row (assert declared policy).
- 3-device chain (LWW tiebreak by origin_device_id).
- Conflict resolver round-trip (parked -> resolve -> audit row).

### §6.5 Crash & Recovery
- SIGKILL during a multi-step transaction -- assert atomicity.
- SQLite WAL state after crash -- reopen succeeds, no corruption.
- Disk full on write path -- graceful failure, no half-written row.

### §6.6 Scale & Performance
- 10k row tables (loaded from `clinical-day.sql` scaled fixture).
- FTS5 search at 1k+ patients.
- Outbox drain throughput on a backlog of N ops.

### §6.7 Security & Permissions
- Role bypass attempts: receptionist tries an accountant route.
- JWT tampering: alter `role` claim and replay.
- FTS5 query injection: payload with `MATCH` operators.
- Soft-delete bypass: read a deleted row via a direct IPC.

### §6.8 Data Integrity
- Migration replay: fresh DB + on a populated DB.
- FK violation drills.
- Soft-delete cascade rules.
- `sync_version` monotonicity (never decreases).

---

## §7 Performance SLOs (this phase's surfaces)

Default SLOs in `.claude/rules/testing.md` §9 apply unless overridden. For each row, the `Default?` column declares whether the threshold is the §9 default (`yes`) or a phase-specific override (`no`). Overrides MUST have a rationale; a silent override is forbidden.

| Surface | Operation | Threshold | Default? | Test name | Rationale |
|-|-|-|-|-|-|

---

## §8 Definition of Done

Phase row in `testing-status.md` flips to `complete` only when EVERY box below is checked.

- [ ] All §1 unit tests green in CI.
- [ ] All §2 integration tests green in CI.
- [ ] All §3 contract tests green in CI.
- [ ] All §4 E2E tests green in CI.
- [ ] §5 persona scripts run and pass (record date/runner in row below).
- [ ] §6 all eight edge categories addressed (no empty subsections).
- [ ] §7 SLOs met for every row.
- [ ] Coverage gates per `.claude/rules/testing.md` §8 met for every row in §1.3.
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed where `.claude/rules/testing.md` §10 applies. List the paths this phase owns (or write `none -- phase adds no snapshot artifacts`):
  - <path-to-snapshot-1>
  - <path-to-snapshot-2>
- [ ] `testing-status.md` row updated (Unit / Integration / Contract / E2E / Manual counts, Coverage %, Started / Completed dates, Open Defects).
- [ ] Lint, typecheck, build all green.

**Persona run record:**

The first row is the **canonical persona** -- the one persona script that gates `complete` per `.claude/rules/testing.md` §11 ("at least one persona script in `personas.md` exercises this phase's surfaces end-to-end and passes"). Pick exactly one from `personas.md`. Additional rows are optional reinforcement runs.

| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
| Canonical persona (DoD-gating): `<P-N name>` | -- | -- | -- | -- |
