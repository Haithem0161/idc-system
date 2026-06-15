import fp from 'fastify-plugin'
import fastifyEnv from '@fastify/env'
import type { FastifyInstance } from 'fastify'

/**
 * Env validation via `@fastify/env`. Fails fast at boot when the production
 * invariants are violated (missing `DATABASE_URL` or `JWT_PUBLIC_KEY`).
 *
 * The schema below enumerates every var the server actually reads at runtime
 * — kept in sync with `.env.template` by the CI grep guardrail in
 * phase-09 §8 DoD.
 *
 * Authored per phase-09 SHIP-1 (env validation rewrite using the canonical
 * plugin instead of a hand-rolled `if (!DATABASE_URL)` check).
 */

const envSchema = {
  type: 'object',
  properties: {
    NODE_ENV: { type: 'string', default: 'development' },
    DATABASE_URL: { type: 'string', default: '' },
    REDIS_URL: { type: 'string', default: '' },
    JWT_PUBLIC_KEY: { type: 'string', default: '' },
    JWT_PRIVATE_KEY: { type: 'string', default: '' },
    JWT_SECRET: { type: 'string', default: '' },
    JWT_ACCESS_TTL_SECONDS: { type: 'string', default: '900' },
    JWT_REFRESH_TTL_SECONDS: { type: 'string', default: '2592000' },
    BOOTSTRAP_SUPERADMIN_EMAIL: { type: 'string', default: '' },
    BOOTSTRAP_SUPERADMIN_PASSWORD: { type: 'string', default: '' },
    BOOTSTRAP_TENANT_ID: { type: 'string', default: '' },
    METRICS_TOKEN: { type: 'string', default: '' },
    // Minimum client app version allowed to sync. A client whose
    // X-App-Version is below this is told to upgrade (426). Empty = no gate.
    MIN_CLIENT_VERSION: { type: 'string', default: '' },
  },
} as const

interface ConfigShape {
  NODE_ENV: string
  DATABASE_URL: string
  REDIS_URL: string
  JWT_PUBLIC_KEY: string
  JWT_PRIVATE_KEY: string
  JWT_SECRET: string
  JWT_ACCESS_TTL_SECONDS: string
  JWT_REFRESH_TTL_SECONDS: string
  BOOTSTRAP_SUPERADMIN_EMAIL: string
  BOOTSTRAP_SUPERADMIN_PASSWORD: string
  BOOTSTRAP_TENANT_ID: string
  METRICS_TOKEN: string
  MIN_CLIENT_VERSION: string
}

async function plugin (fastify: FastifyInstance): Promise<void> {
  await fastify.register(fastifyEnv, {
    schema: envSchema,
    confKey: 'config',
    // We already load .env ourselves in the entry path; skip duplicate work
    // and avoid `.env` polluting test runs that explicitly scrub vars.
    dotenv: false,
  })

  const cfg = fastify.config as ConfigShape
  const env = cfg.NODE_ENV
  const isProd = env === 'production'

  if (isProd) {
    const missing: string[] = []
    if (cfg.DATABASE_URL.trim().length === 0) {
      missing.push('DATABASE_URL')
    }
    if (cfg.JWT_PUBLIC_KEY.trim().length === 0) {
      missing.push('JWT_PUBLIC_KEY')
    }
    if (missing.length > 0) {
      throw new Error(
        `env plugin: missing required environment variables in production: ${missing.join(', ')}`
      )
    }
  } else if (cfg.DATABASE_URL.trim().length === 0) {
    fastify.log.warn(
      'DATABASE_URL is not set. Prisma plugin will not connect and routes will fall back to the in-memory store.'
    )
  }

  fastify.decorate('appEnv', {
    nodeEnv: env,
    isProduction: isProd,
    databaseUrl: cfg.DATABASE_URL.length > 0 ? cfg.DATABASE_URL : null,
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
    config: ConfigShape
  }
}
