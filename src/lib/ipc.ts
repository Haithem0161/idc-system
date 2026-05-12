import { invoke as tauriInvoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

/**
 * Typed wrapper around Tauri's `invoke<>()`.
 *
 * Every IPC command goes through this helper so the TypeScript compiler can
 * enforce arg shape and return shape from the command map below.
 */
export type CommandMap = {
  sync_status: { args: void; result: SyncStatusSnapshot }
  sync_outbox_count: { args: void; result: number }
  sync_trigger_push: { args: void; result: null }
  sync_trigger_pull: { args: void; result: null }
  sync_list_conflicts: {
    args: { limit?: number; offset?: number }
    result: ConflictRecord[]
  }
  sync_resolve_conflict: {
    args: { args: { opId: string; choice: "local" | "server" | "merged"; merged?: unknown } }
    result: null
  }
  device_info: { args: void; result: DeviceInfo }
  config_set_sync_server_url: { args: { url: string }; result: null }
  config_get_sync_server_url: { args: void; result: string | null }
  // Auth
  auth_login: {
    args: { args: { email: string; password: string; entity_id_hint?: string } }
    result: AuthLoginResult
  }
  auth_logout: { args: void; result: null }
  auth_current_user: { args: void; result: AuthUserContext | null }
  auth_lock: { args: void; result: null }
  auth_unlock: { args: { args: { password: string } }; result: null }
  auth_is_locked: { args: void; result: boolean }
  // Users
  users_list: { args: { args: { include_inactive?: boolean } }; result: UserAdminRecord[] }
  users_get: { args: { args: { id: string } }; result: UserAdminRecord }
  users_create: {
    args: { args: { email: string; name: string; role: UserRoleLiteral; password: string } }
    result: UserAdminRecord
  }
  users_update: {
    args: { args: { id: string; email?: string; name?: string; role?: UserRoleLiteral } }
    result: UserAdminRecord
  }
  users_soft_delete: { args: { args: { id: string } }; result: null }
  users_reset_password: { args: { args: { id: string; new_password: string } }; result: null }
  users_create_first_admin: {
    args: { args: { email: string; name: string; password: string; entity_id?: string } }
    result: UserAdminRecord
  }
  // Settings
  settings_list: { args: void; result: SettingRecord[] }
  settings_get: { args: { args: { key: string } }; result: SettingRecord | null }
  settings_update: {
    args: { args: { key: string; value: SettingValueWire } }
    result: SettingRecord
  }
}

export type UserRoleLiteral = "superadmin" | "receptionist" | "accountant"

export interface AuthUserContext {
  user_id: string
  entity_id: string
  email: string
  name: string | null
  role: string
}

export interface AuthLoginResult {
  mode: "online" | "offline"
  user: UserAdminRecord
}

export interface UserAdminRecord {
  id: string
  email: string
  name: string
  role: UserRoleLiteral
  is_active: boolean
  last_login_at: string | null
  created_at: string
  updated_at: string
  entity_id: string
  version: number
}

export type SettingValueWire =
  | { valueType: "int"; value: number }
  | { valueType: "decimal"; value: string }
  | { valueType: "text"; value: string }
  | { valueType: "bool"; value: boolean }

export interface SettingRecord {
  id: string
  key: string
  value: SettingValueWire
  updated_at: string
  version: number
  entity_id: string
}

export type SyncStatus = "idle" | "pushing" | "pulling" | "offline" | "error"

export interface SyncStatusSnapshot {
  status: SyncStatus
  pendingOps: number
  pending_ops?: number
}

export interface DeviceInfo {
  deviceId: string
  appVersion: string
  device_id?: string
  app_version?: string
}

export interface ConflictRecord {
  opId: string
  entity: string
  entityId: string
  serverPayload: unknown
  localPayload: unknown
  reason: string
}

export async function invoke<K extends keyof CommandMap>(
  command: K,
  ...rest: CommandMap[K]["args"] extends void ? [] : [CommandMap[K]["args"]]
): Promise<CommandMap[K]["result"]> {
  if (!isTauri()) {
    throw new Error(`IPC unavailable: not running inside Tauri (command=${command})`)
  }
  const args = (rest[0] ?? undefined) as Record<string, unknown> | undefined
  return await tauriInvoke<CommandMap[K]["result"]>(command, args)
}

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined"
  )
}

export async function listenEvent<T>(
  event: string,
  handler: (payload: T) => void
): Promise<UnlistenFn> {
  if (!isTauri()) {
    // No-op subscription when not in Tauri (e.g. Vite dev pageload).
    return async () => undefined
  }
  return await listen<T>(event, (e) => handler(e.payload))
}

export const SYNC_EVENTS = {
  STATUS: "sync:status",
  CONFLICT: "sync:conflict",
  PROGRESS: "sync:progress",
  AUTH_EXPIRED: "auth:session_expired",
} as const
