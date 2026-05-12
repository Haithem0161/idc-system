import type { RefreshTokenRecord, UserRecord, UserRole } from './types'

export interface UserRepository {
  getByEmail (email: string, entityId: string): Promise<UserRecord | null>
  getById (id: string): Promise<UserRecord | null>
  create (input: {
    id: string
    email: string
    name: string
    passwordHash: string
    role: UserRole
    entityId: string
  }): Promise<UserRecord>
  updateLastLogin (id: string): Promise<void>
  updatePasswordHash (id: string, passwordHash: string): Promise<void>
  count (): Promise<number>
}

export interface RefreshTokenRepository {
  issue (input: {
    userId: string
    entityIdTenant: string
    deviceId: string | null
    ttlSeconds: number
  }): Promise<{ id: string, plaintextToken: string, expiresAt: string }>

  rotate (presentedToken: string, deviceId: string | null): Promise<{
    id: string
    plaintextToken: string
    expiresAt: string
    userId: string
    entityIdTenant: string
  }>

  revokeByPlaintext (plaintextToken: string): Promise<void>
  revokeAllForUser (userId: string): Promise<void>
  loadRaw (id: string): Promise<RefreshTokenRecord | null>
}
