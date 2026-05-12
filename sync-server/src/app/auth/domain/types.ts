export type UserRole = 'superadmin' | 'receptionist' | 'accountant'

export interface UserRecord {
  id: string
  email: string
  name: string
  passwordHash: string
  role: UserRole
  isActive: boolean
  entityId: string
  lastLoginAt: string | null
  createdAt: string
  updatedAt: string
  deletedAt: string | null
  version: number
}

export interface RefreshTokenRecord {
  id: string
  userId: string
  tokenHash: string
  entityIdTenant: string
  expiresAt: string
  revokedAt: string | null
  createdAt: string
  deviceId: string | null
}
