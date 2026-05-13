# IDC System Personas -- Day-Scripts

Cross-cutting persona day-scripts. Each persona is a named actor with a profile, a sequenced workflow, failure injections at known points, and explicit pass criteria. See `.claude/rules/testing.md` §3.5 + §4.

A persona script is a single E2E run from app boot to app close. It exercises 3+ phases end-to-end and catches integration gaps that unit/integration tests miss. Personas are the "real-life" testing arm of the suite.

## How Personas Run

- Loaded against the `fixtures/clinical-day.sql` seed unless a script explicitly overrides.
- Executed by WebdriverIO + `tauri-driver` against the built binary (`pnpm test:e2e`).
- Two-device personas use `MULTI_DEVICE=true` to spin a second instance on a different port.
- Sync server runs in Docker (`docker compose up -d`) before the run; Postgres is seeded from the same fixture mapped through Prisma.
- Each script logs a structured trace (step number, timestamp, IPC calls, outbox state) to `personas/runs/<persona>-<iso-date>.jsonl` for post-run inspection.
- Pass criteria are asserted by the script; a manual "looked fine" is not a pass.

Persona runs are mandatory for phase DoD (`.claude/rules/testing.md` §11). At least one persona must touch every phase's surfaces.

## Persona Roster

| # | Name | Role | Phases touched | Devices |
|-|-|-|-|-|
| P1 | Asma the Accountant | Accountant | 02, 07, 08 | 1 |
| P2 | Mehdi the Receptionist | Receptionist | 02, 04, 05, 06 | 1 |
| P3 | Mariam the Superadmin | Superadmin | 01, 02, 03 | 1 |
| P4 | Two-Device Conflict | Receptionist + Receptionist + Superadmin | 01, 05, 08 | 2 |
| P5 | Year-End Audit | Accountant + Superadmin | 02, 07, 08 | 1 |

Future personas (added as phase test plans surface new flows): patient-side intake kiosk, multi-clinic franchise admin, etc.

---

## P1 -- Asma the Accountant

### Profile
- Role: `accountant`
- Device: single Tauri binary, Linux, locale `ar` with `arabic_numerals: true`
- Baseline data: `clinical-day.sql` -- 30 visits already exist for "today" (Tuesday), of which 22 are paid, 8 outstanding; 5 doctors active; 2 operator shifts closed.
- Mental model: opens the app at end of day, wants to close the day cleanly and run an earnings report.

### Day Script
1. Boot the binary; verify the lock screen renders in RTL with Arabic-Indic numerals on the time display.
2. Log in online with her superadmin-issued accountant credentials.
3. Land on `/accounting`. Assert the 5 KPI tiles render with `tnum` numerals and the eyebrow rule reads "Tuesday * 12 May 2026 * 18:42" in Arabic.
4. Open `/accounting/visits` with default filter (today, all statuses). Assert 30 rows; assert 8 highlighted as outstanding.
5. Apply filter: status = `paid`, group by `check_type`. Assert grouping aggregates show correct sums; export CSV; assert downloaded file matches the in-app row totals.
6. Click on doctor Dr. Mohammed Hassan from the dashboard top-5 list. Land on `/accounting/doctors/<id>`. Assert earnings breakdown by check type renders, source visit list is correct.
7. Click on operator Kareem from the dashboard. Land on `/accounting/operators/<id>`. Assert hours on shift + attributed visit window matches the shift table.
8. Go to `/accounting/daily-close`. Assert the close UI shows "provisional" status (since 2 ops are still in the outbox, simulated by network drop in step 9).
9. **Failure injection:** disconnect from network; click "Sync" -- assert sync pill shows offline.
10. Re-enable network; assert sync pill resumes; assert 2 pending ops drain; assert daily-close status flips from provisional to ready-to-close.
11. Sign + freeze the daily close. Assert PDF generates; capture hash, compare to expected snapshot.
12. Log out.

### Failure Injections
- Step 9: network disconnect during sync.
- Optional: kill the app between steps 7 and 8; reopen; assert state is preserved.

### Pass Criteria
- All 12 steps complete without manual intervention.
- Step 11 PDF hash matches the snapshot in `expected/asma-daily-close-tuesday.pdf.sha256`. If renderer changed, regenerate with `--update-snapshots` + visual review.
- Trace log shows no IPC errors and the outbox is empty at logout.
- Visual: assert RTL layout invariants (eyebrow on right, numerals right-aligned in tables).

---

## P2 -- Mehdi the Receptionist

### Profile
- Role: `receptionist`
- Device: single Tauri binary, Windows, locale `en` with Western numerals.
- Baseline data: `clinical-day.sql` cleared of today's visits; doctors + patients + inventory seeded; one prior operator_shift for Mehdi closed yesterday.
- Mental model: busy Tuesday morning, walks through 25 patients between 08:00 and 13:00, takes a lunch break offline.

### Day Script
1. Boot, log in, land on `/reception`.
2. Open shift: clock in via `/reception/shifts`. Assert shift row created with current timestamp.
3. For each of 25 visits (loop):
   - Search for patient in FTS5 (some by partial name, some by phone number, one new patient created mid-loop).
   - Create new visit with 1-3 checks, doctor assignment, optional inventory consumption.
   - Lock the visit (full snapshot + pricing + inventory deduction in one transaction).
   - Print A5 receipt -- assert PDF generates; sample 3 of 25 for hash comparison.
   - Visit 12 deliberately uses an off-catalog inventory item amount; assert validation error.
   - Visit 18 has a doctor with delayed-report flag; assert visit-detail shows the pending-report indicator.
4. **Failure injection:** at visit 14, the network drops for 22 minutes (lunch). The receptionist continues working offline. Assert all 14-22 visits are committed locally; outbox accumulates.
5. Network restored at visit 23. Assert sync pill goes from offline to pushing; assert all queued ops drain within the SLO (50 ops/sec); assert no conflicts (single device).
6. At visit 25, clock out via `/reception/shifts`. Assert shift updated_at; assert hours computed correctly.
7. Manual quick-check: pull up `/inventory` -- assert all consumption from today is reflected in on-hand counts.
8. Log out.

### Failure Injections
- Step 4: 22-minute network drop covering 8 visits.
- Step 3 visit 12: invalid inventory amount.
- Optional: SIGKILL during visit 17 lock transaction; reopen; assert visit-17 is either fully committed or fully absent (no half-state).

### Pass Criteria
- All 25 visits land as `locked` with correct totals.
- Outbox empty within 60 seconds of network restoration.
- 3 sampled A5 receipts hash-match expected snapshots.
- Inventory adjustments sum equals expected consumption from the 25 visits.
- No P0 or P1 defects logged.

---

## P3 -- Mariam the Superadmin

### Profile
- Role: `superadmin`
- Device: fresh install, no prior data. Bootstrap from blank.
- Mental model: setting up the clinic for the first time. Adds users, doctors, operators, inventory, configures settings.

### Day Script
1. First launch: assert the `FirstLaunchSetupModal` appears.
2. Configure sync server URL (Docker compose target).
3. Bootstrap the superadmin (`/auth/bootstrap-superadmin`). Set password.
4. Log in.
5. Add 3 doctors via `/admin/doctors`, each with 2-3 check pricings.
6. Add 5 inventory items via `/admin/inventory-items`, with 2 of them linked to consumption maps.
7. Add 2 operators via `/admin/operators`, assign specialties.
8. Add 1 receptionist + 1 accountant user via `/admin/users`. Send credentials.
9. Configure system settings via `/admin/settings`: language default, currency, working hours.
10. Trigger a sync push. Assert all the catalog, users, settings flow to the server; verify via direct curl against the sync server's `/sync/pull` from a fresh second device (no E2E, just contract).
11. **Failure injection:** stop the sync server mid-step-8; assert the user-creation UI surfaces "queued offline" state; restart server; assert push resumes.
12. Log out.

### Failure Injections
- Step 11: sync server stopped during user creation.

### Pass Criteria
- All scaffold data lands on the server within 30 seconds of network restoration.
- Sync envelope schema asserted via contract test (§3.3 of phase plans).
- No `last-write-wins` conflicts (single-device setup).

---

## P4 -- Two-Device Conflict

### Profile
- Two devices: Device A (Mehdi, receptionist, locale `en`) + Device B (Sara, receptionist, locale `ar`).
- Both online, both have `clinical-day.sql` synced.
- Mental model: same patient walks into the clinic; Mehdi and Sara both update her record at nearly the same time. Mariam (superadmin) arrives later to resolve.

### Day Script
1. Boot both devices. Verify both are at sync_state head.
2. Device A and Device B both go offline.
3. Device A: edit patient "Layla Hashim" address (LWW entity).
4. Device B: edit patient "Layla Hashim" phone (LWW entity, different field).
5. Device A: create a NEW visit for Layla.
6. Device B: create a NEW visit for Layla.
7. Both devices reconnect within 2 seconds of each other.
8. Both push to the server.
9. **Assert (additive entity, `visits`):** the policy is `manual` -- both visits should park in `ConflictParked`. Both devices' `/sync/conflicts` count badges should increment.
10. **Assert (LWW entity, `patients`):** the policy is `last-write-wins`. The patient row converges to the lexicographically smaller `origin_device_id` on tie; verify both devices pull the same final state.
11. Boot Device C (Mariam, superadmin). Navigate to `/sync/conflicts`. Assert the two visit conflicts are listed.
12. Mariam resolves: keeps Device A's visit, discards Device B's.
13. Resolver emits an `conflict_resolve` audit row. Assert both Device A and Device B pull this row.
14. Assert Device B's discarded visit is reflected as resolved (UI shows the resolution result).
15. Log out all three devices.

### Failure Injections
- Step 7: timing matters -- intentionally vary the gap between reconnects (0ms, 500ms, 2s) and assert the same final state.

### Pass Criteria
- Zero data loss: every push attempt is either committed, parked, or returned to the user with a clear status. Nothing vanishes.
- Conflict policy enforcement: LWW entities converge; manual entities park; additive-only entities never conflict.
- Resolution audit row is visible on all participating devices.

---

## P5 -- Year-End Audit

### Profile
- Asma (accountant) + Mariam (superadmin) on the same device, role-switching via logout/login.
- Baseline data: `clinical-day.sql` extended with 12 months of synthetic visits (loaded via a separate scale fixture; documented in `fixtures/README.md`).
- Mental model: year-end audit. Pull aggregate reports, query the audit log, export PDFs.

### Day Script
1. Asma logs in.
2. Run `/accounting/visits` with a 12-month date range. Assert the report renders within the 90-day-window SLO scaled to 12 months (target: < 4s p95 for 12 months, vs < 1s for 90 days).
3. Export visits CSV. Assert row count matches expected (e.g. 8000+).
4. Run `/accounting/doctors` summary; drill into top doctor; assert earnings sum matches CSV.
5. Run `/accounting/operators` summary; drill into top operator; assert hours sum matches operator_shifts data.
6. Run `/accounting/daily-close` for an arbitrary day 8 months ago. Assert PDF generates and matches the historical record (regenerated on demand).
7. Log out. Log in as Mariam.
8. Navigate to `/audit`. Query: action = `lock`, date = last 12 months. Assert N rows where N matches the visit count.
9. Export audit CSV. Assert non-empty and well-formed.
10. Query: action = `conflict_resolve`, last 12 months. Assert all historical resolutions are visible.
11. Run audit vacuum job. Assert local audit_log shrinks to 90-day retention while server retains indefinitely.
12. Log out.

### Failure Injections
- Step 11: trigger vacuum while another sync push is in flight; assert no race condition.

### Pass Criteria
- All reports complete within scaled SLOs.
- Aggregate sums reconcile across reports (visits CSV total = doctor earnings sum = operator earnings sum within rounding).
- Audit vacuum behaves correctly: local pruning, server untouched.
- Daily-close historical PDFs are byte-stable (same input -> same hash).
