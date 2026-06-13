import { randomUUID, createHash, randomBytes } from 'node:crypto'
import type { PrismaClient } from '@prisma/client'

import { DomainError } from '../../../common/errors/domain'
import type {
  RefreshTokenRepository,
  UserRepository,
} from '../../domain/repositories'
import type {
  RefreshTokenRecord,
  UserRecord,
  UserRole,
} from '../../domain/types'

/**
 * Prisma-backed replacement for `MemoryUserStore`.
 *
 * Behaviour parity with the memory implementation plus:
 *   - `RefreshToken.tokenHash` `@unique` is the single source of truth — no
 *     parallel id↔hash map to drift (phase-09 §4 Refresh-token persistence).
 *   - Rotation is wrapped in a `$transaction([revoke, insert])` so there is
 *     never a window where neither token is valid.
 *   - Invalid / expired refresh tokens raise `DomainError` (401) rather than
 *     plain `Error`, so the global error handler returns the right code.
 */
export class PrismaUserStore implements UserRepository, RefreshTokenRepository {
  // Refresh-token TTL used by rotate(); defaults to 30d, overridable from
  // JWT_REFRESH_TTL_SECONDS at wiring time (previously hardcoded here).
  private readonly refreshTtlSec: number

  constructor (
    private readonly prisma: PrismaClient,
    refreshTtlSec: number = 60 * 60 * 24 * 30
  ) {
    this.refreshTtlSec = refreshTtlSec
  }

  async getByEmail (email: string, entityId: string): Promise<UserRecord | null> {
    const lower = email.trim().toLowerCase()
    const row = await this.prisma.user.findUnique({
      where: { user_email_unique: { entityId, email: lower } },
    })
    if (!row || row.deletedAt !== null) return null
    return toUserRecord(row)
  }

  async getById (id: string): Promise<UserRecord | null> {
    const row = await this.prisma.user.findUnique({ where: { id } })
    return row ? toUserRecord(row) : null
  }

  async create (input: {
    id: string
    email: string
    name: string
    passwordHash: string
    role: UserRole
    entityId: string
  }): Promise<UserRecord> {
    const now = new Date()
    const row = await this.prisma.user.create({
      data: {
        id: input.id,
        email: input.email.trim().toLowerCase(),
        name: input.name.trim(),
        passwordHash: input.passwordHash,
        role: input.role,
        isActive: true,
        entityId: input.entityId,
        createdAt: now,
        updatedAt: now,
        version: 1,
      },
    })
    return toUserRecord(row)
  }

  async updateLastLogin (id: string): Promise<void> {
    const now = new Date()
    await this.prisma.user.update({
      where: { id },
      data: { lastLoginAt: now, updatedAt: now },
    }).catch(() => undefined)
  }

  async updatePasswordHash (id: string, passwordHash: string): Promise<void> {
    await this.prisma.$transaction([
      this.prisma.user.update({
        where: { id },
        data: { passwordHash, updatedAt: new Date(), version: { increment: 1 } },
      }),
      this.prisma.refreshToken.updateMany({
        where: { userId: id, revokedAt: null },
        data: { revokedAt: new Date() },
      }),
    ])
  }

  async count (): Promise<number> {
    return await this.prisma.user.count({ where: { deletedAt: null } })
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
    const expiresAt = new Date(Date.now() + input.ttlSeconds * 1000)
    await this.prisma.refreshToken.create({
      data: {
        id,
        userId: input.userId,
        tokenHash: hash,
        entityIdTenant: input.entityIdTenant,
        expiresAt,
        deviceId: input.deviceId,
      },
    })
    return { id, plaintextToken: plaintext, expiresAt: expiresAt.toISOString() }
  }

  async rotate (presentedToken: string, deviceId: string | null) {
    const hash = sha256(presentedToken)
    const current = await this.prisma.refreshToken.findUnique({ where: { tokenHash: hash } })
    if (!current || current.revokedAt !== null) {
      throw new DomainError('SESSION_EXPIRED', 'invalid refresh token', 401)
    }
    if (current.expiresAt.getTime() < Date.now()) {
      throw new DomainError('SESSION_EXPIRED', 'expired refresh token', 401)
    }

    const plaintext = randomBytes(32).toString('hex')
    const newHash = sha256(plaintext)
    const newId = randomUUID()
    const ttlSeconds = this.refreshTtlSec
    const expiresAt = new Date(Date.now() + ttlSeconds * 1000)

    await this.prisma.$transaction([
      this.prisma.refreshToken.update({
        where: { id: current.id },
        data: { revokedAt: new Date() },
      }),
      this.prisma.refreshToken.create({
        data: {
          id: newId,
          userId: current.userId,
          tokenHash: newHash,
          entityIdTenant: current.entityIdTenant,
          expiresAt,
          deviceId,
        },
      }),
    ])

    return {
      id: newId,
      plaintextToken: plaintext,
      expiresAt: expiresAt.toISOString(),
      userId: current.userId,
      entityIdTenant: current.entityIdTenant,
    }
  }

  async revokeByPlaintext (plaintextToken: string): Promise<void> {
    const hash = sha256(plaintextToken)
    await this.prisma.refreshToken.updateMany({
      where: { tokenHash: hash, revokedAt: null },
      data: { revokedAt: new Date() },
    })
  }

  async revokeAllForUser (userId: string): Promise<void> {
    await this.prisma.refreshToken.updateMany({
      where: { userId, revokedAt: null },
      data: { revokedAt: new Date() },
    })
  }

  async loadRaw (id: string): Promise<RefreshTokenRecord | null> {
    const row = await this.prisma.refreshToken.findUnique({ where: { id } })
    return row ? toRefreshTokenRecord(row) : null
  }
}

function sha256 (input: string): string {
  return createHash('sha256').update(input).digest('hex')
}

function toUserRecord (r: {
  id: string
  email: string
  name: string
  passwordHash: string
  role: 'superadmin' | 'receptionist' | 'accountant'
  isActive: boolean
  entityId: string
  lastLoginAt: Date | null
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  version: number
}): UserRecord {
  return {
    id: r.id,
    email: r.email,
    name: r.name,
    passwordHash: r.passwordHash,
    role: r.role,
    isActive: r.isActive,
    entityId: r.entityId,
    lastLoginAt: r.lastLoginAt ? r.lastLoginAt.toISOString() : null,
    createdAt: r.createdAt.toISOString(),
    updatedAt: r.updatedAt.toISOString(),
    deletedAt: r.deletedAt ? r.deletedAt.toISOString() : null,
    version: r.version,
  }
}

function toRefreshTokenRecord (r: {
  id: string
  userId: string
  tokenHash: string
  entityIdTenant: string
  expiresAt: Date
  revokedAt: Date | null
  createdAt: Date
  deviceId: string | null
}): RefreshTokenRecord {
  return {
    id: r.id,
    userId: r.userId,
    tokenHash: r.tokenHash,
    entityIdTenant: r.entityIdTenant,
    expiresAt: r.expiresAt.toISOString(),
    revokedAt: r.revokedAt ? r.revokedAt.toISOString() : null,
    createdAt: r.createdAt.toISOString(),
    deviceId: r.deviceId,
  }
}
