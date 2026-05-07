# Phase N: <Name>

**Goal:** <One sentence describing what this phase delivers>

**Surfaces:** Frontend | Tauri/Rust | Sync Server | All
**Dependencies:** Phase X, Phase Y (or "None")
**Complexity:** S | M | L | XL

## 1. Local Schema Changes (Tauri SQLite)

```sql
-- migrations/NNN_<name>.sql
CREATE TABLE IF NOT EXISTS <table> (
  id              TEXT PRIMARY KEY,
  -- domain columns
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  deleted_at      TEXT,
  version         INTEGER NOT NULL DEFAULT 0,
  dirty           INTEGER NOT NULL DEFAULT 1,
  last_synced_at  TEXT,
  origin_device_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_<table>_updated_at ON <table>(updated_at);
```

- Modified tables: <list field-by-field additions>
- New enums (CHECK constraints): <list>

## 2. Server Schema Changes (Prisma)

```prisma
model <Name> {
  id              String   @id @default(uuid()) @db.Uuid
  // domain fields
  entityId        String   @map("entity_id") @db.Uuid
  createdAt       DateTime @default(now()) @map("created_at") @db.Timestamptz
  updatedAt       DateTime @updatedAt @map("updated_at") @db.Timestamptz
  version         Int      @default(0)
  lastSyncedAt    DateTime? @map("last_synced_at") @db.Timestamptz
  tombstone       Boolean  @default(false)
  originDeviceId  String?  @map("origin_device_id")

  @@map("<table_name>")
  @@index([entityId, updatedAt])
  @@index([entityId, tombstone])
}
```

- Modified models: <list>
- New enums: <list>

## 3. DDD Implementation

### Frontend (React)

**Routes:**

| Path | File | Description |
|-|-|-|
| `/<resource>` | `pages/<resource>/list.tsx` | List view |
| `/<resource>/:id` | `pages/<resource>/detail.tsx` | Detail view |

**Stores:** `useThingDraftStore` (form drafts).
**Queries / Mutations:** `useThingList`, `useThingDetail`, `useCreateThing`, `useUpdateThing`, `useDeleteThing` in `features/<domain>/api/`.
**Schemas:** `thingSchema`, `createThingInput` in `features/<domain>/schemas.ts`.

### Tauri/Rust

**Entity:** `Thing` in `domains/<name>/domain/entities/thing.rs` -- `try_new()`, methods.
**Repository trait:** `ThingRepository` -- `find_all`, `find_by_id`, `upsert`, `soft_delete`.
**SQLite repo:** prepared statements for all four; transactional upsert.
**Tauri commands:**

| Command | Args | Returns | Description |
|-|-|-|-|
| `thing_list` | `{ filters? }` | `Vec<Thing>` | List active rows |
| `thing_get` | `{ id }` | `Option<Thing>` | One row or none |
| `thing_create` | `{ input, op_id }` | `Thing` | Create + queue for sync |
| `thing_update` | `{ id, patch, op_id }` | `Thing` | Patch + queue for sync |
| `thing_delete` | `{ id, op_id }` | `()` | Tombstone + queue for sync |

### Sync Server (Fastify)

**Entity:** `Thing` aggregate.
**Repository:** interface + Prisma impl.
**Schemas (TypeBox):** `ThingResponse`, `ThingListResponse`, `CreateThingBody`, `UpdateThingBody`.
**Routes:**

| Method | Path | Description |
|-|-|-|
| `GET` | `/things` | Paginated list |
| `GET` | `/things/:id` | Detail |
| `POST` | `/things` | Create |
| `PATCH` | `/things/:id` | Update |
| `DELETE` | `/things/:id` | Soft delete |

(Server routes are for direct admin / report use; the Tauri client uses `/sync/*` instead.)

## 4. Business Logic

### Service: `ThingService`

`createThing(input, actor)`:
1. Validate input via constructor invariants.
2. Generate UUID v7.
3. Persist locally in transaction.
4. Enqueue outbox op (`upsert`, `op_id`).
5. Emit `thing:created` event.

(Repeat numbered steps for each method, per surface.)

### Sync Contract

| Entity | Push? | Pull? | Conflict Policy | Notes |
|-|-|-|-|-|
| `thing` | yes | yes | last-write-wins | Tiebreak by `origin_device_id`. |

## 5. Infrastructure Updates

- TENANT_MODELS additions: `Thing` (server-side, has `entityId`).
- Audit triggers: `thing` table on `init-custom-sql.sql`.
- Local indexes: as listed in section 1.
- Tauri capabilities: no changes / <list scopes>.
- Plugins: <list>.

## 6. Verification

1. `cd src-tauri && cargo clippy --all-targets -- -D warnings` -- no lint errors.
2. `cd src-tauri && cargo test` -- all tests pass.
3. `pnpm lint && pnpm build` -- frontend builds cleanly.
4. `pnpm tauri dev` -- desktop app boots; smoke-test `/things` list, detail, create.
5. `mcp__curl__curl_get` to `/things` against the sync server -- returns the test data.
6. Sync round-trip: create offline -> reconnect -> server has it.
7. Conflict scenario: edit same record on two clients while offline -> reconnect both -> last-write-wins applied with no data loss.
8. Run existing tests -- no regressions.

## 7+. PRD Gap Additions

(Appended by gap analysis passes. Numbered subsections 7.1, 7.2, ...)
