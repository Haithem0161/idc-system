import fp from 'fastify-plugin'
import cors from '@fastify/cors'
import type { FastifyInstance } from 'fastify'

/**
 * CORS with a strict origin allowlist (per `.claude/rules/sync-server.md`
 * Required Plugins). `@fastify/cors` was already a dependency but had never
 * been registered, leaving the server with no CORS policy.
 *
 * Allowed origins:
 *   - Tauri webview: `tauri://localhost` and `http://tauri.localhost`
 *     (the production desktop app's webview origin on macOS/Linux and Windows).
 *   - Local dev: `http://localhost:*` / `http://127.0.0.1:*` (Vite dev server).
 *   - Extra origins from `CORS_EXTRA_ORIGINS` (comma-separated) for staging.
 *
 * Requests with NO `Origin` header (the Tauri Rust HTTP client / curl / native
 * tooling -- which is how all sync traffic actually reaches the server) are
 * allowed: CORS is a browser-enforced policy and only meaningful for the
 * webview's own fetches (login form). Anything else is denied.
 */
const STATIC_ALLOWED = new Set<string>([
  'tauri://localhost',
  'http://tauri.localhost',
  'https://tauri.localhost',
])

function isLocalDevOrigin (origin: string): boolean {
  try {
    const { hostname, protocol } = new URL(origin)
    return (
      (protocol === 'http:' || protocol === 'https:') &&
      (hostname === 'localhost' || hostname === '127.0.0.1')
    )
  } catch {
    return false
  }
}

async function plugin (fastify: FastifyInstance): Promise<void> {
  const extra = (process.env.CORS_EXTRA_ORIGINS ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  for (const o of extra) STATIC_ALLOWED.add(o)

  await fastify.register(cors, {
    origin (origin, callback) {
      // No Origin header -> native/server-to-server caller (Tauri Rust client,
      // curl). Allow; CORS only governs browser-origin fetches.
      if (origin === undefined || origin === null || origin === '') {
        callback(null, true)
        return
      }
      if (STATIC_ALLOWED.has(origin) || isLocalDevOrigin(origin)) {
        callback(null, true)
        return
      }
      fastify.log.warn({ origin }, 'cors: rejected disallowed origin')
      callback(null, false)
    },
    credentials: true,
  })
}

export default fp(plugin, { name: 'cors' })
