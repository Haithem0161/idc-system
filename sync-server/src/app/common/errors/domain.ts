/**
 * Domain error codes (phase-01 §7.26).
 *
 * Maps onto `ErrorResponseSchema.code` and the i18n `errors:*` keys consumed
 * by the frontend. Subsequent phases extend this set as they add domain
 * logic.
 */
export type DomainCode =
  | 'NOT_AUTHENTICATED'
  | 'SESSION_EXPIRED'
  | 'VALIDATION_ERROR'
  | 'CONFLICT_PARKED'
  | 'NOT_FOUND'
  | 'NETWORK_OFFLINE'
  | 'SERVER_UNAVAILABLE'
  | 'DATABASE_ERROR'
  | 'CONFIGURATION_ERROR'
  | 'INTERNAL_ERROR'
  | 'UNSUPPORTED_OP'
  | 'AUDIT_IMMUTABLE'
  | 'ADDITIVE_VIOLATION'
  | 'ALREADY_RESOLVED'

export class DomainError extends Error {
  readonly code: DomainCode
  readonly status: number
  readonly details?: Record<string, unknown>

  constructor (code: DomainCode, message: string, status = 400, details?: Record<string, unknown>) {
    super(message)
    this.name = 'DomainError'
    this.code = code
    this.status = status
    this.details = details
  }
}
