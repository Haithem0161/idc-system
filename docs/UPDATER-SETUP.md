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

`{{target}}`, `{{arch}}`, `{{current_version}}` are substituted by the plugin.
The endpoint must return a signed update manifest (`latest.json`) or 204 when the
client is current.

## 3. Build + sign release bundles

Set the private key in the environment and run the release build; the bundler
signs the artifacts automatically:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.idc/updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<password or empty>"
pnpm tauri build
```

The build emits a `.sig` next to each bundle. The release pipeline (see the
separate "no release pipeline" build-health finding — `.github/workflows/`)
publishes the bundles + the `latest.json` manifest to the endpoint from step 2.

## 4. Host the update manifest

Serve a `latest.json` per platform/arch at the endpoint, e.g.:

```json
{
  "version": "0.2.0",
  "notes": "...",
  "pub_date": "2026-06-13T00:00:00Z",
  "platforms": {
    "linux-x86_64": {
      "signature": "<contents of the .sig file>",
      "url": "https://releases.example.com/idc/idc_0.2.0_amd64.AppImage"
    }
  }
}
```

GitHub Releases works as the host (point the endpoint at the release asset URL).

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

- [x] Plugins wired (`updater` + `process`) and capabilities granted.
- [x] Signing keypair generated; private key at `~/.idc/updater.key` (move to CI secrets).
- [x] `plugins.updater` block added with real pubkey (endpoint is a placeholder).
- [x] Frontend "Check for updates" action (Settings) + actionable 426 banner.
- [ ] Point `plugins.updater.endpoints` at a real host (replace `RELEASES_HOST_TODO.invalid`).
- [ ] Release pipeline builds, signs, and publishes bundles + `latest.json` to that host.
