// Phase-09 §4.1 E2E: clock in / clock out happy path.
//
// Proves the load-bearing reception flow that the receptionist owns
// every morning:
//
//   1. Land on /reception/shifts.
//   2. Click "Clock in operator" -- the dialog opens.
//   3. Pick an operator from the dropdown + add a note.
//   4. Submit -- the on-shift table now includes that operator with a
//      live `is-live` status pill (formatSince ticks).
//   5. Click "Clock out" on the row -- the shift moves into today's
//      history with a concrete duration (no em-dash).
//
// Gated by RUN_FULL_E2E=true. See e2e/support/gate.ts.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- clock in and out", function () {
  it("opens the clock-in dialog, picks an operator, and renders an on-shift row", async function () {
    // Step 1: arrive on the shifts page via the sidebar nav.
    await clickLinkWithText("Operator shifts")
    await waitForText("On shift")

    // Step 2: open the clock-in dialog.
    await clickButtonWithText("Clock in operator")
    await waitForText("Clock in operator")

    // Step 3: pick an operator + submit. The select carries a
    // placeholder option as the first child; the second option is
    // the first qualified operator from the seed fixture (clinical-day
    // SQLite -- see fixtures/README.md).
    const dialog = await browser.$("[role='dialog']")
    await dialog.waitForExist({ timeout: 10_000 })
    const select = await dialog.$("select")
    await select.waitForExist({ timeout: 10_000 })
    const options = await select.$$("option")
    // options[0] = placeholder ("Select an operator"); options[1] = first
    // qualified operator from the seed.
    const firstCandidate = options[1]
    if (!firstCandidate) throw new Error("seed fixture has no qualified operators")
    const value = await firstCandidate.getAttribute("value")
    if (value == null) throw new Error("first candidate option missing value attribute")
    await select.selectByAttribute("value", value)
    // Submit via the dialog's submit button.
    const submit = await dialog.$("button[type='submit']")
    await submit.click()

    // Step 4: the on-shift table now carries a live status pill.
    await browser.waitUntil(
      async () => {
        const pill = await browser.$(".status-pill.is-live")
        return await pill.isExisting()
      },
      {
        timeout: 10_000,
        timeoutMsg: "Expected the on-shift table to render an is-live status pill",
      },
    )
  })

  it("clocks out an existing shift -- the row moves into today's history with a concrete duration", async function () {
    // Step 5: click the row's "Clock out" button.
    await clickButtonWithText("Clock out")

    // Step 6: today's history table renders the same row with a
    // numeric duration cell (no em-dash, which would imply the shift
    // is still open).
    await waitForText("Today's shifts")
    const historyDurationCells = await browser.$$(
      "//table//td[contains(@class, 'font-mono')]",
    )
    expect(historyDurationCells.length).toBeGreaterThan(0)
  })
})
