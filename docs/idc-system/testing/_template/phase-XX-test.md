# Phase NN: <Name> -- Test Plan

> Template. Copy to `../phase-NN-test.md` and replace every `<placeholder>` and `TODO`.

**Proves:** <One sentence: what user-facing capability this plan verifies. Lift from `docs/idc-system/phase-NN.md` Goal.>

**Surfaces under test:** Frontend | Tauri/Rust | Sync Server | All
**Dependencies (other test plans):** Phase X test, Phase Y test (or "None")
**Test Data:** <which factories, which fixtures>
**Tool prerequisites:** <e.g. `cargo-llvm-cov`, `tauri-driver`, none>

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

### §1.3 Coverage target
- Domain layer: >= 90% lines (per `.claude/rules/testing.md` §8).

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

| Command | Happy-path test | Error-path test |
|-|-|-|

### §2.3 Sync server route handlers
- DB: real Prisma test DB; per-test teardown.

| Route | Test | Asserts |
|-|-|-|

### §2.4 React Query mutation/query flows
- Mocked IPC. Assert cache invalidation, optimistic update, rollback on error.

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

| IPC command | Rust struct | TS schema |
|-|-|-|

### §3.3 Sync envelope contract
- Push payload conforms to versioned envelope.
- Pull payload conforms.
- Each entity in this phase declares its conflict-resolution policy and the test asserts the declared policy matches expectation.

---

## §4 E2E Tests (Pyramid Layer 4)

### §4.1 Happy-path flows
WebdriverIO specs driving the built binary. One spec per major flow.

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

> Every subsection must be filled. `N/A -- <one-line reason>` is allowed; empty is forbidden.

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

Default SLOs in `.claude/rules/testing.md` §9 apply. Overrides for this phase:

| Surface | Operation | Threshold | Test name | Notes |
|-|-|-|-|-|

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
- [ ] Coverage gates per `.claude/rules/testing.md` §8 met.
- [ ] No open P0 or P1 defects against this phase in `defects.md`.
- [ ] Snapshot files committed where §10 applies (receipts, PDFs, sync envelopes).
- [ ] `testing-status.md` row updated.
- [ ] Lint, typecheck, build all green.

**Persona run record:**
| Persona | Runner | Date | Result | Notes |
|-|-|-|-|-|
