// Phase-09 DEF-007 G27 -- Prisma User<->OperatorShift back-relations sentinel.
//
// The phase-04 build cycle wired four named relations between `User` and
// `OperatorShift` so `prisma generate` succeeds:
//
//   1. User.shiftCheckIns  -> OperatorShift[]  @relation("ShiftCheckIn")
//   2. User.shiftCheckOuts -> OperatorShift[]  @relation("ShiftCheckOut")
//   3. OperatorShift.checkInByUser  -> User    @relation("ShiftCheckIn", ...)
//   4. OperatorShift.checkOutByUser -> User?   @relation("ShiftCheckOut", ...)
//
// Phase-09 DEF-007 G27 lists this as a deferred-feature regression
// sentinel: BLOCKER-7's docker-compose smoke test implicitly verifies
// the schema compiles (`prisma db push` succeeds), but no automated
// test catches a stealth removal of one of the back-relations -- which
// would surface only when a subsequent migration runs against the
// drifted schema. This sentinel pins the four relation declarations as
// a static-source contract.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import * as assert from 'node:assert/strict'

const schemaPath = join(__dirname, '..', '..', 'prisma', 'schema.prisma')
const schema = readFileSync(schemaPath, 'utf8')

test('DEF-007 G27: User model declares shiftCheckIns back-relation with @relation("ShiftCheckIn")', () => {
  assert.match(
    schema,
    /shiftCheckIns\s+OperatorShift\[\]\s+@relation\("ShiftCheckIn"\)/,
    'User.shiftCheckIns OperatorShift[] @relation("ShiftCheckIn") must be declared',
  )
})

test('DEF-007 G27: User model declares shiftCheckOuts back-relation with @relation("ShiftCheckOut")', () => {
  assert.match(
    schema,
    /shiftCheckOuts\s+OperatorShift\[\]\s+@relation\("ShiftCheckOut"\)/,
    'User.shiftCheckOuts OperatorShift[] @relation("ShiftCheckOut") must be declared',
  )
})

test('DEF-007 G27: OperatorShift.checkInByUser FK uses @relation("ShiftCheckIn") with onDelete: Restrict', () => {
  assert.match(
    schema,
    /checkInByUser\s+User\s+@relation\("ShiftCheckIn",\s*fields:\s*\[checkInByUserId\],\s*references:\s*\[id\],\s*onDelete:\s*Restrict\)/,
    'OperatorShift.checkInByUser must reference User via ShiftCheckIn with Restrict',
  )
})

test('DEF-007 G27: OperatorShift.checkOutByUser FK uses @relation("ShiftCheckOut") with onDelete: Restrict (nullable)', () => {
  // The shift may still be open (no check-out) so the FK is nullable.
  // The relation name MUST match the User side; a typo here is the
  // exact regression this sentinel guards.
  assert.match(
    schema,
    /checkOutByUser\s+User\?\s+@relation\("ShiftCheckOut",\s*fields:\s*\[checkOutByUserId\],\s*references:\s*\[id\],\s*onDelete:\s*Restrict\)/,
    'OperatorShift.checkOutByUser must reference User? via ShiftCheckOut with Restrict',
  )
})

test('DEF-007 G27: relation names are paired (both ShiftCheckIn ends present)', () => {
  // Prisma requires a back-relation for every named relation. A
  // common regression is dropping the User-side `shiftCheckIns` while
  // leaving the OperatorShift-side `checkInByUser` -- `prisma generate`
  // then fails with the cryptic "validation error: relation field
  // expects a back-relation" message. This test catches that BEFORE
  // generate runs.
  const checkInCount =
    (schema.match(/@relation\("ShiftCheckIn"/g) ?? []).length
  assert.equal(
    checkInCount, 2,
    `expected exactly 2 references to @relation("ShiftCheckIn") ` +
      `(one per end of the relation); found ${checkInCount}`,
  )
})

test('DEF-007 G27: relation names are paired (both ShiftCheckOut ends present)', () => {
  const checkOutCount =
    (schema.match(/@relation\("ShiftCheckOut"/g) ?? []).length
  assert.equal(
    checkOutCount, 2,
    `expected exactly 2 references to @relation("ShiftCheckOut") ` +
      `(one per end of the relation); found ${checkOutCount}`,
  )
})

test('DEF-007 G27: ShiftCheckIn and ShiftCheckOut relation names do not collide with other relations', () => {
  // Defense against a future refactor renaming a different relation to
  // a `ShiftCheckIn` / `ShiftCheckOut` literal (the names are case-
  // sensitive at the Prisma level, but a literal collision would
  // break code-generation in confusing ways).
  const allRelations = Array.from(
    schema.matchAll(/@relation\("([A-Za-z]+)"/g),
  ).map((m: RegExpMatchArray) => m[1])
  const shiftCheckIn = allRelations.filter((r) => r === 'ShiftCheckIn').length
  const shiftCheckOut = allRelations.filter((r) => r === 'ShiftCheckOut').length
  // Exactly 2 of each -- the User end + the OperatorShift end.
  assert.equal(shiftCheckIn, 2)
  assert.equal(shiftCheckOut, 2)
})
