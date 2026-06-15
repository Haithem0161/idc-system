// Phase-10 two-binary multi-device E2E harness.
//
// The base wdio.conf.ts spins ONE tauri-driver against ONE binary instance --
// that is "device A", driven by the global `browser` from @wdio/globals. To
// prove the offline-first fan-out and conflict invariants we need a SECOND,
// independently-stored desktop instance ("device B"). This module spins it up.
//
// Isolation is the load-bearing detail. Two binaries sharing one app-data
// directory would share one SQLite file and one pinned-key file -- they would
// be a single logical device, and the test would prove nothing. On Linux Tauri
// resolves `app_data_dir()` under $XDG_DATA_HOME (then $HOME/.local/share), and
// the app stores its DB at `<app_data_dir>/com.idc.system/idc-local.db` (see
// src-tauri/src/bin/seed_weekly.rs for the canonical path). So we give device B
// its own XDG_DATA_HOME via a temp dir passed to its tauri-driver child. Each
// device therefore gets its own DB, its own outbox, and its own pinned key --
// exactly like two physical workstations.
//
// Lifecycle per device B:
//   1. Spawn a dedicated `tauri-driver` on a private port with a private
//      XDG_DATA_HOME (and XDG_CONFIG_HOME) in the child's env.
//   2. Open a WebdriverIO `remote()` session against that driver, pointed at
//      the same debug binary device A uses.
//   3. Hand the caller the session; tear everything down in `stop()`.
//
// This only RUNS on a machine with a display + tauri-driver + the platform
// webview driver (webkit2gtk-driver on Linux). The specs that use it are gated
// by RUN_FULL_E2E=true && MULTI_DEVICE=true so the default CI path skips them.

import os from "node:os"
import path from "node:path"
import fs from "node:fs"
import net from "node:net"
import { spawn, type ChildProcess } from "node:child_process"
import { remote, type Browser } from "webdriverio"

const REPO_ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..", "..")
const BINARY = path.resolve(REPO_ROOT, "src-tauri", "target", "debug", "idc-system")
const TAURI_DRIVER = path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver")

export interface SecondDevice {
  /** The WebdriverIO session driving device B's webview. */
  readonly browser: Browser
  /** Isolated app-data root for device B (its SQLite + pinned key live here). */
  readonly dataDir: string
  /** Tear down the session, the driver, and the temp data dir. */
  stop(): Promise<void>
}

interface SpawnedDriver {
  readonly proc: ChildProcess
  readonly port: number
}

function waitForPort(host: string, port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs
  return new Promise((resolve, reject) => {
    const attempt = (): void => {
      const sock = net.connect({ host, port }, () => {
        sock.destroy()
        resolve()
      })
      sock.on("error", () => {
        sock.destroy()
        if (Date.now() > deadline) {
          reject(new Error(`tauri-driver did not open ${host}:${port} within ${timeoutMs}ms`))
        } else {
          setTimeout(attempt, 200)
        }
      })
    }
    attempt()
  })
}

/**
 * Start a second desktop instance with an isolated app-data directory.
 *
 * @param port  A private port for device B's tauri-driver (must differ from the
 *              base config's 4444). Defaults to 4445.
 */
export async function startSecondDevice(port = 4445): Promise<SecondDevice> {
  if (!fs.existsSync(BINARY)) {
    throw new Error(
      `device-B binary not found at ${BINARY}. Run \`pnpm tauri build --no-bundle --debug\` first.`,
    )
  }
  if (!fs.existsSync(TAURI_DRIVER)) {
    throw new Error(
      `tauri-driver not found at ${TAURI_DRIVER}. Install with \`cargo install tauri-driver\`.`,
    )
  }

  // Isolated XDG roots -> isolated SQLite + pinned key for device B.
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "idc-device-b-data-"))
  const configDir = fs.mkdtempSync(path.join(os.tmpdir(), "idc-device-b-config-"))

  const driver = await spawnDriver(port, dataDir, configDir)

  // WebdriverIO session against THIS driver (not the global :4444 one).
  const browser = await remote({
    hostname: "127.0.0.1",
    port,
    logLevel: "warn",
    capabilities: {
      // tauri-driver proxies to the platform webview driver; the application
      // path is how it knows which binary to launch.
      "tauri:options": { application: BINARY },
      // wdio requires a browserName; tauri-driver ignores the value.
      browserName: "wry",
    } as WebdriverIO.Capabilities,
  })

  return {
    browser,
    dataDir,
    async stop() {
      try {
        await browser.deleteSession()
      } catch {
        // session may already be gone; ignore
      }
      driver.proc.kill()
      for (const dir of [dataDir, configDir]) {
        try {
          fs.rmSync(dir, { recursive: true, force: true })
        } catch {
          // best-effort cleanup
        }
      }
    },
  }
}

function spawnDriver(port: number, dataDir: string, configDir: string): Promise<SpawnedDriver> {
  const proc = spawn(TAURI_DRIVER, ["--port", String(port)], {
    stdio: [null, "inherit", "inherit"],
    env: {
      ...process.env,
      // The two env vars that re-root Tauri's app_data_dir()/app_config_dir()
      // on Linux, giving device B its own SQLite + pinned-key file.
      XDG_DATA_HOME: dataDir,
      XDG_CONFIG_HOME: configDir,
    },
  })
  proc.on("error", (err) => {
    throw new Error(`failed to spawn tauri-driver for device B: ${err.message}`)
  })
  return waitForPort("127.0.0.1", port, 30_000).then(() => ({ proc, port }))
}
