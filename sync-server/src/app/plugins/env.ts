import fp from 'fastify-plugin'
import type { FastifyInstance } from 'fastify'

/**
 * Minimal env validation. Fails fast at boot when required vars are missing.
 *
 * Production (`NODE_ENV=production`):
 *   - `DATABASE_URL` MUST be set and non-empty (Prisma connection target).
 *   - One of `JWT_PUBLIC_KEY` (RS256) MUST be set; `JWT_SECRET` alone is not
 *     enough. The auth-jwt plugin enforces this; we surface the rule here.
 *
 * Non-production: any missing var is logged at `warn` level. Tests bootstrap
 * with `JWT_SECRET` and (when exercising Prisma paths) `DATABASE_URL`.
 *
 * Authored per phase-09 §5 to replace the runtime "fall through to a Prisma
 * connection error" failure mode.
 */
async function plugin (fastify: FastifyInstance): Promise<void> {
  const env = process.env.NODE_ENV ?? 'development'
  const isProd = env === 'production'

  const missing: string[] = []
  if (isProd) {
    if (!process.env.DATABASE_URL || process.env.DATABASE_URL.trim().length === 0) {
      missing.push('DATABASE_URL')
    }
    const hasPublic = process.env.JWT_PUBLIC_KEY && process.env.JWT_PUBLIC_KEY.trim().length > 0
    if (!hasPublic) {
      missing.push('JWT_PUBLIC_KEY')
    }
  } else if (!process.env.DATABASE_URL) {
    fastify.log.warn(
      'DATABASE_URL is not set. Prisma plugin will not connect and routes will fall back to the in-memory store.'
    )
  }

  if (missing.length > 0) {
    throw new Error(
      `env plugin: missing required environment variables in production: ${missing.join(', ')}`
    )
  }

  fastify.decorate('appEnv', {
    nodeEnv: env,
    isProduction: isProd,
    databaseUrl: process.env.DATABASE_URL ?? null,
  })
}

export interface AppEnv {
  nodeEnv: string
  isProduction: boolean
  databaseUrl: string | null
}

export default fp(plugin, { name: 'env' })

declare module 'fastify' {
  interface FastifyInstance {
    appEnv: AppEnv
  }
}
