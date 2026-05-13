# Sync Conflict Coverage Matrix

Cross-cutting plan covering the 3 conflict policies across every entity that declares one. See `.claude/rules/testing.md` §4 + §6.4 and `.claude/rules/offline-first.md`.

The IDC system uses three conflict policies. Each entity declares exactly one. This matrix has one row per (entity, scenario) cell. Every row MUST have a deterministic test before its phase test plan can flip to `complete`.

## Policy Catalogue

| Policy | Behaviour | Entities (per build status.md) |
|-|-|-|
| `additive-only` | Never conflicts. Duplicates are tolerated (ordering by `created_at`). Updates use LWW with `origin_device_id` tiebreak. Soft-delete uses delete-wins. | `audit_log`, `operator_shifts`, `inventory_adjustments` |
| `last-write-wins` | Server keeps the row with the higher `updated_at`. On tie, lexicographically smaller `origin_device_id` wins. Delete-vs-edit: delete wins on tie. | `users`, all 8 catalog entities (`check_types`, `check_subtypes`, `doctors`, `doctor_check_pricing`, `operators`, `operator_specialties`, `inventory_items`, `inventory_consumption_map`), `patients` |
| `manual` | On version / `updated_at` mismatch, row is parked in `ConflictParked`. Resolution is via `/sync/conflicts/:opId/resolve` by a superadmin. | `settings`, `visits` |

Total: 15 syncable entities, each declares one policy. The matrix below lists the scenarios for each cell.

## Scenarios (per cell)

For every (entity, policy) cell:

1. **Happy duplicate / no conflict** -- two devices create independently; both land cleanly.
2. **Concurrent update, different fields** -- LWW merges; manual parks.
3. **Concurrent update, same field** -- policy's tie-breaking rule applies and converges.
4. **Update vs delete** -- delete wins (for LWW + additive); manual parks (for manual).
5. **Three-way chain** -- A -> B -> C in close succession; final state is C's value (LWW) or first parked (manual).
6. **Origin-device-id tiebreak** -- two writes with identical `updated_at` (forced via mocked clock); deterministic winner.
7. **Soft-delete sync** -- `deleted_at` propagates correctly; subsequent read returns nothing.
8. **Resolver round-trip** (manual only) -- park -> superadmin resolves -> audit row emitted -> all devices observe.

## Matrix

> Test name format: `sync_<entity>_<scenario>.rs` (Rust integration) or `sync_<entity>_<scenario>.e2e.ts` (E2E). One file per cell.

### additive-only

| Entity | S1 dup | S2 diff-field | S3 same-field | S4 vs-delete | S5 chain | S6 tiebreak | S7 soft-del | S8 resolver | Owning phase test |
|-|-|-|-|-|-|-|-|-|-|
| `audit_log` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-01-test |
| `operator_shifts` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-04-test |
| `inventory_adjustments` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-05-test, phase-06-test |

### last-write-wins

| Entity | S1 dup | S2 diff-field | S3 same-field | S4 vs-delete | S5 chain | S6 tiebreak | S7 soft-del | S8 resolver | Owning phase test |
|-|-|-|-|-|-|-|-|-|-|
| `users` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-02-test |
| `check_types` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `check_subtypes` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `doctors` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `doctor_check_pricing` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `operators` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `operator_specialties` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `inventory_items` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `inventory_consumption_map` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-03-test |
| `patients` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | n/a | phase-05-test |

### manual

| Entity | S1 dup | S2 diff-field | S3 same-field | S4 vs-delete | S5 chain | S6 tiebreak | S7 soft-del | S8 resolver | Owning phase test |
|-|-|-|-|-|-|-|-|-|-|
| `settings` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | TODO | phase-02-test |
| `visits` | TODO | TODO | TODO | TODO | TODO | TODO | TODO | TODO | phase-05-test |

## Coverage Tracker

| Policy | Entities | Cells | Done | Open |
|-|-|-|-|-|
| additive-only | 3 | 21 (7 per entity, S8 n/a) | 0 | 21 |
| last-write-wins | 10 | 70 (7 per entity, S8 n/a) | 0 | 70 |
| manual | 2 | 16 (8 per entity) | 0 | 16 |
| **Total** | **15** | **107** | **0** | **107** |

## Authoring Notes

- Tests live alongside the phase test plan that owns the entity, NOT in a central conflict-test folder. The phase's `phase-XX-test.md` §2.3 / §4.3 list each cell as a row.
- The matrix above is the source of truth for "what cells exist." The phase plan is the source of truth for "what cells are written."
- A cell counts as `done` when: an automated test exists AND it passes AND it asserts the correct policy outcome (not just "no crash").
- The `n/a` cells for `S8 resolver` on non-manual policies are correct -- only `manual` entities trigger the resolver.
