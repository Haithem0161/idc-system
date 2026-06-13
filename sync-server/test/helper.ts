// Phase-09 test bootstrap. Side effects MUST run before any other module
// (especially anything that calls `process.loadEnvFile()` at top-level and
// pulls DATABASE_URL out of `.env`).

// Pre-flight: load .env ourselves so we can scrub the vars before
// any plugin sees them. Tests target the MemorySyncStore path.
try {
  process.loadEnvFile?.()
} catch { /* no .env present is fine */ }
delete process.env.DATABASE_URL
// Scrub the first-launch bootstrap vars. If they leak from .env, the
// auth-services plugin fire-and-forgets bootstrapSuperadmin on every app
// build (auth-services.ts), racing each test's own explicit bootstrap; the
// loser throws "users already exist" and a DIFFERENT, order-dependent set of
// auth tests fails on each run.
delete process.env.BOOTSTRAP_SUPERADMIN_EMAIL
delete process.env.BOOTSTRAP_SUPERADMIN_PASSWORD
delete process.env.BOOTSTRAP_TENANT_ID
if (!process.env.JWT_SECRET || process.env.JWT_SECRET.length < 32) {
  process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'
}
if (process.env.NODE_ENV === 'production') {
  delete process.env.NODE_ENV
}

// This file contains code that we reuse between our tests.
import * as test from 'node:test'
import Fastify, { type FastifyInstance } from 'fastify'
import fp from 'fastify-plugin'
import { TypeBoxValidatorCompiler } from '@fastify/type-provider-typebox'
import { app } from '../src/app/app.js'

// Already scrubbed env at top-of-file. Re-scrub after import-time side effects
// in case a plugin auto-loads .env.
delete process.env.DATABASE_URL
delete process.env.BOOTSTRAP_SUPERADMIN_EMAIL
delete process.env.BOOTSTRAP_SUPERADMIN_PASSWORD
delete process.env.BOOTSTRAP_TENANT_ID

export type TestContext = {
  after: typeof test.after
}

// Build the app via fastify-plugin so decorators are exposed for tests
// (production setup uses `skipOverride: false`).
async function build (t: TestContext): Promise<FastifyInstance> {
  const fastify = Fastify({
    logger: { level: 'fatal' },
  })
  fastify.setValidatorCompiler(TypeBoxValidatorCompiler)

  await fastify.register(fp(app, { name: 'app-test-bootstrap' }), {
    skipOverride: true,
  } as Parameters<typeof app>[1])

  await fastify.ready()

  // Tear down our app after we are done
  t.after(() => void fastify.close())

  return fastify
}

function config (): { skipOverride: true } {
  return { skipOverride: true }
}

export {
  config,
  build,
}
