import { randomUUID } from 'node:crypto'
import { hash as argonHash, verify as argonVerify } from '@node-rs/argon2'

import { DomainError } from '../../common/errors/domain'
import type {
  RefreshTokenRepository,
  UserRepository,
} from '../domain/repositories'
import type { UserRecord, UserRole } from '../domain/types'

// Defaults match the documented lifetimes (access 15m, refresh 30d). The
// actual values are read from JWT_ACCESS_TTL_SECONDS / JWT_REFRESH_TTL_SECONDS
// at wiring time (auth-services plugin) and injected via the constructor; these
// constants are the fallback when no config is supplied (e.g. unit tests).
const DEFAULT_ACCESS_TOKEN_TTL_SEC = 15 * 60
const DEFAULT_REFRESH_TOKEN_TTL_SEC = 30 * 24 * 60 * 60

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
  private readonly accessTtlSec: number
  private readonly refreshTtlSec: number
  // Server-authoritative tenant for the single-clinic desktop flow: when a
  // bootstrap call omits entityId, the admin is stamped with this. Empty means
  // the caller MUST supply one (multi-tenant / misconfigured server).
  private readonly defaultEntityId: string

  constructor (
    private readonly users: UserRepository,
    private readonly tokens: RefreshTokenRepository,
    private readonly signer: TokenSigner,
    opts?: { accessTtlSec?: number; refreshTtlSec?: number; defaultEntityId?: string }
  ) {
    this.accessTtlSec = opts?.accessTtlSec ?? DEFAULT_ACCESS_TOKEN_TTL_SEC
    this.refreshTtlSec = opts?.refreshTtlSec ?? DEFAULT_REFRESH_TOKEN_TTL_SEC
    this.defaultEntityId = (opts?.defaultEntityId ?? '').trim()
  }

  /** True once any user exists -- drives the desktop first-launch decision. */
  async isInitialized (): Promise<boolean> {
    return (await this.users.count()) > 0
  }

  async login (
    email: string,
    password: string,
    entityId: string,
    deviceId: string | null
  ): Promise<LoginResult> {
    const normalized = email.trim().toLowerCase()
    // The tenant comes FROM the user, not the client. When the caller supplies
    // an explicit, real tenant we honor it (multi-tenant disambiguation); when
    // it is omitted/'unscoped' (the single-clinic desktop, which has no reason
    // to know the tenant UUID) we resolve the user by email alone and read the
    // tenant off their row. This is what lets "email + password" log in without
    // the client ever sending a tenant id.
    const explicit = entityId && entityId !== 'unscoped'
    const user = explicit
      ? await this.users.getByEmail(normalized, entityId)
      : (await this.users.findByEmail(normalized)) ?? (await this.users.getByEmail(normalized, 'unscoped'))
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

  /**
   * Rotate the refresh + access tokens.
   *
   * Phase-10 T5: when `expectedUserId` is supplied (the `sub` of a presented
   * access token, possibly expired), the rotation MUST belong to that subject.
   * This binds the refresh token to its owner so a leaked/mixed token cannot be
   * rotated under a different identity. It stays optional so the offline-first
   * refresh path still works when the client has no access token to present.
   */
  async refresh (refreshToken: string, deviceId: string | null, expectedUserId?: string) {
    const rotated = await this.tokens
      .rotate(refreshToken, deviceId, expectedUserId)
      .catch((err) => {
        // Preserve an explicit subject-mismatch (403) thrown by the store;
        // everything else collapses to a 401 session-expired.
        if (err instanceof DomainError) throw err
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
    // Phase-10 T11: defense-in-depth -- the refresh-token row carries its own
    // tenant; assert the loaded user belongs to that same tenant so a token can
    // never mint an access token scoped to a different entity.
    if (user.entityId !== rotated.entityIdTenant) {
      throw new DomainError('FORBIDDEN', 'refresh token tenant does not match user', 403)
    }
    const access = this.signer.sign(
      claimsFor(user),
      this.accessTtlSec
    )
    return {
      accessToken: access,
      refreshToken: rotated.plaintextToken,
      expiresAt: new Date(Date.now() + this.accessTtlSec * 1000).toISOString(),
    }
  }

  /**
   * Revoke a refresh token. Phase-10 T5: when `expectedUserId` is supplied, the
   * token must belong to that subject or the revoke is rejected (403) -- a
   * leaked token cannot be used to force-logout a different user.
   */
  async logout (refreshToken: string, expectedUserId?: string): Promise<void> {
    await this.tokens.revokeByPlaintext(refreshToken, expectedUserId)
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

  /**
   * Phase-10 T6: server-canonical identity for the authenticated subject. The
   * client calls this after login/refresh to confirm the server agrees on who
   * is logged in (the JWT claims alone are not ground truth -- a user may have
   * been deactivated server-side since the token was minted). Never returns the
   * password hash.
   */
  async getProfile (userId: string): Promise<{
    id: string
    email: string
    name: string
    role: string
    entityId: string
  }> {
    const user = await this.users.getById(userId)
    if (!user || !user.isActive) {
      throw new DomainError('NOT_FOUND', `user ${userId}`, 404)
    }
    return {
      id: user.id,
      email: user.email,
      name: user.name,
      role: user.role,
      entityId: user.entityId,
    }
  }

  async bootstrapSuperadmin (
    email: string,
    name: string,
    password: string,
    entityId: string | undefined,
    id?: string
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
    // The server owns tenancy: prefer an explicit entityId, else stamp the
    // configured default. If neither is present the server is misconfigured for
    // bootstrap -- fail loud rather than create an unscoped admin.
    const tenant = (entityId ?? '').trim() || this.defaultEntityId
    if (!tenant) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'no entityId supplied and DEFAULT_ENTITY_ID is not configured on the server',
        422
      )
    }
    const hashStr = await argonHash(password)
    return this.users.create({
      id: id ?? randomUUID(),
      email,
      name,
      passwordHash: hashStr,
      role: 'superadmin',
      entityId: tenant,
    })
  }

  private async issueTokenPair (user: UserRecord, deviceId: string | null): Promise<LoginResult> {
    const access = this.signer.sign(claimsFor(user), this.accessTtlSec)
    const issued = await this.tokens.issue({
      userId: user.id,
      entityIdTenant: user.entityId,
      deviceId,
      ttlSeconds: this.refreshTtlSec,
    })
    return {
      accessToken: access,
      refreshToken: issued.plaintextToken,
      expiresAt: new Date(Date.now() + this.accessTtlSec * 1000).toISOString(),
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
