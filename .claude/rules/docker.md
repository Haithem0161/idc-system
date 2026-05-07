---
paths:
  - "**/Dockerfile*"
  - "**/docker-compose*"
  - "**/.env*"
  - "sync-server/**"
---

# Docker Rules

Docker applies primarily to the **sync server** stack. The Tauri app itself is not containerized for end-user use -- it ships as a native bundle. Local Postgres + Redis containers exist to develop the sync server.

## Allowed Commands

- DO: `docker restart`, `docker exec`, `docker logs`.
- DO: `docker compose up`, `docker compose restart`, `docker compose down` (down stops -- it does not delete volumes by default).
- DO: Edit `Dockerfile`s and `docker-compose.yaml`.

## FORBIDDEN Commands

- **NEVER** run `docker rm`, `docker compose rm`, or any container-removal command.
- **NEVER** run `docker system prune`, `docker container prune`, `docker volume prune`, `docker image prune`, or any destructive cleanup.
- These are blocked by the `block-destructive` PreToolUse hook. If you genuinely need a fresh state, ask the user.

## Sync Server Compose

```bash
docker compose up -d sync-server                                # start sync server + its DB
docker compose restart sync-server                              # picks up Prisma schema changes
docker compose up -d --force-recreate -V sync-server            # full recreate (use after pnpm add)
docker logs sync-server --tail 200 -f                           # tail logs
```

## Dockerfile Auto-Sync (Sync Server)

The sync server's `Dockerfile.dev` MUST run schema sync on start before launching the app:

```bash
npx prisma db push --accept-data-loss
psql $DATABASE_URL -f prisma/init-custom-sql.sql
node dist/main.js
```

If a `Dockerfile.dev` is missing this pattern, add it. No service should require manual `docker exec` to sync schema.

## Schema Changes (Prisma)

After modifying `prisma/schema.prisma`:

```bash
# 1. Restart container -- auto-runs prisma db push on startup
docker compose restart sync-server

# 2. If init-custom-sql.sql changed (new triggers, functions, audit tables):
docker exec -i sync-db psql -U postgres -d sync_db < sync-server/prisma/init-custom-sql.sql
```

## Decision Trees

**When to restart vs rely on watch mode:**
- Watch mode handles edits to existing `.ts` files (live reload via bind mounts).
- Restart needed (`docker compose restart`): new files added, Prisma schema changes, config changes.
- Full recreate needed (`--force-recreate -V`): new packages installed, dependency changes.

**When to use `-V` with Docker:**
- After running `pnpm add <package>` (anonymous volumes cache stale `node_modules`).
- After major dependency changes.
- Command: `docker compose up -d --force-recreate -V sync-server`.
