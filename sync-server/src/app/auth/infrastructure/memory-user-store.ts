import { randomUUID, createHash, randomBytes } from 'node:crypto'

import { DomainError } from '../../common/errors/domain'
import type {
  RefreshTokenRepository,
  UserRepository,
} from '../domain/repositories'
import type {
  RefreshTokenRecord,
  UserRecord,
  UserRole,
} from '../domain/types'

/**
 * In-memory user + refresh token store for Phase-2 development and tests.
 * Production swap-in: Prisma-backed implementation that follows the same
 * port contract.
 */
export class MemoryUserStore implements UserRepository, RefreshTokenRepository {
  // Test-introspectable. Production swap (PrismaUserStore) replaces this store
  // entirely; these Maps exist only on the in-memory test/dev path.
  readonly users = new Map<string, UserRecord>()
  readonly tokens = new Map<string, RefreshTokenRecord>()
  readonly tokenHashes = new Map<string, string>()

  // Refresh-token TTL used by rotate(); defaults to 30d, overridable from
  // JWT_REFRESH_TTL_SECONDS at wiring time (previously hardcoded here).
  private readonly refreshTtlSec: number

  constructor (refreshTtlSec: number = 60 * 60 * 24 * 30) {
    this.refreshTtlSec = refreshTtlSec
  }

  async getByEmail (email: string, entityId: string): Promise<UserRecord | null> {
    const lower = email.trim().toLowerCase()
    for (const u of this.users.values()) {
      if (u.entityId === entityId && u.email === lower && u.deletedAt === null) {
        return u
      }
    }
    return null
  }

  async getById (id: string): Promise<UserRecord | null> {
    return this.users.get(id) ?? null
  }

  async create (input: {
    id: string
    email: string
    name: string
    passwordHash: string
    role: UserRole
    entityId: string
  }): Promise<UserRecord> {
    const now = new Date().toISOString()
    const record: UserRecord = {
      id: input.id,
      email: input.email.trim().toLowerCase(),
      name: input.name.trim(),
      passwordHash: input.passwordHash,
      role: input.role,
      isActive: true,
      entityId: input.entityId,
      lastLoginAt: null,
      createdAt: now,
      updatedAt: now,
      deletedAt: null,
      version: 1,
    }
    this.users.set(record.id, record)
    return record
  }

  async updateLastLogin (id: string): Promise<void> {
    const u = this.users.get(id)
    if (!u) return
    u.lastLoginAt = new Date().toISOString()
    u.updatedAt = u.lastLoginAt
  }

  async updatePasswordHash (id: string, passwordHash: string): Promise<void> {
    const u = this.users.get(id)
    if (!u) return
    u.passwordHash = passwordHash
    u.version += 1
    u.updatedAt = new Date().toISOString()
    await this.revokeAllForUser(id)
  }

  async count (): Promise<number> {
    let n = 0
    for (const u of this.users.values()) {
      if (u.deletedAt === null) n += 1
    }
    return n
  }

  async issue (input: {
    userId: string
    entityIdTenant: string
    deviceId: string | null
    ttlSeconds: number
  }): Promise<{ id: string, plaintextToken: string, expiresAt: string }> {
    const plaintext = randomBytes(32).toString('hex')
    const hash = sha256(plaintext)
    const id = randomUUID()
    const expiresAt = new Date(Date.now() + input.ttlSeconds * 1000).toISOString()
    const record: RefreshTokenRecord = {
      id,
      userId: input.userId,
      tokenHash: hash,
      entityIdTenant: input.entityIdTenant,
      expiresAt,
      revokedAt: null,
      createdAt: new Date().toISOString(),
      deviceId: input.deviceId,
    }
    this.tokens.set(id, record)
    this.tokenHashes.set(hash, id)
    return { id, plaintextToken: plaintext, expiresAt }
  }

  async rotate (presentedToken: string, deviceId: string | null, expectedUserId?: string) {
    const hash = sha256(presentedToken)
    const id = this.tokenHashes.get(hash)
    const current = id ? this.tokens.get(id) : null
    if (!current || current.revokedAt !== null) {
      throw new DomainError('SESSION_EXPIRED', 'invalid refresh token', 401)
    }
    if (Date.parse(current.expiresAt) < Date.now()) {
      throw new DomainError('SESSION_EXPIRED', 'expired refresh token', 401)
    }
    // Phase-10 T5: bind to the presented subject when one is supplied.
    if (expectedUserId && current.userId !== expectedUserId) {
      throw new DomainError('FORBIDDEN', 'refresh token does not belong to the authenticated user', 403)
    }
    current.revokedAt = new Date().toISOString()

    const issued = await this.issue({
      userId: current.userId,
      entityIdTenant: current.entityIdTenant,
      deviceId,
      ttlSeconds: this.refreshTtlSec,
    })
    return {
      ...issued,
      userId: current.userId,
      entityIdTenant: current.entityIdTenant,
    }
  }

  async revokeByPlaintext (plaintextToken: string, expectedUserId?: string): Promise<void> {
    const hash = sha256(plaintextToken)
    const id = this.tokenHashes.get(hash)
    if (!id) return
    const t = this.tokens.get(id)
    if (!t) return
    // Phase-10 T5: when a subject is supplied, only revoke a token that belongs
    // to it. A leaked token presented under a different identity is a no-op.
    if (expectedUserId && t.userId !== expectedUserId) return
    t.revokedAt = new Date().toISOString()
  }

  async revokeAllForUser (userId: string): Promise<void> {
    for (const t of this.tokens.values()) {
      if (t.userId === userId && t.revokedAt === null) {
        t.revokedAt = new Date().toISOString()
      }
    }
  }

  async loadRaw (id: string): Promise<RefreshTokenRecord | null> {
    return this.tokens.get(id) ?? null
  }
}

function sha256 (input: string): string {
  return createHash('sha256').update(input).digest('hex')
}
