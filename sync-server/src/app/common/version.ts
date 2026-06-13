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
