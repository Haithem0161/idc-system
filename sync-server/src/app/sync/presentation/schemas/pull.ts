import { Type, type Static } from '@sinclair/typebox'

export const PullQuerySchema = Type.Object({
  since: Type.Optional(Type.String()),
  limit: Type.Optional(Type.Number({ minimum: 1, maximum: 500 })),
})

export const PullResponseSchema = Type.Object({
  changes: Type.Array(
    Type.Object({
      entity: Type.String(),
      entity_id: Type.String(),
      payload: Type.Record(Type.String(), Type.Unknown()),
      updated_at: Type.String(),
      version: Type.Number(),
    })
  ),
  next_cursor: Type.String(),
})

export type PullQuery = Static<typeof PullQuerySchema>
export type PullResponse = Static<typeof PullResponseSchema>
