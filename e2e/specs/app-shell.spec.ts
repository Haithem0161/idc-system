// Phase-01 §4 E2E smoke: the binary boots, the webview renders a document,
// the document has a non-empty body. Anything more specific belongs to
// later phase E2E specs once phase-02 (auth) lands and a login screen
// exists. The plan's full §4 menu (offline / token-expiry / multi-device)
// stacks on top of this scaffold.

import { browser } from '@wdio/globals'
import { expect } from 'chai'

describe('Phase-01 app shell smoke', () => {
  it('opens the webview and renders a body element', async () => {
    const body = await browser.$('body')
    const exists = await body.isExisting()
    expect(exists).to.equal(true)
  })

  it('mounts the React root with non-empty HTML', async () => {
    const body = await browser.$('body')
    const html = await body.getHTML()
    // We don't pin exact contents -- phase-01 only proves Vite/React
    // mounted at all. Phase-02 onward authors content-specific assertions.
    expect(html.length).to.be.greaterThan(0)
  })
})
