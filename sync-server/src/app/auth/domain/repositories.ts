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

  // `expectedUserId` (phase-10 T5): when set, the token must belong to this
  // subject (the `sub` of a presented access token) or the operation is
  // rejected with a 403 -- binding the refresh token to its owner. Optional so
  // the offline-first refresh path still works without an access token.
  rotate (presentedToken: string, deviceId: string | null, expectedUserId?: string): Promise<{
    id: string
    plaintextToken: string
    expiresAt: string
    userId: string
    entityIdTenant: string
  }>

  revokeByPlaintext (plaintextToken: string, expectedUserId?: string): Promise<void>
  revokeAllForUser (userId: string): Promise<void>
  loadRaw (id: string): Promise<RefreshTokenRecord | null>
}
