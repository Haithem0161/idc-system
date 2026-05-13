# Security Coverage Plan

Cross-cutting plan for authentication, authorization, input handling, and secret-storage invariants. See `.claude/rules/testing.md` §4 + §6.7, and `.claude/rules/auth.md`.

Security testing in IDC focuses on five attack surfaces: auth bypass, JWT integrity, role-based access control, injection (FTS5, JSON, headers), and secret storage. Every phase test plan addresses §6.7; this plan aggregates the cross-cutting drills and ensures the role-route matrix is exhaustively covered.

## Attack Surface Inventory

| Surface | Vehicle | Owner |
|-|-|-|
| Online auth | `POST /auth/login` | phase-02-test §6.7 |
| Offline auth | Stronghold + Argon2id cache | phase-02-test §6.7 |
| Token refresh | `POST /auth/refresh` | phase-02-test §6.7 |
| Password change | `POST /auth/change-password` | phase-02-test §6.7 |
| Superadmin bootstrap | `POST /auth/bootstrap-superadmin` | phase-02-test §6.7 |
| JWT validation (all routes) | Fastify `@fastify/jwt` plugin | phase-02-test §6.7 |
| Role-route matrix | All 35 frontend routes + all 16 server routes + all 106 IPC commands | this plan + per-phase plans |
| FTS5 query injection | `patients_fts`, `doctors_fts` | phase-03-test §6.7, phase-05-test §6.7 |
| JSON body validation | All TypeBox schemas on server | phase-09-test §6.7 (contract layer) |
| Header injection | All HTTP routes | phase-09-test §6.7 |
| Soft-delete bypass | Direct IPC reads | phase-02-test §6.7 (cross-cutting) |
| Refresh-token replay | `refresh_tokens` table | phase-02-test §6.7 |
| Secret storage | Tauri Stronghold | phase-02-test §6.7 |
| Audit-log tamper | `audit_log` write path | phase-01-test §6.7 |
| Sync envelope spoofing | Server `processed_op` check | phase-01-test §6.7 |

## Authentication Drills

| Drill | Expected behaviour |
|-|-|
| Login with valid credentials online | 200 + JWT pair |
| Login with valid credentials offline (cached) | success via local Argon2id verify |
| Login with wrong password 5 times | account locked per `.claude/rules/auth.md` policy; 429 or domain error |
| Login with expired account | denied |
| Login with deleted user | denied |
| Refresh with valid refresh token | new access token issued; old refresh rotated |
| Refresh with revoked refresh token | denied + audit row emitted |
| Refresh with refresh from different device | denied (or rotated per policy -- assert declared behaviour) |
| Change password as another user | denied |
| Bootstrap superadmin after one already exists | denied |
| Login after a long offline period (token expired but cache valid) | success via cache; flagged for re-online verification |

## JWT Tampering Drills

For every protected route, run all of:

| Tamper | Expected |
|-|-|
| Alter `role` claim from `receptionist` to `superadmin` | 401 (signature mismatch) |
| Use an HS256 token where the server expects RS256 | 401 |
| Use a token signed with the wrong private key | 401 |
| Use an expired token | 401 |
| Use a future-dated `nbf` token | 401 |
| Use a token with a missing `sub` | 401 |
| Use a token with a `sub` for a deleted user | 401 (server checks user is active) |

## Role-Route Access Matrix

Roles in IDC: `superadmin`, `accountant`, `receptionist`. Per `.claude/rules/auth.md`.

For every protected surface, assert:
- Allowed roles: request succeeds with 200/equivalent.
- Disallowed roles: request returns 403 + audit row (`access_denied` action).

The full matrix is too large to enumerate by hand here -- it is GENERATED. Each phase test plan §3.1 contract-tests the swagger schema; a derived test harness reads the swagger doc, extracts the `security` clauses, and runs all `role x route` permutations. Generation is owned by the contract-test harness in `sync-server/test/contract/role-matrix.test.ts` (to be authored).

Frontend route gates: every `<RoleGate>` wrapper has its own component test in the phase plan that introduced it (see phase-03-test §6.7, phase-04-test §6.7, ..., phase-08-test §6.7).

## Injection Drills

### FTS5

| Payload | Target | Expected |
|-|-|-|
| `Layla OR (SELECT *)` | `patients_fts` MATCH | Treated as literal; no SQL execution; safe-empty results |
| `*` alone | `patients_fts` MATCH | Either rejected or returns all (declared behaviour) |
| Very long string (10k chars) | `patients_fts` MATCH | Either truncated or rejected gracefully; no crash |
| Arabic + non-printable chars | `patients_fts` MATCH | Sanitized; no corruption |
| NULL byte | any text input | Rejected at validation layer |

### JSON / TypeBox

Every server route's request body is validated by a TypeBox schema. Drills:

| Payload | Expected |
|-|-|
| Missing required field | 400 + schema error |
| Extra unknown field | rejected (assert `additionalProperties: false` is set) |
| Wrong type (string for number, etc.) | 400 |
| Very large body (10MB) | rejected at Fastify body limit |
| Deeply nested object | rejected at depth limit |
| Prototype pollution attempt (`__proto__`) | rejected |

### Headers

| Header | Drill | Expected |
|-|-|-|
| `Authorization` | Missing | 401 |
| `Authorization` | Malformed | 401 |
| `Content-Type` | Wrong | 415 |
| `X-Tenant-Id` | Spoofed for another tenant | 403 (if applicable) |
| Path traversal (`../`) in any path param | request rejected | 400 |
| CRLF in any value | request rejected | 400 |

## Soft-Delete Bypass

Soft-delete is a sync-and-UI convenience; security MUST NOT depend on it. Drills:

- Issue a direct IPC to read a soft-deleted entity by ID -- assert it returns "not found" (not the soft-deleted row).
- Issue a sync push that touches a soft-deleted row from another device -- assert delete-wins per policy.
- Issue a sync pull for a soft-deleted entity -- assert it surfaces as deleted on the local side.
- Filter listings: assert deleted rows do not leak into list endpoints.

## Refresh-Token Replay

| Drill | Expected |
|-|-|
| Submit the same refresh token twice in quick succession | First succeeds; second returns 401 (rotated and revoked) |
| Submit a refresh token from a different device | declared behaviour: either denied or rotated; assert it matches |
| Submit a refresh token whose user has changed password since issuance | denied |

## Secret Storage Invariants

- Argon2id password cache encrypted at rest in Stronghold; key never leaves Rust process.
- JWT signing key on the server lives in env var; assert it is NOT logged anywhere on startup or in any error path.
- Frontend NEVER stores access tokens in `localStorage` (per CLAUDE.md "common pitfalls"). All token access goes through Rust IPC.
  - Drill: scan compiled frontend bundle for `localStorage.setItem` calls touching `token` / `jwt` / `auth` substrings; fail on match.

## Audit-Log Tamper Resistance

`audit_log` is the source of truth for what happened. Drills:

- Issue a direct IPC to delete an audit row -- assert it fails (no IPC exposed).
- Edit an audit row via SQL on the local DB and trigger a sync -- assert the server rejects the modified row (additive-only policy: row immutable after insert).
- Server-side: attempt to update an existing `audit_log` row via Prisma -- assert it is blocked by a Postgres trigger or model-level guard.

## Sync Envelope Spoofing

- Replay a previously-processed `opId` -- assert `processed_op` dedupes and returns the original result deterministically.
- Submit a push from device A masquerading as device B (`origin_device_id` mismatch with auth user) -- assert it is rejected.
- Submit a push with a future `updated_at` to win LWW unfairly -- assert the server clamps or rejects per declared policy.

## Coverage Tracker

| Surface | Drills | Done | Open |
|-|-|-|-|
| Auth | 11 | 0 | 11 |
| JWT tamper | 7 | 0 | 7 |
| Role-route matrix | generated | 0 | depends on swagger surface count |
| FTS5 injection | 5 | 0 | 5 |
| JSON / TypeBox | 6 | 0 | 6 |
| Headers | 6 | 0 | 6 |
| Soft-delete bypass | 4 | 0 | 4 |
| Refresh-token replay | 3 | 0 | 3 |
| Secret storage | 4 | 0 | 4 |
| Audit-log tamper | 3 | 0 | 3 |
| Sync envelope spoofing | 3 | 0 | 3 |
| **Total (named)** | **52 + matrix** | **0** | **52 + matrix** |
