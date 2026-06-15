#!/usr/bin/env bash
# Phase-10 live two-device round-trip gate.
#
# Stands up the REAL sync-server against REAL Postgres (RS256, NODE_ENV=production)
# and drives two simulated devices through the full sync round trip over HTTP:
# pull fan-out, the Manual-policy conflict round trip (park + resolve), schema
# negotiation, auth subject binding, merged re-validation, and metrics.
#
# Unlike the in-memory unit suite, this proves the offline-first multi-device
# promise against the actual Prisma persistence path -- the thing the desktop
# app's sync engine talks to. It is NOT part of `pnpm test` (it needs Docker +
# Postgres); run it explicitly before a release:
#
#   ./tools/run-roundtrip-e2e.sh
#
# Requirements: docker, openssl, node 20+, pnpm. Leaves the Postgres container
# running (reuse-friendly); pass --teardown to stop it afterwards.
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE"

DB_URL='postgresql://postgres:postgres@localhost:5449/idc_sync'
KEY_DIR="$(mktemp -d)"
SERVER_PID=""
TEARDOWN_DB=0
[ "${1:-}" = "--teardown" ] && TEARDOWN_DB=1

cleanup () {
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$KEY_DIR"
  if [ "$TEARDOWN_DB" = "1" ]; then
    docker compose stop sync-db >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "==> Starting Postgres (sync-db)"
docker compose up -d sync-db >/dev/null
# Wait for healthy.
for i in $(seq 1 30); do
  if docker exec idc-sync-db pg_isready -U postgres -d idc_sync >/dev/null 2>&1; then break; fi
  sleep 1
done

echo "==> Generating an ephemeral RS256 keypair"
openssl genrsa -out "$KEY_DIR/private.pem" 2048 2>/dev/null
openssl rsa -in "$KEY_DIR/private.pem" -pubout -out "$KEY_DIR/public.pem" 2>/dev/null

echo "==> Building server + applying schema"
pnpm build:ts >/dev/null
DATABASE_URL="$DB_URL" pnpm prisma generate >/dev/null 2>&1
DATABASE_URL="$DB_URL" pnpm prisma db push >/dev/null 2>&1
docker exec -i idc-sync-db psql -U postgres -d idc_sync < prisma/init-custom-sql.sql >/dev/null 2>&1 || true

echo "==> Starting the sync-server (NODE_ENV=production, RS256)"
NODE_ENV=production \
DATABASE_URL="$DB_URL" \
HOST=0.0.0.0 PORT=3161 \
JWT_PUBLIC_KEY="$(cat "$KEY_DIR/public.pem")" \
JWT_PRIVATE_KEY="$(cat "$KEY_DIR/private.pem")" \
BOOTSTRAP_SUPERADMIN_EMAIL='admin@idc.local' \
BOOTSTRAP_SUPERADMIN_PASSWORD='hunter22pw' \
BOOTSTRAP_TENANT_ID='clinic-1' \
DEFAULT_ENTITY_ID='clinic-1' \
METRICS_TOKEN='metrics-secret' \
  npx fastify start -a 0.0.0.0 -p 3161 -l warn dist/app/app.js >/tmp/idc-roundtrip-server.log 2>&1 &
SERVER_PID=$!

echo "==> Waiting for /healthz"
for i in $(seq 1 30); do
  if curl -sf -o /dev/null http://localhost:3161/healthz; then break; fi
  sleep 1
done

echo "==> Running the round-trip checks"
JWT_PRIVATE_KEY_PATH="$KEY_DIR/private.pem" node "$HERE/test/e2e/roundtrip.mjs"
