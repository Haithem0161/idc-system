import { Type, type Static } from '@sinclair/typebox'

export const PushOpSchema = Type.Object({
  op_id: Type.String({ minLength: 1 }),
  entity: Type.String({ minLength: 1 }),
  entity_id: Type.String({ minLength: 1 }),
  op: Type.Literal('upsert'),
  payload_b64: Type.String({ minLength: 1 }),
})

export const PushBodySchema = Type.Object({
  ops: Type.Array(PushOpSchema, { minItems: 1, maxItems: 200 }),
})

export const PushResponseSchema = Type.Object({
  accepted: Type.Array(
    Type.Object({
      op_id: Type.String(),
      status: Type.Union([Type.Literal('applied'), Type.Literal('duplicate')]),
    })
  ),
  conflicts: Type.Array(
    Type.Object({
      op_id: Type.String(),
      entity: Type.String(),
      entity_id: Type.String(),
      server_payload: Type.Unknown(),
      local_payload: Type.Unknown(),
      reason: Type.String(),
    })
  ),
})

export type PushBody = Static<typeof PushBodySchema>
export type PushResponse = Static<typeof PushResponseSchema>
