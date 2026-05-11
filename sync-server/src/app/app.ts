import { join } from 'node:path'
import AutoLoad, { AutoloadPluginOptions } from '@fastify/autoload'
import { FastifyPluginAsync, FastifyServerOptions } from 'fastify'
import { TypeBoxValidatorCompiler } from '@fastify/type-provider-typebox'

export interface AppOptions extends FastifyServerOptions, Partial<AutoloadPluginOptions> {
}

const options: AppOptions = {}

const app: FastifyPluginAsync<AppOptions> = async (
  fastify,
  opts
): Promise<void> => {
  fastify.setValidatorCompiler(TypeBoxValidatorCompiler)

  // Shared plugins (auth, swagger, errors, service wiring).
  // eslint-disable-next-line no-void
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'plugins'),
    options: opts,
  })

  // Global routes (/healthz, /).
  // eslint-disable-next-line no-void
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'routes'),
    options: opts,
  })

  // Sync bounded context routes.
  // eslint-disable-next-line no-void
  void fastify.register(AutoLoad, {
    dir: join(__dirname, 'sync', 'routes'),
    options: opts,
  })
}

export default app
export { app, options }
