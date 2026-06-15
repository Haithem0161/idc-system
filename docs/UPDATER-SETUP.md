# App self-updater setup (tauri-plugin-updater)

The desktop app is **wired end-to-end** for self-update:

- `tauri-plugin-updater` + `tauri-plugin-process` registered (`src-tauri/src/lib.rs`),
  with `updater:default` and `process:default` capabilities granted
  (`src-tauri/capabilities/default.json`).
- A real signing **public key** is configured in `src-tauri/tauri.conf.json`
  (`plugins.updater.pubkey`). The matching **private key was generated to
  `~/.idc/updater.key`** on the build machine — it is a credential and is NOT in
  the repo.
- The frontend trigger is live: a "Check for updates" action in **Settings → App
  updates** (`src/pages/admin/settings.tsx`) and a "Check for update" button on
  the 426 upgrade banner (`src/components/shell/app-shell.tsx`), both driven by
  `src/features/updater/`.

The **one remaining step is operational, not code**: the `endpoints` URL in
`tauri.conf.json` is still the placeholder `https://RELEASES_HOST_TODO.invalid/...`.
Until it points at a host that serves a signed manifest, `checkForUpdate()`
short-circuits to a neutral "updates not available" state (it recognises the
`.invalid` placeholder and does not surface a network error). Point it at a real
host and the whole flow goes live with no further code change.

This pairs with the server-side version gate (`MIN_CLIENT_VERSION`, see the
`version-gate` plugin in `sync-server/`): the gate tells an outdated client to
upgrade (HTTP 426 → in-app banner); the updater is how that client actually pulls
the new binary.

---

## Release flow: `pnpm release` + GitHub Actions (recommended)

The whole build → sign → deploy is automated. After the one-time setup below,
cutting a release is a single local command:

```bash
pnpm release patch   # 0.1.0 -> 0.1.1   (also: minor, major)
```

That runs [`tools/release.mjs`](../tools/release.mjs): it bumps the three version
fields (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) in
lockstep, refreshes `Cargo.lock`, commits `chore(release): vX.Y.Z`, tags it, and
pushes branch + tag **atomically**. The tag push triggers
[`.github/workflows/release.yml`](../.github/workflows/release.yml):

1. **build** (matrix: `ubuntu-22.04` + `windows-latest`) — `pnpm tauri build`
   restricted to the one updater bundle per OS (AppImage / NSIS), signed with the
   `TAURI_SIGNING_PRIVATE_KEY` secret. Asserts the tag == all three version
   fields and that the endpoint host is not the placeholder, then uploads each
   platform's bundle + `.sig` as an artifact.
2. **deploy** (once, serialized) — runs [`tools/ci-deploy-release.sh`](../tools/ci-deploy-release.sh),
   which writes a per-platform `latest.json` (signature read verbatim from the
   `.sig`, every field `jq`-escaped) and rsyncs bundle + manifest to the VPS over
   SSH — binary first, manifest last, so a client never sees a manifest pointing
   at a not-yet-uploaded binary.

The repo is **public**, so GitHub-hosted Linux + Windows runners are **free and
unmetered**. No macOS runner (clinics are Windows + Linux x86_64 only).

### One-time setup

**A. VPS — create a least-privilege deploy user that owns only the docroot:**

```bash
# on the VPS, as an admin
sudo adduser --disabled-password --gecos "" idcdeploy
sudo mkdir -p /var/www/idc-updates/idc
sudo chown -R idcdeploy:idcdeploy /var/www/idc-updates
sudo chmod -R 755 /var/www/idc-updates
sudo -u idcdeploy mkdir -p /home/idcdeploy/.ssh && sudo -u idcdeploy chmod 700 /home/idcdeploy/.ssh
```

Add the nginx `location /idc/` (see [`updater-nginx.conf.example`](./updater-nginx.conf.example))
and a TLS cert for the host (`certbot`). The updater refuses plain HTTP.

**B. Dedicated deploy SSH key (on your machine), authorize it on the VPS:**

```bash
ssh-keygen -t ed25519 -a 100 -N "" -C "idc-ci-deploy" -f ~/.ssh/idc_deploy_ed25519
# append the PUBLIC key to the deploy user, ideally restricted:
#   /home/idcdeploy/.ssh/authorized_keys  (one line)
#   restrict,command="rrsync -wo /var/www/idc-updates" ssh-ed25519 AAAA...idc_deploy_ed25519
```

**C. Set the real releases host** in `src-tauri/tauri.conf.json` (replace
`RELEASES_HOST_TODO.invalid`) and commit. CI fails the build if the placeholder
is still present, because the host is baked into the signed binary.

```jsonc
"endpoints": [ "https://<your-host>/idc/{{target}}/{{arch}}/latest.json" ]
```

**D. GitHub repository secrets** (Settings → Secrets and variables → Actions, or
`gh secret set`). Pipe key files via stdin to avoid paste/CRLF mangling:

| Secret | Contents | How |
|-|-|-|
| `TAURI_SIGNING_PRIVATE_KEY` | the minisign private key | `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.idc/updater.key` |
| `DEPLOY_SSH_KEY` | the deploy **private** key | `gh secret set DEPLOY_SSH_KEY < ~/.ssh/idc_deploy_ed25519` |
| `DEPLOY_KNOWN_HOSTS` | the VPS host key (pinned, MITM-safe) | `gh secret set DEPLOY_KNOWN_HOSTS < <(ssh-keyscan <your-host>)` — verify the fingerprint out-of-band first |
| `UPDATE_HOST` | releases host, no scheme | `gh secret set UPDATE_HOST --body "<your-host>"` |
| `DEPLOY_SSH_USER` | `idcdeploy` | `gh secret set DEPLOY_SSH_USER --body "idcdeploy"` |
| `DEPLOY_SSH_HOST` | VPS ip/hostname | `gh secret set DEPLOY_SSH_HOST --body "1.2.3.4"` |
| `DEPLOY_DOCROOT` | nginx docroot | `gh secret set DEPLOY_DOCROOT --body "/var/www/idc-updates"` |
| `DEPLOY_SSH_PORT` | ssh port (optional; defaults to 22) | `gh secret set DEPLOY_SSH_PORT --body "22"` |

The signing key has **no password**; the workflow sets
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ""` inline (it is non-sensitive), so no
empty-password secret is needed. `UPDATE_HOST` must equal the host you set in
step C and the nginx `server_name`.

After that, every release is just `pnpm release patch|minor|major`. Watch it in
the repo's **Actions** tab.

### Manual fallback (`tools/release-update.sh`)

If you ever need to publish without CI (or test locally), the manual script
below still works — it builds for the OS it runs on and scps to the VPS. The CI
path supersedes it for normal releases.

---

## 1. Signing keypair (DONE on this machine)

Already generated:

```bash
pnpm tauri signer generate -w ~/.idc/updater.key --password ""
```

- Private key: `~/.idc/updater.key` — a CREDENTIAL. Never commit it. Move it into
  the CI secret store (`TAURI_SIGNING_PRIVATE_KEY`) for release builds.
- Public key: already pasted into `plugins.updater.pubkey` in `tauri.conf.json`.
- To rotate, regenerate and replace the pubkey in `tauri.conf.json`.

## 2. Point the endpoint at a real host (`src-tauri/tauri.conf.json`)

Replace the placeholder host in the existing `plugins.updater.endpoints` entry:

```jsonc
{
  "plugins": {
    "updater": {
      "pubkey": "<already set — do not change unless rotating keys>",
      "endpoints": [
        "https://releases.example.com/idc/{{target}}/{{arch}}/latest.json"
      ]
    }
  }
}
```

`{{target}}` and `{{arch}}` are substituted by the plugin; the resulting URL is
a **static `latest.json`**. The plugin downloads it, compares its `version` to
the running app, and (if newer) downloads the bundle and verifies its signature
against the pubkey before installing. A static file host (nginx) is all you need
-- there is no dynamic server to write.

The endpoint MUST be HTTPS (the plugin refuses plain HTTP).

## 2b. VPS: serve the update files (one-time)

The desktop updates are static files; serve them from the same VPS as the
sync-server. On the VPS:

```bash
sudo mkdir -p /var/www/idc-updates/idc
# allow your deploy user to write into it (the release script scps here)
sudo chown -R "$USER" /var/www/idc-updates
```

Add an nginx `location /idc/` that serves `/var/www/idc-updates` -- a ready
example (dedicated subdomain OR a path next to the proxied sync-server) is in
[`updater-nginx.conf.example`](./updater-nginx.conf.example). Issue a TLS cert
for the host (`certbot`) and reload nginx. Verify:

```bash
curl -I https://<your-host>/idc/   # 403/404 is fine; TLS must be valid
```

Then set the host in `tauri.conf.json` (replace `RELEASES_HOST_TODO.invalid`):

```jsonc
"endpoints": [ "https://<your-host>/idc/{{target}}/{{arch}}/latest.json" ]
```

## 3 + 4. Build, sign, and publish a release (`tools/release-update.sh`)

`tools/release-update.sh` does the build + sign + manifest + upload in one shot.
It builds for the OS it RUNS on (Linux -> AppImage, Windows -> installer) --
Tauri cannot cross-build these, so run it once per target OS. Each run only
updates its own platform's directory, so Linux and Windows releases coexist.

```bash
# From the repo root, on each target OS:
UPDATE_HOST=<your-host> \
DEPLOY_SSH=deploy@<vps-ip> \
DEPLOY_DOCROOT=/var/www/idc-updates \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
./tools/release-update.sh
```

It reads the private key from `~/.idc/updater.key` (override with
`TAURI_SIGNING_PRIVATE_KEY`), reads the version from `tauri.conf.json`, writes
`latest.json`, and scps the bundle + manifest to
`<docroot>/idc/<target>/<arch>/` on the VPS.

**Release checklist:** bump `version` in `src-tauri/tauri.conf.json`, commit,
then run the script on a Linux box and (for clinic Windows PCs) on a Windows box.

The manifest it writes looks like:

```json
{
  "version": "0.2.0",
  "notes": "IDC System 0.2.0",
  "pub_date": "2026-06-13T00:00:00Z",
  "platforms": {
    "linux-x86_64": {
      "signature": "<contents of the .sig file>",
      "url": "https://<your-host>/idc/linux/x86_64/idc_0.2.0_amd64.AppImage"
    }
  }
}
```

## 5. Frontend trigger (DONE)

Already wired — no further code needed. The plugin JS API (`updater:default` +
`process:default`) is called through `src/features/updater/`:

- `updater.ts` — `checkForUpdate()` wraps `check()` + `relaunch()`, guards
  against running outside Tauri and against the placeholder host.
- `use-updater.ts` — `useUpdater()` drives the UI state machine (single in-flight
  check/install, terminal states).
- **Settings → App updates** (`src/pages/admin/settings.tsx`) exposes a manual
  "Check for updates" button that offers "Download and restart" when one is found.
- The **426 upgrade banner** (`src/components/shell/app-shell.tsx`) carries a
  "Check for update" button so the server's `app:upgrade_required` is actionable.

The underlying call is the standard pattern:

```ts
import { check } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"

const update = await check()
if (update) {
  await update.downloadAndInstall()
  await relaunch()
}
```

## Status

Done in code (no further changes needed):

- [x] Plugins wired (`updater` + `process`) and capabilities granted.
- [x] Signing keypair generated; private key at `~/.idc/updater.key`.
- [x] `plugins.updater` block added with real pubkey + static `latest.json` endpoint shape.
- [x] Frontend "Check for updates" action (Settings) + actionable 426 banner.
- [x] Release script (`tools/release-update.sh`) + nginx example (`updater-nginx.conf.example`).

Your operational steps (one-time, then per release):

- [ ] One-time: `mkdir /var/www/idc-updates`, add the nginx `/idc/` location, issue a TLS cert.
- [ ] One-time: replace `RELEASES_HOST_TODO.invalid` in `tauri.conf.json` with your host.
- [ ] Per release: bump `version`, run `tools/release-update.sh` on Linux and on Windows.
