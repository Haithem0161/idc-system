import { join } from 'node:path'
import AutoLoad, { AutoloadPluginOptions } from '@fastify/autoload'
import { FastifyPluginAsync, FastifyServerOptions } from 'fastify'
import { TypeBoxValidatorCompiler } from '@fastify/type-provider-typebox'

export interface AppOptions extends FastifyServerOptions, Partial<AutoloadPluginOptions> {
}

// `trustProxy` makes Fastify derive `request.ip` from `X-Forwarded-For`. The
// server binds 127.0.0.1 only (docker-compose.prod.yaml), so the sole client is
// the local nginx reverse proxy -- without this, every request's ip would be
// nginx's 127.0.0.1 and the per-IP rate limiter would bucket the whole internet
// together. fastify-cli applies this only when `start` is run with `--options`
// (see package.json), which it is.
const options: AppOptions = {
  trustProxy: true,
}

const app: FastifyPluginAsync<AppOptions> = async (
  fastify,
  opts
): Promise<void> => {
  fastify.setValidatorCompiler(TypeBoxValidatorCompiler)

  // Shared plugins (auth, swagger, errors, service wiring).
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'plugins'),
    options: opts,
  })

  // Global routes (/healthz, /).
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'routes'),
    options: opts,
  })

  // Sync bounded context routes.
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'sync', 'routes'),
    options: opts,
  })

  // Auth bounded context routes.
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'auth', 'routes'),
    options: opts,
  })

  // Reports bounded context routes (phase-07).
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'domains', 'reports', 'routes'),
    options: opts,
  })

  // Audit bounded context routes (phase-08).
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'domains', 'audit', 'routes'),
    options: opts,
  })
}

export default app
export { app, options }
