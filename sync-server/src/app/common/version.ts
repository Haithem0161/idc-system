import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

/**
 * Single source of truth for the server version. Reads it from `package.json`
 * at runtime instead of hardcoding a literal that drifts (healthz + swagger
 * previously both hardcoded '0.1.0' while package.json declared something else).
 *
 * Resolution strategy, in order:
 *   1. `npm_package_version` -- set by npm/pnpm when launched via a script.
 *   2. `package.json` walked up from this module's directory at runtime.
 *   3. '0.0.0' as a last-resort sentinel (never expected in practice).
 *
 * Read once at module load; the version does not change while the process runs.
 */
function resolveVersion (): string {
  const fromEnv = process.env.npm_package_version
  if (fromEnv && fromEnv.trim().length > 0) return fromEnv.trim()

  // Walk up from the compiled module location (dist/app/common/) until a
  // package.json with a version is found, or the filesystem root is reached.
  let dir = __dirname
  for (let i = 0; i < 6; i += 1) {
    try {
      const pkgPath = join(dir, 'package.json')
      const raw = readFileSync(pkgPath, 'utf-8')
      const parsed = JSON.parse(raw) as { version?: string }
      if (typeof parsed.version === 'string' && parsed.version.length > 0) {
        return parsed.version
      }
    } catch {
      // not at this level; keep walking up
    }
    const parent = dirname(dir)
    if (parent === dir) break
    dir = parent
  }
  return '0.0.0'
}

export const SERVER_VERSION: string = resolveVersion()

/**
 * The sync schema version the server speaks. Mirrors the desktop client's
 * `SYNC_SCHEMA_VERSION` (the local-migration count in
 * `src-tauri/src/db/migrations.rs`). Returned to clients in the pull response as
 * `server_schema_version` so a client can log/diagnose a schema drift even when
 * the `MIN_CLIENT_SCHEMA_VERSION` gate lets the request through (phase-10 T3).
 *
 * Bump this in lockstep whenever a client migration changes a synced column's
 * shape, and set `MIN_CLIENT_SCHEMA_VERSION` to enforce the floor.
 */
export const SERVER_SCHEMA_VERSION = 11
