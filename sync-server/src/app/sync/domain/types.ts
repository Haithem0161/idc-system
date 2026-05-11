/**
 * Shared types for the sync bounded context.
 *
 * The Phase-1 server only handles `audit_log` (additive-only). Schemas for
 * other entities are owned by their phase.
 */

export type EntityName = string

export interface PushOp {
  op_id: string
  entity: EntityName
  entity_id: string
  op: 'upsert'
  payload_b64: string
}

export interface AuditPayload {
  id: string
  actor_user_id: string
  action: string
  entity: string
  entity_id: string
  delta: Record<string, unknown>
  ip: string | null
  device_id: string
  at: string
  created_at: string
  updated_at: string
  deleted_at: string | null
  version: number
  last_synced_at: string | null
  origin_device_id: string | null
  entity_id_tenant: string
}

export interface ParkedConflict {
  opId: string
  entity: string
  entityId: string
  serverPayload: unknown
  localPayload: unknown
  reason: string
}

export interface ChangeRow {
  entity: EntityName
  entity_id: string
  payload: Record<string, unknown>
  updated_at: string
  version: number
}

export interface AcceptedOp {
  op_id: string
  status: 'applied' | 'duplicate'
}
