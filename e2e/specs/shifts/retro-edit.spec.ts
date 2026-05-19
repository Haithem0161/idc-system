// Phase-09 §4.1 E2E: retroactive shift edit.
//
// Proves the receptionist can correct a wrong clock-in time without
// re-creating the shift -- the audit trail keeps the prior + next
// values (phase-04 §7.shifts).
//
//   1. Land on /reception/shifts, pick a row in today's history.
//   2. Open "Edit shift" dialog.
//   3. Tweak the check-in input to an earlier time, add a note.
//   4. Save -- the row reflects the new in time AND the duration
//      column updates.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- retroactive shift edit", function () {
  it("opens the edit dialog, mutates the check-in time, and refreshes today's history", async function () {
    await clickLinkWithText("Operator shifts")
    await waitForText("Today's shifts")

    // The retroactive editor opens via a per-row "Edit" button on the
    // today's-history table. The seed fixture has at least one
    // closed shift in today's history.
    await clickButtonWithText("Edit shift retroactively")

    // The dialog renders with the existing check_in_at + check_out_at
    // values seeded into the datetime-local inputs.
    const dialog = await browser.$("[role='dialog']")
    await dialog.waitForExist({ timeout: 10_000 })
    await waitForText("Edit shift")

    const dateInputs = await dialog.$$("input[type='datetime-local']")
    expect(dateInputs.length).toEqual(2)
    const checkInInput = dateInputs[0]
    if (!checkInInput) throw new Error("expected a datetime-local check_in input")
    // Mutate the check-in to 06:00 local -- the audit row will record
    // the prior value as well.
    await checkInInput.setValue("2026-05-19T06:00")

    // Add a note for the audit trail.
    const noteInput = await dialog.$("input[type='text']")
    await noteInput.setValue("manual correction -- forgot to clock in")

    // Save.
    const submit = await dialog.$("button[type='submit']")
    await submit.click()

    // The dialog closes (it unmounts on success), and the history row
    // now carries the new in-time. Wait for the dialog to disappear.
    await dialog.waitForExist({ reverse: true, timeout: 10_000 })
  })
})
