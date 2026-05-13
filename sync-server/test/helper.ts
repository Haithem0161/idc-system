// Phase-09 test bootstrap. Side effects MUST run before any other module
// (especially fastify-cli's start.js, which calls `process.loadEnvFile()`
// at top-level and pulls DATABASE_URL out of `.env`).

// Pre-flight: load .env ourselves so we can scrub the vars before
// fastify-cli sees them. Tests target the MemorySyncStore path.
try {
  process.loadEnvFile?.()
} catch { /* no .env present is fine */ }
delete process.env.DATABASE_URL
if (!process.env.JWT_SECRET || process.env.JWT_SECRET.length < 32) {
  process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'
}
if (process.env.NODE_ENV === 'production') {
  delete process.env.NODE_ENV
}

// This file contains code that we reuse between our tests.
import * as path from 'node:path'
import * as test from 'node:test'

// Phase-09 auth-jwt rewrite: HS256 dev fallback requires JWT_SECRET to be at
// least 32 characters. Set it before any plugin loads.
if (!process.env.JWT_SECRET || process.env.JWT_SECRET.length < 32) {
  process.env.JWT_SECRET = 'test-only-shared-secret-with-thirty-two-plus-characters'
}
if (process.env.NODE_ENV === 'production') {
  delete process.env.NODE_ENV
}

// Already scrubbed env at top-of-file. fastify-cli/start.js still calls
// `process.loadEnvFile()` at require-time which can re-populate
// DATABASE_URL from .env — scrub again afterwards.
const helper = require('fastify-cli/helper.js')
delete process.env.DATABASE_URL

export type TestContext = {
  after: typeof test.after
}

const AppPath = path.join(__dirname, '..', 'src', 'app', 'app.ts')

// Fill in this config with all the configurations
// needed for testing the application
function config () {
  return {
    skipOverride: true // Register our application with fastify-plugin
  }
}

// Automatically build and tear down our instance
async function build (t: TestContext) {
  // you can set all the options supported by the fastify CLI command
  const argv = [AppPath]

  // fastify-plugin ensures that all decorators
  // are exposed for testing purposes, this is
  // different from the production setup
  const app = await helper.build(argv, config())

  // Tear down our app after we are done
  // eslint-disable-next-line no-void
  t.after(() => void app.close())

  return app
}

export {
  config,
  build
}
