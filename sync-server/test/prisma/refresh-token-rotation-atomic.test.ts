// Phase-09 DEF-007 G33 -- server-side SIGKILL refresh-token rotation rollback.
//
// The brief: when refresh-token rotation is mid-flight (revoke old +
// create new) and the server is SIGKILL'd, the old token MUST stay
// valid -- Postgres rolls both writes back as a single transaction.
// The contract is "atomic rotation": the two writes MUST live inside
// the same `prisma.$transaction([...])` call so the database driver
// enforces all-or-nothing semantics.
//
// Driving an actual SIGKILL mid-transaction requires container
// orchestration outside this Node test harness. The sentinel here is
// static-source: the rotate method MUST use `$transaction` with both
// writes in the same array. A regression that swapped to two separate
// awaits (revoke then create) would create a window where the old
// token is revoked but the new one not yet issued -- a SIGKILL there
// would lock the user out until manual DB recovery.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import * as assert from 'node:assert/strict'

const userStorePath = join(
  __dirname,
  '..',
  '..',
  'src',
  'app',
  'auth',
  'infrastructure',
  'prisma',
  'user-store.ts',
)
const userStore = readFileSync(userStorePath, 'utf8')

test('DEF-007 G33: PrismaUserStore.rotate uses prisma.$transaction (atomic revoke + create)', () => {
  // Extract the body of the `rotate` method.
  const rotateMatch = userStore.match(
    /async rotate \(presentedToken: string, deviceId: string \| null(?:, expectedUserId\?: string)?\)\s*\{([\s\S]*?)\n {2}\}/,
  )
  assert.ok(rotateMatch, 'PrismaUserStore.rotate must be present')
  const body = rotateMatch[1]
  // The atomic contract: $transaction wraps both writes. A regression
  // that split them into two awaits would drop this token.
  assert.match(
    body,
    /\$transaction\(\s*\[/,
    'rotate must wrap writes in prisma.$transaction([...]) for atomicity',
  )
})

test('DEF-007 G33: rotate $transaction includes BOTH refreshToken.update + refreshToken.create', () => {
  const rotateMatch = userStore.match(
    /async rotate \(presentedToken: string, deviceId: string \| null(?:, expectedUserId\?: string)?\)\s*\{([\s\S]*?)\n {2}\}/,
  )
  const body = rotateMatch![1]
  // Extract the $transaction array. The body inside MUST contain both
  // a refreshToken.update (revoke the old) AND a refreshToken.create
  // (issue the new). A regression that dropped either would either
  // leak old tokens (no revoke) or fail to issue replacements
  // (no create).
  const txnMatch = body.match(/\$transaction\(\s*\[([\s\S]*?)\]\s*\)/)
  assert.ok(txnMatch, 'rotate must call prisma.$transaction with an array of writes')
  const txnBody = txnMatch[1]
  assert.match(
    txnBody,
    /refreshToken\.update\b/,
    'rotate $transaction must include refreshToken.update (revoke the old token)',
  )
  assert.match(
    txnBody,
    /refreshToken\.create\b/,
    'rotate $transaction must include refreshToken.create (issue the new token)',
  )
})

test('DEF-007 G33: rotate revokes the OLD token by id BEFORE creating the new one', () => {
  // Order matters in array transactions for some Prisma adapters --
  // we pin "revoke comes first" so a SIGKILL between operations
  // either leaves the old token live (rollback) or both write
  // (success). The reversed order would create a brief window
  // where TWO valid tokens exist for the same user.
  const rotateMatch = userStore.match(
    /async rotate \(presentedToken: string, deviceId: string \| null(?:, expectedUserId\?: string)?\)\s*\{([\s\S]*?)\n {2}\}/,
  )
  const body = rotateMatch![1]
  const txnMatch = body.match(/\$transaction\(\s*\[([\s\S]*?)\]\s*\)/)
  const txnBody = txnMatch![1]
  const updatePos = txnBody.indexOf('refreshToken.update')
  const createPos = txnBody.indexOf('refreshToken.create')
  assert.ok(updatePos >= 0 && createPos >= 0, 'both writes must be present')
  assert.ok(
    updatePos < createPos,
    'refreshToken.update (revoke) must come BEFORE refreshToken.create (issue)',
  )
})

test('DEF-007 G33: rotate revokes by setting revokedAt (not by row deletion)', () => {
  // The revoke side of the rotation sets `revokedAt` rather than
  // deleting the row. This preserves the audit trail (rotated
  // tokens stay queryable for forensic review) and lets the
  // `current.revokedAt !== null` guard in the next call reject
  // a replayed-old-token attack.
  const rotateMatch = userStore.match(
    /async rotate \(presentedToken: string, deviceId: string \| null(?:, expectedUserId\?: string)?\)\s*\{([\s\S]*?)\n {2}\}/,
  )
  const body = rotateMatch![1]
  assert.match(
    body,
    /data:\s*\{\s*revokedAt:\s*new Date\(\)\s*,?\s*\}/,
    'rotate must set revokedAt: new Date() (soft revoke, no row delete)',
  )
  // No raw DELETE on refresh-tokens in the rotation path.
  assert.doesNotMatch(
    body,
    /refreshToken\.delete\b|refreshToken\.deleteMany\b/,
    'rotate must NOT delete refresh-token rows (preserve audit trail)',
  )
})

test('DEF-007 G33: rotate rejects already-revoked tokens (no replay)', () => {
  // The pre-rotation guard at the start of the method rejects a
  // token whose revokedAt is non-null. This is the replay defense:
  // if an attacker captures an old refresh token after a successful
  // rotation, presenting it returns SESSION_EXPIRED rather than
  // issuing a new pair.
  const rotateMatch = userStore.match(
    /async rotate \(presentedToken: string, deviceId: string \| null(?:, expectedUserId\?: string)?\)\s*\{([\s\S]*?)\n {2}\}/,
  )
  const body = rotateMatch![1]
  assert.match(
    body,
    /current\.revokedAt\s*!==\s*null/,
    'rotate must reject when current.revokedAt !== null (replay defense)',
  )
  assert.match(
    body,
    /SESSION_EXPIRED/,
    'rejected replay must throw SESSION_EXPIRED domain error',
  )
})
