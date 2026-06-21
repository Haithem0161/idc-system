# IDC VPS Tasks — hand this to the Claude Code running on the VPS

Context: you are on the IDC production VPS (149.102.139.41). The desktop app is
offline-first; this box runs (a) the **sync server** behind nginx at
`idc-sync.madebyhaithem.com`, and (b) nginx serving release artifacts from
`/var/www/idc-updates` at `idc-release.madebyhaithem.com`. A round of
ship-readiness hardening + a new public download page just landed in the repo.
Three things need doing on the box. Do them in order; each has a verification
step — don't move on until it passes.

Standing rules on this box: **never** `docker rm` / `docker compose rm` / any
`docker ... prune` / `docker volume rm`. Use `docker compose up/restart/down`,
`docker exec`, `docker logs` only. Don't print secrets. Don't edit
`sync-server/.env` values blindly — read first, change only what's specified.

---

## Task 1 — Redeploy the hardened sync server (new deps + trustProxy)

What changed in the repo (already committed/pulled — `git pull` first if not):
- Added `@fastify/helmet` and `@fastify/rate-limit` (two new plugins:
  `src/app/plugins/helmet.ts`, `src/app/plugins/rate-limit.ts`).
- Per-route rate limits on `/auth/login` (5/min), `/auth/refresh` (20/min),
  `/auth/change-password` (10/min), `/sync/push` (60/min); global 300/min/IP.
- `trustProxy: true` is now exported from `src/app/app.ts` and applied via
  `fastify start --options` (the npm `start`/`dev:start` scripts were updated).
  This is REQUIRED for rate-limiting to work behind nginx — without it every
  client looks like nginx's 127.0.0.1 and shares one bucket.
- Swagger UI (`/documentation`) is now gated to **non-production** only.

Steps:
1. `cd` to the sync-server stack on the box, `git pull` the latest.
2. Because two npm packages were added, the anonymous `node_modules` volume is
   stale — recreate with `-V` (this is the documented "after pnpm add" path):
   ```
   docker compose -f docker-compose.yaml -f docker-compose.prod.yaml up -d --force-recreate -V sync-server
   ```
   (Use the SAME compose-file combo the box already uses for prod. The prod
   override binds 127.0.0.1:3161 and sets NODE_ENV=production.)
3. **Confirm `NODE_ENV=production` is actually in effect** (gates Swagger off +
   forces RS256, not the HS256 dev fallback):
   ```
   docker exec idc-sync-server env | grep -E '^NODE_ENV='
   ```
   It must print `NODE_ENV=production`. If it prints `development` or nothing,
   the `.env` alongside the prod compose is wrong — stop and report; do not
   "fix" it by guessing keys.

Verify (all from the box, hitting the local bind so nginx isn't in the way):
- Health: `curl -s http://127.0.0.1:3161/healthz` → `{"status":"ok",...}`.
- Security headers present:
  `curl -sI http://127.0.0.1:3161/healthz | grep -iE 'x-frame-options|x-content-type-options'`
  → should show `X-Frame-Options: DENY` and `X-Content-Type-Options: nosniff`.
- Swagger is OFF in prod:
  `curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:3161/documentation`
  → should be `404` (NOT 200).
- Rate-limit is live and keys on the real client IP through nginx. Hit the
  PUBLIC host so `X-Forwarded-For` is set by nginx, and fire >5 logins fast:
  ```
  for i in $(seq 1 8); do
    curl -s -o /dev/null -w '%{http_code} ' \
      -X POST https://idc-sync.madebyhaithem.com/auth/login \
      -H 'content-type: application/json' \
      --data '{"email":"x@y.z","password":"nope"}'
  done; echo
  ```
  You should see some `401`s then `429`s appear once 5/min is exceeded. If you
  see all `401` and never `429`, trustProxy/`--options` did not take effect —
  check `docker exec idc-sync-server cat package.json | grep '"start"'` shows
  `fastify start --options ...`, then restart.

  NOTE: nginx must forward the client IP for the per-IP limit to be correct. The
  `location /` proxy block for `idc-sync.madebyhaithem.com` should include:
  ```
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_set_header X-Real-IP $remote_addr;
  ```
  If those are missing, add them and `nginx -t && systemctl reload nginx`.

---

## Task 2 — Stand up the download subdomain `idc-download.madebyhaithem.com`

The sync server now serves a self-contained public download page at
`GET /download` (route `src/app/routes/download.ts`). It needs a DNS record + an
nginx vhost that reverse-proxies the subdomain ROOT to the server's `/download`.

1. DNS: ensure `idc-download.madebyhaithem.com` has an A record →
   `149.102.139.41` (same as the other idc subdomains). If you can't manage DNS
   from the box, report the exact record needed and stop at the nginx step.
2. nginx vhost (TLS via the existing certbot/Let's Encrypt setup the other
   subdomains use). The site root proxies to the sync server's `/download`:
   ```nginx
   server {
       server_name idc-download.madebyhaithem.com;

       location = / {
           proxy_pass http://127.0.0.1:3161/download;
           proxy_set_header Host $host;
           proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
           proxy_set_header X-Forwarded-Proto $scheme;
       }
       location = /download {
           proxy_pass http://127.0.0.1:3161/download;
           proxy_set_header Host $host;
           proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
           proxy_set_header X-Forwarded-Proto $scheme;
       }
       # everything else 404s (this host only serves the download page)
       location / { return 404; }

       listen 443 ssl; # managed by certbot
       # ssl_certificate / ssl_certificate_key lines added by certbot
   }
   ```
   Then issue/extend the cert (mirror however the other idc vhosts got theirs,
   e.g. `certbot --nginx -d idc-download.madebyhaithem.com`), `nginx -t`,
   `systemctl reload nginx`.

Verify:
- `curl -sI https://idc-download.madebyhaithem.com/ | head -1` → `200`.
- The body is the page, not JSON:
  `curl -s https://idc-download.madebyhaithem.com/ | grep -c 'Download the desktop app'` → `1`.
- It carries the page CSP:
  `curl -sI https://idc-download.madebyhaithem.com/ | grep -i content-security-policy`
  → a header containing `default-src 'none'` and `script-src 'nonce-...'`.

The page fetches the live release manifests client-side from
`idc-release.madebyhaithem.com`, so until Task 3's files exist the cards show
"Not available yet" — that's expected, not a bug.

---

## Task 3 — Make the releases host serve the new first-time-installer files

The release pipeline was extended to publish, per platform, into
`/var/www/idc-updates/idc/<target>/x86_64/`:
- `latest.json` — the Tauri updater manifest (already served; unchanged).
- **`install.json`** — NEW: the download page reads this for the human installer
  link. `{ version, platforms: { <key>: { url, name } } }`.
- The **first-time installer** binary: Windows `*-setup.exe` (NEW), Linux
  `*.AppImage` (already there — it's both the updater bundle and the installer).

These land automatically on the NEXT tagged release (CI → `ci-deploy-release.sh`
→ rrsync into the docroot). Your job is to make sure nginx will serve them with
the right behavior. The existing `idc-release.madebyhaithem.com` vhost almost
certainly already serves the whole `/var/www/idc-updates` tree as static files,
so usually NOTHING needs to change — but verify:

1. Confirm the release vhost serves arbitrary files from the docroot (it must,
   since it already serves `latest.json` + the AppImage). Check the `root` is
   `/var/www/idc-updates` and there's a `location /` (or `/idc/`) serving files.
2. Make sure `.exe` and `.json` are delivered as downloads / correct types and
   are NOT blocked by any `location ~ \.(exe)$ { deny }`-style rule. A safe
   addition if you want explicit control:
   ```nginx
   location ~* \.(exe|AppImage)$ {
       add_header Content-Disposition "attachment";
       add_header Access-Control-Allow-Origin "*";   # page fetch()es from the download subdomain
   }
   location ~* \.json$ {
       add_header Access-Control-Allow-Origin "*";    # install.json/latest.json are cross-origin fetched
       default_type application/json;
   }
   ```
   The `Access-Control-Allow-Origin: *` matters: the download page on
   `idc-download.…` does a cross-origin `fetch()` of `install.json`/`latest.json`
   on `idc-release.…`. Without CORS the manifests won't load and every card
   stays "Not available yet". (The binaries themselves are plain navigations, so
   they don't strictly need CORS, but it's harmless.)
3. `nginx -t && systemctl reload nginx`.

Verify (after the next release deploys; if no release has run since this change,
just confirm the CORS/serving config and note that the files appear post-release):
- `curl -sI https://idc-release.madebyhaithem.com/idc/linux/x86_64/install.json | grep -iE 'HTTP/|access-control-allow-origin'`
  → `200` + `Access-Control-Allow-Origin: *`.
- `curl -s https://idc-release.madebyhaithem.com/idc/linux/x86_64/install.json | head -c 200`
  → JSON with a `platforms.linux-x86_64.url` ending in `.AppImage`.
- For Windows (after a release built it):
  `curl -s https://idc-release.madebyhaithem.com/idc/windows/x86_64/install.json`
  → `platforms.windows-x86_64.url` ending in `-setup.exe`, and
  `curl -sI <that url>` → `200`.
- Finally, load `https://idc-download.madebyhaithem.com/` in a browser (or
  `curl` won't run the JS) and confirm the Linux/Windows cards show a version +
  an enabled Download button.

---

## Report back

When done, report for each task: pass/fail of every verify step, the exact
`NODE_ENV` value you saw, whether nginx already had `X-Forwarded-For` forwarding
+ the release-host CORS rules (or you added them), and anything that looked off
(e.g. a vhost that 404s, a wrong `root`, missing cert). If `NODE_ENV` was not
`production`, do NOT improvise `.env` — report it and wait.
