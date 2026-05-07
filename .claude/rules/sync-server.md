---
paths:
  - "sync-server/**"
  - "**/fastify*"
  - "**/swagger*"
  - "**/*.prisma"
  - "**/plugins/**"
  - "**/routes/**"
  - "**/schemas/**"
---

# Sync Server Rules (Fastify + Prisma)

The sync server is a single Fastify service that the Tauri app talks to for **sync** (push / pull / realtime) and **backup** (snapshots, restore, exports). It is NOT a general-purpose API for the frontend -- the desktop app is the source of truth, and the server's contract surface is intentionally narrow.

When the server doesn't yet exist in this repo, this file documents the conventions any future commit MUST follow. When it does, this file is the authoritative rulebook for it.

## Core Principles

1. **Plugin-First.** Use Fastify plugins for everything; don't reinvent the wheel.
2. **Context7 First (MANDATORY).** Before writing code that uses a library/plugin (Fastify core, `@fastify/*`, Prisma, BullMQ, TypeBox, undici, jsonwebtoken, etc.), call `resolve-library-id` then `query-docs`. Implementation must be grounded in current docs.
3. **Comprehensive Swagger.** Every route MUST have a TypeBox schema with description, tags, summary, body/response, security.
4. **Plugin-First Auth.** RS256 JWT verified by an `auth.plugin.ts`; no hand-rolled middleware in routes.
5. **Multi-tenant by default.** A `tenant.plugin.ts` decorates `request.tenant!.db` with a Prisma client extension that injects `entityId` into every query for tenant-scoped models.
6. **No emojis** anywhere.
7. **Package management:** `pnpm add <pkg>` only -- never edit `package.json` by hand.
8. **`.js` extensions on all relative imports** (required by `nodenext` module resolution).

## Layout (DDD + Plugin)

```
sync-server/
├── src/
│   ├── main.ts                       # entry point
│   ├── app/
│   │   ├── app.ts                    # main app plugin (Swagger, error handler, autoload)
│   │   ├── plugins/                  # auto-loaded
│   │   │   ├── config.ts             # @fastify/env validation
│   │   │   ├── prisma.plugin.ts      # PrismaClient + pg pool + tenant extension
│   │   │   ├── auth.plugin.ts        # JWT verification, requireRole, requireEntityContext
│   │   │   ├── tenant.plugin.ts      # request.tenant! decorator
│   │   │   ├── redis.plugin.ts       # cache + BullMQ
│   │   │   ├── swagger.plugin.ts     # @fastify/swagger + @fastify/swagger-ui
│   │   │   └── errors.plugin.ts      # @fastify/sensible + global error refs
│   │   ├── common/                   # shared utils, errors, schemas
│   │   ├── db/                       # prisma/, migrations/, init-custom-sql.sql
│   │   ├── sync/                     # the sync engine SERVER-side
│   │   │   ├── routes/               # /sync/push, /sync/pull, /sync/stream
│   │   │   ├── service/              # push/pull orchestration
│   │   │   └── conflict/             # per-entity policies
│   │   └── domains/<domain-name>/
│   │       ├── domain/               # entities, repository interfaces, services
│   │       ├── presentation/         # routes, schemas
│   │       └── infrastructure/       # prisma repos, jobs
│   └── ...
├── prisma/schema.prisma
├── prisma/init-custom-sql.sql
├── .env.template
└── Dockerfile
```

## Required Plugins

| Plugin | Purpose |
|-|-|
| `@fastify/sensible` | Common HTTP errors (`reply.notFound`, `badRequest`, `unauthorized`, `conflict`). |
| `@fastify/cors` | Strict origin allowlist (Tauri webview origin + dev origins). |
| `@fastify/helmet` | Security headers. |
| `@fastify/rate-limit` | Per-IP and per-user limits; tighter on `/sync/push`. |
| `@fastify/jwt` | RS256 verification using `JWT_PUBLIC_KEY_PATH`. |
| `@fastify/env` | Env validation -- required + typed. |
| `@fastify/compress` | Response compression. |
| `@fastify/swagger` + `@fastify/swagger-ui` | Auto-generated docs at `/documentation`. |
| `@fastify/multipart` | For backup uploads/restores. |
| `@prisma/client` + `@prisma/adapter-pg` | Database. |

## Sync Endpoints (Canonical Contract)

| Method | Path | Purpose |
|-|-|-|
| `POST` | `/sync/push` | Apply a batch of client ops (`upsert` / `delete`). Returns per-op results: `applied`, `conflict`, `rejected`. Idempotent on `op_id`. |
| `GET` | `/sync/pull?since=<cursor>&limit=<n>` | Stream changes for the tenant since cursor. Returns `{ changes, nextCursor, hasMore }`. |
| `GET` | `/sync/stream` | SSE feed of new changes for the tenant. Optional optimisation; pull is authoritative. |
| `GET` | `/sync/health` | Server health + schema version + retention policy. |
| `POST` | `/backup/snapshot` | Trigger a full snapshot for the tenant. Returns a job id. |
| `GET` | `/backup/snapshots` | List available snapshots. |
| `POST` | `/backup/restore` | Restore from a snapshot id (admin only, dangerous). |
| `GET` | `/backup/export?format=<json|csv>&entity=<>` | Export user data (GDPR). |

These contracts are versioned via `Accept-Version` header. Breaking changes cut a new version; the old version is supported until all installed clients are above the new minimum.

## Route Schema Requirements

Every route MUST have:
- `description` -- multi-line markdown (Features, Use Cases, Behavior, Restrictions, Side Effects).
- `tags` -- for Swagger grouping (e.g., `['sync']`, `['backup']`, `['<domain>']`).
- `summary` -- one-line.
- Full `body` / `querystring` / `params` schemas.
- All `response` schemas (200, 400, 401, 403, 404, 409, 422, 426, 5xx) -- use `$ref` to common error schemas (`{ $ref: 'NotFoundError#' }`).
- `security` requirements when protected.
- `onRequest: [fastify.authenticate, fastify.requireEntityContext]` for tenant-scoped routes.

## Response Pattern

```typescript
{ success: true, data: <typed payload> }
// or
{ success: false, error: { code, message, details? } }
```

Never return bare arrays. Lists are `{ success: true, data: { items, total, cursor } }`.

## Domain Layer Rules

- Use `AggregateRoot<Props>` with `create()` / `reconstitute()` factories and `toPrisma()` / `toResponse()` serializers.
- Domain layer has ZERO external dependencies (no Prisma, no Fastify).
- Repositories: interface in `domain/repositories/`, Prisma implementation in `infrastructure/repositories/`.
- Nullable fields: `Type.Union([T, Type.Null()])` in response schemas; `?? null` (NOT `?.`) in `toResponse()`.

## Prisma

- Schema columns: `@map("snake_case")`, tables: `@@map("table_name")`, timestamps: `@db.Timestamptz`, IDs: `@default(uuid())`.
- Sync columns on every syncable model: `version Int`, `lastSyncedAt DateTime?`, `tombstone Boolean @default(false)`, `originDeviceId String?`.
- Push / sync indexes: `@@index([entityId, updatedAt])`, `@@index([entityId, tombstone])`.
- Use `prisma db push` (NOT `migrate dev`) -- shadow DB fails with P3006. Run `init-custom-sql.sql` after every push.
- Nested creates use relation names (`department: { connect: { id } }` NOT `departmentId`). Field-whitelist nested data.
- Tenant scoping: only models WITH `entityId` go in `TENANT_MODELS`. Child / junction models inherit isolation via FK -- do NOT add them; the extension will crash.

## Auth

- RS256 JWT, public key loaded by `auth.plugin.ts` from `JWT_PUBLIC_KEY_PATH`.
- JWT fields: `sub` (userId), `email`, `status`, `isSuperadmin`, `sessionId`, `entityId`, `holdingId`, `entityRole`.
- Inter-service auth (when more services exist): `x-service-api-key` header, 32+ char random.
- The Tauri app authenticates as a regular user; there is no machine-to-machine flow for end-user devices.

## Error Handling

- `@fastify/sensible` only: `reply.notFound()`, `reply.badRequest()`, `reply.unauthorized()`, `reply.conflict()`, `reply.unprocessableEntity()`.
- Schema validation errors return 422 with the field path -- never leak internal details.
- 409 Conflict on sync: include `{ serverVersion, clientVersion, both: { server, client } }` so the client can run the conflict resolver.

## Background Jobs (BullMQ)

- Pattern: route enqueues to Redis sub-millisecond -> BullMQ worker (concurrency = 5) picks up -> performs work -> retry 3x with exponential backoff (2s base).
- Use cases: backup snapshot generation, retention sweeps, large export jobs.
- Workers live in `domains/<name>/infrastructure/jobs/<name>.job.ts`. Queue setup in `plugins/bullmq.plugin.ts`.

## Docker

- All sync-server work runs in Docker via `docker-compose.yaml`. Do NOT use `nx serve` locally.
- Allowed: `docker compose up/restart/down`, `docker exec`, `docker logs`, `docker restart`.
- FORBIDDEN: `docker rm`, `docker compose rm`, `docker system prune`, `docker container prune`, `docker volume prune`, `docker image prune`.
- Dockerfile.dev MUST run `prisma db push` + `init-custom-sql.sql` on startup before launching the app.
- After `pnpm add`: `docker compose up -d --force-recreate -V <service>` to bust the anonymous-volume `node_modules` cache.

## Testing the Sync Server

- HTTP requests in tests/manual checks use the curl MCP tools (`mcp__curl__curl_post`, etc.). Bash `curl` is acceptable only when the MCP equivalent doesn't support the feature (e.g., multipart binary upload edge cases).
- Auth flow: `curl_post` to `/auth/login` -> use `accessToken` as `Authorization: Bearer <token>`. Re-login on 401.

## Common Pitfalls

- Anonymous Docker volumes cache stale `node_modules` -- always use `-V` after `pnpm add`.
- New files may not compile in Docker watch mode until container restart.
- Nullable fields: `Type.Union([T, Type.Null()])` not `Type.Optional()` in response schemas.
- Entity `toResponse()` for nullable: `?? null` not `?.`.
- Adding a model without `entityId` to `TENANT_MODELS` crashes the extension on first request.
- Sync push with no `op_id` MUST be rejected 400 -- there is no fallback dedupe.
