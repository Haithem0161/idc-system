// Security hardening + download landing page (ship-readiness fixes):
//  - @fastify/helmet sets security headers on every response.
//  - @fastify/rate-limit caps abuse-sensitive routes (login).
//  - GET /download serves the self-contained HTML page with a nonce-based CSP.

import { test } from 'node:test'
import * as assert from 'node:assert'

import { build } from '../helper'

test('GET /download serves the HTML page with a nonce-based CSP that matches its script', async (t) => {
  const app = await build(t)
  const res = await app.inject({ url: '/download' })

  assert.strictEqual(res.statusCode, 200)
  assert.match(res.headers['content-type'] as string, /text\/html/)

  const csp = res.headers['content-security-policy'] as string
  assert.ok(csp, 'download page must set a Content-Security-Policy')
  assert.match(csp, /default-src 'none'/)
  // connect-src is limited to the releases host the page fetches manifests from.
  assert.match(csp, /connect-src https:\/\/idc-release\.madebyhaithem\.com/)

  // The CSP carries a script nonce, and the inline <script> uses that SAME
  // nonce -- so the script runs without 'unsafe-inline'.
  const m = /script-src 'nonce-([^']+)'/.exec(csp)
  assert.ok(m, 'CSP must carry a script nonce')
  const nonce = m![1]
  assert.ok(
    res.payload.includes(`<script nonce="${nonce}">`),
    'the inline script must carry the CSP nonce'
  )

  // It is the real download page, not a stub.
  assert.match(res.payload, /Download the desktop app/)
  assert.match(res.payload, /linux-x86_64/)
  // It prefers the first-time-installer manifest (install.json) and falls back
  // to the updater manifest (latest.json).
  assert.match(res.payload, /install\.json/)
  assert.match(res.payload, /latest\.json/)
})

test('GET /download/ (trailing slash) resolves too', async (t) => {
  const app = await build(t)
  const res = await app.inject({ url: '/download/' })
  assert.strictEqual(res.statusCode, 200)
})

test('every response carries helmet security headers', async (t) => {
  const app = await build(t)
  const res = await app.inject({ url: '/healthz' })
  assert.strictEqual(res.headers['x-content-type-options'], 'nosniff')
  // frameguard: deny
  assert.strictEqual(res.headers['x-frame-options'], 'DENY')
  // X-Powered-By is stripped (hidePoweredBy).
  assert.strictEqual(res.headers['x-powered-by'], undefined)
})

test('global rate-limit is active and /auth/login carries the strict per-route ceiling', async (t) => {
  const app = await build(t)

  // Global limiter is wired: every response advertises the standard headers.
  const health = await app.inject({ url: '/healthz' })
  assert.ok(
    health.headers['x-ratelimit-limit'] !== undefined,
    'global rate-limit must set x-ratelimit-limit on every response'
  )

  // The login route overrides the global ceiling with the strict anti-
  // credential-stuffing limit (5/min/IP). The advertised limit proves the
  // per-route policy is applied (429 enforcement under inject is exercised in
  // production by a real client IP; the policy wiring is what we assert here).
  const login = await app.inject({
    method: 'POST',
    url: '/auth/login',
    headers: { 'content-type': 'application/json' },
    payload: { email: 'nobody@example.com', password: 'wrong-password' },
  })
  assert.strictEqual(
    login.headers['x-ratelimit-limit'],
    '5',
    'login must advertise the strict 5/min per-route rate limit'
  )
})
