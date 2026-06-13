# App self-updater setup (tauri-plugin-updater)

The desktop app ships with `tauri-plugin-updater` wired in (`src-tauri/src/lib.rs`)
and the `updater:default` capability granted (`src-tauri/capabilities/default.json`).
It is **inert until you provision a signing key and an update endpoint** — with no
`plugins.updater` config block, `check()` simply finds no update. This document is
the checklist to make it live. Nothing here is committed as a secret.

This pairs with the server-side version gate (`MIN_CLIENT_VERSION`, see the
`version-gate` plugin in `sync-server/`): the gate tells an outdated client to
upgrade (HTTP 426 → in-app banner); the updater is how that client actually pulls
the new binary.

## 1. Generate a signing keypair (once)

```bash
# From the repo root:
pnpm tauri signer generate -w ~/.idc/updater.key
```

This prints a **public key** and writes the **private key** to `~/.idc/updater.key`.
- The private key is a CREDENTIAL. Never commit it. Store it in the CI secret store.
- The optional password is set via `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## 2. Add the updater config to `src-tauri/tauri.conf.json`

Add a `plugins.updater` block with the public key from step 1 and your update
endpoint(s):

```jsonc
{
  "plugins": {
    "updater": {
      "pubkey": "<PUBLIC KEY FROM STEP 1>",
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

## 5. Frontend trigger

Once configured, call the updater from the frontend (the plugin's JS API is
already permitted by `updater:default`):

```ts
import { check } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"

const update = await check()
if (update) {
  await update.downloadAndInstall()
  await relaunch()
}
```

Wire this behind a "Check for updates" action in Settings, and/or trigger it
automatically when the server returns 426 (the app already shows the upgrade
banner on `app:upgrade_required`).

## Status

- [x] Plugin wired and capability granted (inert without config).
- [ ] Signing keypair generated and private key in CI secrets.
- [ ] `plugins.updater` block added with pubkey + endpoint.
- [ ] Release pipeline builds, signs, and publishes bundles + `latest.json`.
- [ ] Frontend "Check for updates" action / auto-check on 426.
