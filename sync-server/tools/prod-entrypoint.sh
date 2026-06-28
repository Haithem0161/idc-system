#!/bin/sh
# Production start sequence for the sync-server (phase-10 T10).
#
# Order:
#   1. Back up the existing database with pg_dump BEFORE any schema change, so a
#      `db push` that drops or rewrites a column is recoverable. The backup is
#      written to /backups (mount a host volume there). On the very first boot
#      the database has no tables yet, so a dump failure is non-fatal -- we log
#      and continue.
#   2. `prisma generate` (client) + `prisma db push` (NO --accept-data-loss, so a
#      destructive drift FAILS the boot instead of silently destroying data).
#   3. Apply the raw-SQL constraints/triggers (init-custom-sql.sql).
#   4. Start Fastify.
#
# Why pg_dump + db push rather than `prisma migrate deploy`: the project's
# sync-server convention (.claude/rules/sync-server.md) is `db push`, not a
# committed migration history (the shadow DB fails with P3006). The backup makes
# `db push` safe-by-recovery without adopting a migration baseline.
set -eu

BACKUP_DIR="${BACKUP_DIR:-/backups}"

if [ -z "${DATABASE_URL:-}" ]; then
  echo "prod-entrypoint: DATABASE_URL is required" >&2
  exit 1
fi

# 1. Backup (best-effort; first boot has nothing to dump).
if command -v pg_dump >/dev/null 2>&1; then
  mkdir -p "$BACKUP_DIR"
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  backup_file="$BACKUP_DIR/idc_sync_${stamp}.sql.gz"
  echo "prod-entrypoint: backing up to $backup_file"
  if pg_dump "$DATABASE_URL" 2>/dev/null | gzip > "$backup_file"; then
    echo "prod-entrypoint: backup written ($(wc -c < "$backup_file") bytes)"
    # Prune backups older than 14 days so the volume does not grow unbounded.
    find "$BACKUP_DIR" -name 'idc_sync_*.sql.gz' -mtime +14 -delete 2>/dev/null || true
  else
    echo "prod-entrypoint: pg_dump produced no backup (first boot or empty DB); continuing" >&2
    rm -f "$backup_file" 2>/dev/null || true
  fi
else
  echo "prod-entrypoint: pg_dump not found in image; skipping backup" >&2
fi

# 2. Schema sync (no --accept-data-loss).
pnpm prisma generate
pnpm prisma db push

# 3. Raw-SQL constraints/triggers.
psql "$DATABASE_URL" -f prisma/init-custom-sql.sql

# 4. Start.
# --options is REQUIRED so fastify-cli applies the exported `options` object
# from app.ts (notably `trustProxy: true`). Without it, behind nginx every
# request looks like 127.0.0.1 and the per-IP rate limiter shares one bucket.
exec pnpm exec fastify start --options -a 0.0.0.0 -p 3161 -l info dist/app/app.js
