import { decode as decodeMsgpack } from '@msgpack/msgpack'

import { DomainError } from '../../common/errors/domain'
import type { AuditPayload } from '../domain/types'

export function decodeAuditPayload (b64: string): AuditPayload {
  const obj = decodeRaw(b64)
  const required = ['id', 'actor_user_id', 'action', 'entity', 'entity_id', 'device_id', 'entity_id_tenant'] as const
  for (const key of required) {
    if (typeof obj[key] !== 'string') {
      throw new DomainError('VALIDATION_ERROR', `audit payload missing field: ${key}`, 422)
    }
  }
  return {
    id: String(obj.id),
    actor_user_id: String(obj.actor_user_id),
    action: String(obj.action),
    entity: String(obj.entity),
    entity_id: String(obj.entity_id),
    delta: (obj.delta as Record<string, unknown>) ?? {},
    ip: typeof obj.ip === 'string' ? obj.ip : null,
    device_id: String(obj.device_id),
    at: typeof obj.at === 'string' ? obj.at : new Date().toISOString(),
    created_at: typeof obj.created_at === 'string' ? obj.created_at : new Date().toISOString(),
    updated_at: typeof obj.updated_at === 'string' ? obj.updated_at : new Date().toISOString(),
    deleted_at: typeof obj.deleted_at === 'string' ? obj.deleted_at : null,
    version: typeof obj.version === 'number' ? obj.version : 1,
    last_synced_at: null,
    origin_device_id: typeof obj.origin_device_id === 'string' ? obj.origin_device_id : null,
    entity_id_tenant: String(obj.entity_id_tenant),
  }
}

export function decodeJsonPayload<T> (b64: string): T {
  const bytes = Buffer.from(b64, 'base64')
  try {
    const txt = bytes.toString('utf-8')
    return JSON.parse(txt) as T
  } catch (err) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'payload is not valid JSON',
      422,
      { reason: (err as Error).message }
    )
  }
}

function decodeRaw (b64: string): Record<string, unknown> {
  const bytes = Buffer.from(b64, 'base64')
  let raw: unknown
  try {
    raw = decodeMsgpack(bytes)
  } catch (err) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'payload is not valid MessagePack',
      422,
      { reason: (err as Error).message }
    )
  }
  if (!raw || typeof raw !== 'object') {
    throw new DomainError('VALIDATION_ERROR', 'payload root must be an object', 422)
  }
  return raw as Record<string, unknown>
}
