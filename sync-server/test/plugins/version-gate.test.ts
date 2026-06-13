import { test } from 'node:test'
import * as assert from 'node:assert'

import { compareVersions } from '../../src/app/plugins/version-gate.js'

test('compareVersions: equal versions', () => {
  assert.strictEqual(compareVersions('1.2.3', '1.2.3'), 0)
  assert.strictEqual(compareVersions('1.0', '1.0.0'), 0)
})

test('compareVersions: lower client', () => {
  assert.strictEqual(compareVersions('1.2.0', '1.2.1'), -1)
  assert.strictEqual(compareVersions('0.9.9', '1.0.0'), -1)
  assert.strictEqual(compareVersions('1.2', '1.10'), -1)
})

test('compareVersions: higher client', () => {
  assert.strictEqual(compareVersions('1.3.0', '1.2.9'), 1)
  assert.strictEqual(compareVersions('2.0.0', '1.9.9'), 1)
  assert.strictEqual(compareVersions('1.10', '1.2'), 1)
})

test('compareVersions: missing segments treated as 0', () => {
  assert.strictEqual(compareVersions('1', '1.0.0'), 0)
  assert.strictEqual(compareVersions('1.0.1', '1'), 1)
})
