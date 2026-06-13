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
host (steps 2–4 below) and the whole flow goes live with no further code change.

This pairs with the server-side version gate (`MIN_CLIENT_VERSION`, see the
`version-gate` plugin in `sync-server/`): the gate tells an outdated client to
upgrade (HTTP 426 → in-app banner); the updater is how that client actually pulls
the new binary.

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
        "https://releases.example.com/idc/{{target}}/{{arch}}/{{current_version}}"
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
