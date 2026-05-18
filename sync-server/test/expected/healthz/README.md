# Healthz canonical snapshots

Two canonical responses pin the `/healthz` envelope shape (phase-09 BLOCKER-5
DoD). Regeneration is explicit: edit the JSON, recompute the sha256, commit
both. CI verifies the byte-exact response against these hashes.

| Snapshot | When it applies |
|-|-|
| `healthz-ok-canonical.json` | Test path: no `DATABASE_URL`, in-memory store, no Redis. All probes report ok and `migrationsApplied=true` (memory fallback). |
| `healthz-fail-canonical.json` | Failure path: Prisma wired but DB unreachable; `db=fail`, `migrationsApplied=false`, overall `status=fail`. |

These hashes guard the shape: if a future schema change adds, removes, or
reorders a key, CI fails until the snapshot is regenerated with explicit PR
review (per `.claude/rules/testing.md` §10).
