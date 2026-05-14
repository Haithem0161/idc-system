// Phase-01 §4 WebdriverIO config for the Tauri binary.
//
// On Linux this needs `webkit2gtk-driver` installed (Ubuntu/Debian:
// `sudo apt install webkit2gtk-driver`). On Windows it needs Edge's
// WebView2 + msedgedriver-tool. tauri-driver is the cross-platform
// proxy in front of those.

import os from 'node:os'
import path from 'node:path'
import { spawn, type ChildProcess } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const __dirname = fileURLToPath(new URL('.', import.meta.url))
const repoRoot = path.resolve(__dirname, '..')
const application = path.resolve(repoRoot, 'src-tauri', 'target', 'debug', 'torch-app-template')

let tauriDriver: ChildProcess | undefined
let exiting = false

function closeTauriDriver(): void {
  exiting = true
  tauriDriver?.kill()
}

function onShutdown(fn: () => void): void {
  const cleanup = (): void => {
    try {
      fn()
    } finally {
      process.exit()
    }
  }
  process.on('exit', cleanup)
  process.on('SIGINT', cleanup)
  process.on('SIGTERM', cleanup)
  process.on('SIGHUP', cleanup)
  process.on('SIGBREAK', cleanup)
}

onShutdown(() => {
  closeTauriDriver()
})

export const config = {
  runner: 'local' as const,
  hostname: '127.0.0.1',
  port: 4444,
  specs: [path.join(__dirname, 'specs', '**', '*.spec.ts')],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application,
      },
    },
  ],
  logLevel: 'info' as const,
  bail: 0,
  baseUrl: 'tauri://localhost',
  waitforTimeout: 10_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,
  framework: 'mocha' as const,
  reporters: ['spec'] as const,
  mochaOpts: {
    ui: 'bdd' as const,
    timeout: 60_000,
  },

  // Phase-01: the binary is expected to exist. CI / local dev runs
  // `pnpm tauri build --no-bundle` before invoking wdio so the binary
  // is up to date; we do NOT rebuild on every wdio run because that
  // makes the developer feedback loop intolerable.
  onPrepare(): void {
    // intentionally empty -- run `pnpm tauri build --no-bundle` first.
  },

  beforeSession(): void {
    const driverPath = path.resolve(os.homedir(), '.cargo', 'bin', 'tauri-driver')
    tauriDriver = spawn(driverPath, [], { stdio: [null, process.stdout, process.stderr] })
    tauriDriver.on('error', (error) => {
      console.error('tauri-driver error:', error)
      process.exit(1)
    })
    tauriDriver.on('exit', (code) => {
      if (!exiting) {
        console.error('tauri-driver exited with code:', code)
        process.exit(1)
      }
    })
  },

  afterSession(): void {
    closeTauriDriver()
  },
}
