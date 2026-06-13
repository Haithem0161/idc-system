import { check } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"

import { isTauri } from "@/lib/ipc"

/// Result of an update check. `available` carries the version + a thunk that
/// downloads, installs, and relaunches into the new binary. `current` means the
/// app is already up to date. `unsupported` means the updater can't run here
/// (browser dev, or no `plugins.updater` endpoint is reachable yet) -- callers
/// render this as a neutral "updates unavailable" state, never an error.
export type UpdateCheck =
  | { kind: "available"; version: string; install: () => Promise<void> }
  | { kind: "current" }
  | { kind: "unsupported" }

/// The host placeholder baked into tauri.conf.json until a real release host is
/// provisioned (see docs/UPDATER-SETUP.md). When the endpoint still points at
/// it, `check()` would only fail DNS, so we short-circuit to `unsupported` to
/// keep the UI honest rather than surfacing a network error the operator can't
/// act on. Remove this guard once a real endpoint is configured.
const PLACEHOLDER_HOST = "RELEASES_HOST_TODO.invalid"

/// Query the update endpoint for a newer signed bundle.
///
/// Never throws for the "not configured" / "not in Tauri" cases -- those return
/// `unsupported`. A genuine transport/signature error from a *configured*
/// endpoint is re-thrown so the caller can show it.
export async function checkForUpdate (): Promise<UpdateCheck> {
  if (!isTauri()) return { kind: "unsupported" }

  let update
  try {
    update = await check()
  } catch (e) {
    // A DNS failure against the placeholder host is expected and not actionable.
    if (String(e).includes(PLACEHOLDER_HOST)) return { kind: "unsupported" }
    throw e
  }

  if (!update) return { kind: "current" }

  return {
    kind: "available",
    version: update.version,
    install: async () => {
      await update.downloadAndInstall()
      await relaunch()
    },
  }
}
