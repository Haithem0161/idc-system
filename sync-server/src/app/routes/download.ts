import { randomBytes } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { type FastifyPluginAsync } from 'fastify'

import { renderDownloadPage } from '../common/download-page.js'

// Brand logo, served as a same-origin asset (so the page's strict
// `img-src 'self'` CSP covers it without inlining ~86KB of base64 per render).
// The compiled app runs from /app (cwd) with dist/ in dist/ and the source tree
// alongside it (COPY . . in the image, bind-mounted in dev); the .webp lives in
// the source tree only, so resolve it from cwd. Read once at boot, cached.
const LOGO_PATH = join(process.cwd(), 'src/app/assets/logo.webp')

/**
 * Public download landing page, served by the sync server so it can sit behind
 * `idc-download.madebyhaithem.com` without a separate static host.
 *
 * The page is self-contained HTML (no build step, no external JS/CSS): it
 * fetches the live Tauri updater manifests (`latest.json`) from the releases
 * host at view time, so the version number and download links never go stale as
 * new releases ship. The releases host is configurable via `RELEASES_HOST`
 * (defaults to the production host).
 *
 * No auth: this is a public marketing/download surface. It exposes nothing
 * sensitive -- only the already-public release artifacts. A narrow,
 * nonce-based CSP is set per response (helmet leaves page CSP to the HTML
 * route), so the single inline script runs without opening `unsafe-inline`.
 */
const download: FastifyPluginAsync = async (fastify) => {
  const releasesHost = (process.env.RELEASES_HOST ?? 'idc-release.madebyhaithem.com').trim()

  // Load the brand logo once at boot. If it is missing for any reason the page
  // still renders (the <img> just 404s); we log and carry on rather than fail.
  let logo: Buffer | null = null
  try {
    logo = await readFile(LOGO_PATH)
  } catch (err) {
    fastify.log.warn({ err, path: LOGO_PATH }, 'download page: brand logo not found; serving without it')
  }

  if (logo) {
    fastify.get('/download/logo.webp', async (_request, reply) => {
      reply.header('cache-control', 'public, max-age=86400, immutable')
      reply.type('image/webp')
      return reply.send(logo)
    })
  }

  const handler = async (
    _request: unknown,
    reply: {
      header: (k: string, v: string) => unknown
      type: (t: string) => { send: (b: string) => unknown }
    }
  ): Promise<unknown> => {
    // Per-response nonce so the one inline <script> can run under a strict CSP
    // without resorting to 'unsafe-inline'.
    const nonce = randomBytes(16).toString('base64')
    const html = renderDownloadPage(releasesHost, nonce)

    reply.header(
      'content-security-policy',
      [
        "default-src 'none'",
        `script-src 'nonce-${nonce}'`,
        "style-src 'unsafe-inline'",
        "img-src 'self' data:",
        "font-src 'self'",
        `connect-src https://${releasesHost}`,
        "form-action 'none'",
        "frame-ancestors 'none'",
        "base-uri 'none'",
      ].join('; ')
    )
    // Short cache: the page is tiny and the live data is fetched client-side, so
    // a few minutes of edge/browser caching is fine and keeps the version fresh.
    reply.header('cache-control', 'public, max-age=300')
    return reply.type('text/html').send(html)
  }

  fastify.get('/download', handler)
  // Trailing-slash alias so the subdomain root path also resolves.
  fastify.get('/download/', handler)
}

export default download
