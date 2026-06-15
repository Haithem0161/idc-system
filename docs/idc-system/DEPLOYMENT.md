# IDC System -- Deployment & Operations Runbook

The single source of truth for standing up the sync server, onboarding clinic
devices, and recovering from trouble. Everything here is grounded in the actual
files in this repo; where a file is the authority, it is linked.

The IDC system is **two deployables**:

| Deployable | Artifact | Where it runs |
|-|-|-|
| **Desktop app** | Signed Tauri bundle (AppImage / NSIS) shipped by the release CI | Each clinic workstation (Linux or Windows x86_64) |
| **Sync server** | `docker compose` stack (Fastify + Postgres) | One VPS, behind nginx on 443 |

The desktop app is the source of truth for the daily workflow. The server exists
for sync and backup only. A clinic can work all day with the server down; nothing
is lost -- the outbox replays on reconnect.

---

## Part 1 -- Sync server bring-up (VPS)

### 1.1 Prerequisites

- A Linux VPS with Docker + the Compose plugin.
- nginx (or equivalent) terminating TLS on 443 and reverse-proxying to
  `127.0.0.1:3161`. The server and Postgres bind to localhost only
  ([docker-compose.prod.yaml](../../sync-server/docker-compose.prod.yaml) uses
  `ports: !override` to keep them off the public internet) -- nginx is the sole
  public entry.
- A DNS A record for the sync host (the default the desktop app ships with is
  `https://idc-sync.madebyhaithem.com`; see Part 2 for changing it).

### 1.2 Generate the RS256 keypair

Production auth is **RS256**: the server signs JWTs with the private key, the
desktop app verifies them offline with the public key (pinned TOFU on first run).
HS256 is refused at boot in production ([auth-jwt.ts](../../sync-server/src/app/plugins/auth-jwt.ts)
only allows the `JWT_SECRET` fallback when `NODE_ENV` is `development`/`test`/unset).

```bash
openssl genrsa -out jwt_private.pem 2048
openssl rsa -in jwt_private.pem -pubout -out jwt_public.pem
```

Keep `jwt_private.pem` secret and backed up offline. If it is ever lost, every
installed desktop app must re-pin the new public key (see 4.4).

### 1.3 Fill `.env`

From `sync-server/`, copy the template and fill it
([.env.template](../../sync-server/.env.template) documents every field):

```bash
cp .env.template .env
```

The production-critical values:

| Var | Production value |
|-|-|
| `NODE_ENV` | `production` |
| `POSTGRES_PASSWORD` | a strong random secret (NOT the dev `postgres`) |
| `DATABASE_URL` | `postgresql://postgres:<POSTGRES_PASSWORD>@sync-db:5432/idc_sync` -- host is the compose service name `sync-db`, internal port `5432` |
| `JWT_PUBLIC_KEY` | full PEM of `jwt_public.pem` (BEGIN/END lines included; multi-line is fine in an env_file) |
| `JWT_PRIVATE_KEY` | full PEM of `jwt_private.pem` |
| `JWT_SECRET` | leave **empty** -- a non-empty value in production is ignored by design, keep it empty to avoid confusion |
| `BOOTSTRAP_SUPERADMIN_EMAIL` / `_PASSWORD` | the first superadmin's credentials (used once; see note below) |
| `BOOTSTRAP_TENANT_ID` | the clinic's tenant id, e.g. `clinic-1`. This is the `entityId` every row is scoped to |
| `DEFAULT_ENTITY_ID` | same as `BOOTSTRAP_TENANT_ID` |
| `METRICS_TOKEN` | a random secret; `/metrics` 404s without it |
| `MIN_CLIENT_VERSION` | empty at first launch. Set to the oldest desktop version you want to allow once you have multiple versions in the field (see 4.3) |
| `MIN_CLIENT_SCHEMA_VERSION` | empty at first launch. Bump only alongside a synced-column schema change (see 4.3) |

`.env` is **never committed** -- the
[preship guardrail](../../tools/preship-guardrails.sh) fails the build if it is
tracked.

> **Bootstrap vs. first-run.** You can seed the superadmin two ways, pick ONE:
> (a) set the `BOOTSTRAP_*` vars and the server creates the admin on boot, or
> (b) leave them empty and create the admin from the **desktop app's first-run
> screen** (Part 2.2). For a clinic the desktop first-run is friendlier; for an
> unattended/headless server the bootstrap vars are simpler. Don't do both for
> the same tenant -- the second is a no-op at best.

### 1.4 Start the stack

```bash
cd sync-server
# Inspect the merged config BEFORE starting -- catches a missing :? guard.
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml config
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml up -d
# Confirm the runtime env actually reached the container:
docker exec idc-sync-server env | sort | grep -E 'NODE_ENV|DATABASE_URL|JWT_PUBLIC'
```

On start, the prod entrypoint
([prod-entrypoint.sh](../../sync-server/tools/prod-entrypoint.sh)) runs, in order:
**pg_dump backup -> `prisma db push` (no `--accept-data-loss`) ->
init-custom-sql.sql -> start Fastify.** A destructive schema drift FAILS the boot
rather than dropping data.

> **Image note.** The prod stack builds from `Dockerfile.dev` (there is no
> separate prod Dockerfile). That is deliberate for a single-clinic deployment:
> the image already installs `postgresql-client` (so `pg_dump`/`psql` work) and
> the prod compose overrides the dangerous default `CMD` (which uses
> `--accept-data-loss`) with the safe `prod-entrypoint.sh` sequence. The dev
> name is cosmetic; the running behaviour is production-safe.

### 1.5 Verify health

```bash
curl -sf http://127.0.0.1:3161/healthz && echo OK
# Swagger (behind nginx auth or localhost only):
#   http://127.0.0.1:3161/documentation
# Metrics (needs the token):
curl -s -H "x-internal-token: $METRICS_TOKEN" http://127.0.0.1:3161/metrics | head
```

Then from a clinic machine over the public host: `curl -sf https://<sync-host>/healthz`.

### 1.6 Acceptance gate (run once, before handing machines over)

The live two-device round trip proves the whole contract against real Postgres:

```bash
cd sync-server
./tools/run-roundtrip-e2e.sh            # leaves Postgres up for re-runs
./tools/run-roundtrip-e2e.sh --teardown # stops Postgres after
```

This is NOT part of `pnpm test` (it needs Docker). It must print
`RESULT: 20 passed, 0 failed` before a release is considered installable.

---

## Part 2 -- Desktop app: install & first-run provisioning

### 2.1 Install

Clinics receive the signed bundle from the updater host (the release CI publishes
AppImage for Linux and NSIS for Windows -- see [release.yml](../../.github/workflows/release.yml)
and [UPDATER-SETUP.md](../UPDATER-SETUP.md)). After the first install, the app
self-updates from the same host; no manual reinstall.

### 2.2 First device (creates the clinic)

On first launch with no local users, the app shows the **first-run screen**
([first-run.tsx](../../src/pages/auth/first-run.tsx)). The operator enters:

- **Admin email / name / password** -- becomes the superadmin.
- **Tenant ID** -- the clinic's `entityId`. **Must match the server's
  `BOOTSTRAP_TENANT_ID` / `DEFAULT_ENTITY_ID`.** Leave blank only for an
  unscoped single-tenant install.
- **Sync server URL** -- defaults to the shipped production host; change it here
  for staging/self-hosted.

On submit the app: saves the sync URL, **pins the server's RS256 public key (TOFU)**
for offline JWT verification, then creates the admin. Key-pinning is best-effort --
a network hiccup doesn't block first-run; it re-pins on the next online action.

### 2.3 Additional devices (device #2, #3, ...)

A second workstation launches, has no sync URL, and shows the **first-launch
setup modal** ([first-launch-setup.tsx](../../src/components/setup/first-launch-setup.tsx)):
it captures the **sync server URL** and pins the server key. The operator then
**logs in** with an existing user's credentials (created on the server / first
device) -- there is no separate "join" step. After login the pull loop fans the
clinic's existing data down to the new device.

> So the onboarding contract is: **device 1 creates the admin + tenant; every
> later device just points at the same URL and logs in.** The tenant scope lives
> in the user's JWT (`entityId`); the device inherits it on login.

### 2.4 Offline behaviour (what to expect)

Per [auth.md](../../.claude/rules/auth.md): a device that logged in once works
fully offline -- reads hit local SQLite, writes commit locally and queue in the
outbox. The cached refresh token (verified offline against the pinned key) keeps
the session alive until it expires (30d). Sync resumes automatically on reconnect.

---

## Part 3 -- Releasing a new desktop version

One command, run by the maintainer locally (see [UPDATER-SETUP.md](../UPDATER-SETUP.md)
for the full prerequisites and secrets):

```bash
pnpm release patch   # or minor | major
```

This bumps the three version fields in lockstep (package.json, tauri.conf.json,
Cargo.toml), commits, tags `vX.Y.Z`, and pushes the tag. The tag push triggers
[release.yml](../../.github/workflows/release.yml): it builds + **signs** the
updater bundles for Linux and Windows, writes per-platform `latest.json`, and
rsyncs everything to the VPS docroot. Installed apps pick up the update on their
next updater check.

**Pre-release checklist:**
- `pnpm lint && pnpm build` green.
- `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` green.
- `cd sync-server && pnpm test` green.
- `bash tools/preship-guardrails.sh` green.
- `cd sync-server && ./tools/run-roundtrip-e2e.sh` -> 20/20 (only when the sync contract changed).

---

## Part 4 -- Operations

### 4.1 Logs & status

```bash
docker logs idc-sync-server --tail 200 -f
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml ps
```

The desktop app surfaces sync state in the status bar (the sync pill:
`idle | pushing | pulling | offline | error`). A device stuck `offline` while the
network is up usually means a wrong sync URL or an unpinned/rotated key.

### 4.2 Backups (Postgres)

The prod entrypoint writes a **gzipped `pg_dump` before every schema change** to
the `sync_db_backups` volume (mounted at `/backups`), pruning dumps older than 14
days. To take an ad-hoc backup or copy one off the box:

```bash
# Ad-hoc dump:
docker exec idc-sync-db pg_dump -U postgres idc_sync | gzip > idc_sync_$(date -u +%Y%m%dT%H%M%SZ).sql.gz
# List entrypoint-written backups:
docker exec idc-sync-server ls -la /backups
# Copy a backup to the host:
docker cp idc-sync-server:/backups/<file>.sql.gz ./
```

For real durability, schedule an **off-box** copy (e.g. nightly `rsync`/object
storage) of `/backups` -- the volume protects against a bad migration, not
against losing the VPS.

### 4.3 Client compatibility gates

Two env knobs reject too-old desktop clients with HTTP 426 (the app then prompts
to upgrade):

- `MIN_CLIENT_VERSION` -- minimum desktop app version (compares `X-App-Version`).
  Set this once you have multiple versions in the field and need to force an
  upgrade. A missing/garbage header **fails open** (allowed).
- `MIN_CLIENT_SCHEMA_VERSION` -- minimum client local-schema version
  (`X-Schema-Version`, the local-migration count). Bump this **only together with**
  `SERVER_SCHEMA_VERSION` in [version.ts](../../sync-server/src/app/common/version.ts)
  when a synced column's shape changes, so an old client can't push a payload
  missing a now-required column. Also fails open on a missing header.

Change either in `.env`, then `docker compose ... up -d` to restart with the new value.

### 4.4 RS256 key rotation

If the private key is compromised or lost:

1. Generate a new keypair (1.2).
2. Update `JWT_PUBLIC_KEY` / `JWT_PRIVATE_KEY` in `.env`, restart the stack.
3. **Every desktop device must re-pin the new public key.** The pin is TOFU; a
   key change means existing devices fail offline verification. Re-pinning happens
   automatically on the next successful online action against the new server, but
   a device that is offline at rotation time will reject the cached token until it
   reconnects. Plan rotation for a window when devices are online.

### 4.5 Restore from a `pg_dump` backup

A restore overwrites the database -- treat it as destructive and confirm the
target before running.

```bash
# 1. Stop the server so nothing writes mid-restore (leave Postgres up).
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml stop sync-server

# 2. Restore. The dump is plain SQL (gzipped); pipe it into psql.
gunzip -c idc_sync_<stamp>.sql.gz | docker exec -i idc-sync-db psql -U postgres -d idc_sync

# 3. Re-apply the raw-SQL constraints/triggers (the dump is data+schema, but
#    re-running this is idempotent and safe).
docker exec -i idc-sync-db psql -U postgres -d idc_sync < prisma/init-custom-sql.sql

# 4. Start the server again.
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml up -d sync-server
```

After a restore, clients pull the restored state on their next pull. Any work a
client did **after** the backup point but **before** the restore is still in that
client's local SQLite + outbox and will re-push on reconnect -- offline-first
means the client, not the server, is the safety net for recent work.

---

## Part 5 -- Quick reference

```bash
# --- Server (from sync-server/) ---
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml up -d        # start
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml restart      # restart
docker compose -f docker-compose.yaml -f docker-compose.prod.yaml stop         # stop (keeps data)
docker logs idc-sync-server --tail 200 -f                                      # logs
curl -sf http://127.0.0.1:3161/healthz                                         # health
./tools/run-roundtrip-e2e.sh                                                   # acceptance gate

# --- Release (from repo root) ---
pnpm release patch                                                             # cut + ship a release

# --- Backup / restore (see 4.2 / 4.5) ---
docker exec idc-sync-db pg_dump -U postgres idc_sync | gzip > backup.sql.gz    # ad-hoc backup
```

**NEVER** run `docker rm` / `docker compose rm` / any `prune` against this stack
(per [docker.md](../../.claude/rules/docker.md)) -- `down`/`stop` keep the data
volume; the prune family destroys it.
