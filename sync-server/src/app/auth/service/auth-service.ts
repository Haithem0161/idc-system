import { randomUUID } from 'node:crypto'
import { hash as argonHash, verify as argonVerify } from '@node-rs/argon2'

import { DomainError } from '../../common/errors/domain'
import type {
  RefreshTokenRepository,
  UserRepository,
} from '../domain/repositories'
import type { UserRecord, UserRole } from '../domain/types'

const ACCESS_TOKEN_TTL_SEC = 15 * 60
const REFRESH_TOKEN_TTL_SEC = 30 * 24 * 60 * 60

export interface LoginResult {
  accessToken: string
  refreshToken: string
  expiresAt: string
  user: {
    id: string
    email: string
    name: string
    role: UserRole
    entityId: string
    passwordHash: string
  }
}

export interface TokenSigner {
  sign (payload: Record<string, unknown>, ttlSec: number): string
  verify (token: string): Record<string, unknown> | null
}

export class AuthService {
  constructor (
    private readonly users: UserRepository,
    private readonly tokens: RefreshTokenRepository,
    private readonly signer: TokenSigner
  ) {}

  async login (
    email: string,
    password: string,
    entityId: string,
    deviceId: string | null
  ): Promise<LoginResult> {
    const normalized = email.trim().toLowerCase()
    const user = await this.users.getByEmail(normalized, entityId)
    if (!user || !user.isActive) {
      throw new DomainError('NOT_AUTHENTICATED', 'invalid credentials', 401)
    }
    const ok = await argonVerify(user.passwordHash, password).catch(() => false)
    if (!ok) {
      throw new DomainError('NOT_AUTHENTICATED', 'invalid credentials', 401)
    }
    await this.users.updateLastLogin(user.id)
    return this.issueTokenPair(user, deviceId)
  }

  async refresh (refreshToken: string, deviceId: string | null) {
    const rotated = await this.tokens
      .rotate(refreshToken, deviceId)
      .catch((err) => {
        throw new DomainError(
          'SESSION_EXPIRED',
          'refresh token invalid or expired',
          401,
          { reason: (err as Error).message }
        )
      })
    const user = await this.users.getById(rotated.userId)
    if (!user || !user.isActive) {
      throw new DomainError('NOT_AUTHENTICATED', 'user not found or inactive', 401)
    }
    const access = this.signer.sign(
      claimsFor(user),
      ACCESS_TOKEN_TTL_SEC
    )
    return {
      accessToken: access,
      refreshToken: rotated.plaintextToken,
      expiresAt: new Date(Date.now() + ACCESS_TOKEN_TTL_SEC * 1000).toISOString(),
    }
  }

  async logout (refreshToken: string): Promise<void> {
    await this.tokens.revokeByPlaintext(refreshToken)
  }

  async changePassword (
    userId: string,
    oldPassword: string,
    newPassword: string
  ): Promise<void> {
    if (newPassword.length < 8) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'new password must be at least 8 characters',
        422
      )
    }
    const user = await this.users.getById(userId)
    if (!user) {
      throw new DomainError('NOT_FOUND', `user ${userId}`, 404)
    }
    const ok = await argonVerify(user.passwordHash, oldPassword).catch(() => false)
    if (!ok) {
      throw new DomainError('NOT_AUTHENTICATED', 'old password mismatch', 401)
    }
    const newHash = await argonHash(newPassword)
    await this.users.updatePasswordHash(user.id, newHash)
  }

  async bootstrapSuperadmin (
    email: string,
    name: string,
    password: string,
    entityId: string
  ): Promise<UserRecord> {
    if ((await this.users.count()) > 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'cannot bootstrap: users already exist',
        409
      )
    }
    if (password.length < 8) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'password must be at least 8 characters',
        422
      )
    }
    const hashStr = await argonHash(password)
    return this.users.create({
      id: randomUUID(),
      email,
      name,
      passwordHash: hashStr,
      role: 'superadmin',
      entityId,
    })
  }

  private async issueTokenPair (user: UserRecord, deviceId: string | null): Promise<LoginResult> {
    const access = this.signer.sign(claimsFor(user), ACCESS_TOKEN_TTL_SEC)
    const issued = await this.tokens.issue({
      userId: user.id,
      entityIdTenant: user.entityId,
      deviceId,
      ttlSeconds: REFRESH_TOKEN_TTL_SEC,
    })
    return {
      accessToken: access,
      refreshToken: issued.plaintextToken,
      expiresAt: new Date(Date.now() + ACCESS_TOKEN_TTL_SEC * 1000).toISOString(),
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        entityId: user.entityId,
        passwordHash: user.passwordHash,
      },
    }
  }
}

function claimsFor (user: UserRecord): Record<string, unknown> {
  return {
    sub: user.id,
    email: user.email,
    entityId: user.entityId,
    role: user.role,
  }
}
