// Phase-09 §4 WebdriverIO selector library.
//
// We DO NOT pin tests to CSS classnames or DOM ordinals -- per
// `.claude/rules/testing.md` §14, "Brittle CSS-selector E2E" is a
// banned anti-pattern. Instead each spec resolves elements through
// one of:
//
//   1. Localized text content (the en/ar resource JSON is the source
//      of truth for visible copy and rarely drifts during a phase --
//      a renamed key surfaces in the i18n contract test before it
//      breaks an E2E spec).
//   2. ARIA role + accessible-name (button, dialog, alert, etc.).
//   3. data-testid attributes we add to load-bearing affordances as
//      the specs are exercised against the live binary.
//
// This module centralizes the text-content helpers so a phase-10
// copy rename touches one file instead of every spec.

import { browser } from "@wdio/globals"

export async function clickButtonWithText (text: string): Promise<void> {
  const btn = await browser.$(`//button[normalize-space()='${text}']`)
  await btn.waitForExist({ timeout: 10_000 })
  await btn.waitForClickable({ timeout: 10_000 })
  await btn.click()
}

export async function clickLinkWithText (text: string): Promise<void> {
  const link = await browser.$(`//a[normalize-space()='${text}']`)
  await link.waitForExist({ timeout: 10_000 })
  await link.waitForClickable({ timeout: 10_000 })
  await link.click()
}

export async function waitForText (text: string, timeout = 10_000): Promise<void> {
  await browser.waitUntil(
    async () => {
      const body = await browser.$("body")
      const html = await body.getHTML()
      return html.includes(text)
    },
    {
      timeout,
      timeoutMsg: `Expected text "${text}" to appear within ${timeout}ms`,
    },
  )
}

export async function fillInputByPlaceholder (
  placeholder: string,
  value: string,
): Promise<void> {
  const input = await browser.$(`input[placeholder='${placeholder}']`)
  await input.waitForExist({ timeout: 10_000 })
  await input.setValue(value)
}

export async function clickDialogConfirm (): Promise<void> {
  // Footer Save / Confirm / Submit button inside a role=dialog.
  const dialog = await browser.$("[role='dialog']")
  await dialog.waitForExist({ timeout: 10_000 })
  const submit = await dialog.$("button[type='submit']")
  await submit.waitForClickable({ timeout: 10_000 })
  await submit.click()
}
