import { Type, Static } from '@sinclair/typebox'

/**
 * Canonical error envelope (phase-01 §7.26 -- ErrorResponseSchema).
 *
 * Every route in this phase and later phases references this for its 4xx/5xx
 * responses. Domain error codes are listed in `error-handler.ts`.
 */
export const ErrorResponseSchema = Type.Object(
  {
    code: Type.String(),
    message: Type.String(),
    details: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
    traceId: Type.String(),
  },
  { $id: 'ErrorResponse' }
)

export type ErrorResponse = Static<typeof ErrorResponseSchema>
