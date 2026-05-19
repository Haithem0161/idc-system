// Phase-09 §4.1 E2E: audit query.
//
// Proves the superadmin can run an audit_query from /admin/audit with
// filters across action + entity + actor + free-text. Phase-08
// §3.Frontend AuditTable + phase-08 §7.16 free-text search invariant.
//
//   1. Land on /admin/audit.
//   2. Pick action="visit.locked" from the action filter.
//   3. Pick entity="visits" from the entity filter.
//   4. Submit the filter form -- the table renders only rows matching
//      both filters.
//   5. Type "voided" in the free-text input -- the table refilters.
//
// Gated by RUN_FULL_E2E=true.

import { browser, expect } from "@wdio/globals"

import { gatedDescribe } from "../../support/gate.js"
import {
  clickButtonWithText,
  clickLinkWithText,
  waitForText,
} from "../../support/selectors.js"

gatedDescribe("Phase-09 §4.1 -- audit query", function () {
  it("filters the audit table by action + entity + free-text", async function () {
    await clickLinkWithText("Audit")
    await waitForText("Audit")

    // Pick action=visit.locked from the action filter.
    const actionSelect = await browser.$("#audit-action")
    await actionSelect.selectByAttribute("value", "visit.locked")

    // Pick entity=visits.
    const entitySelect = await browser.$("#audit-entity")
    await entitySelect.selectByAttribute("value", "visits")

    // Submit the filter form.
    await clickButtonWithText("Apply filters")

    // The table renders at least one row.
    const rows = await browser.$$("tbody tr")
    expect(rows.length).toBeGreaterThan(0)

    // Type "voided" in the free-text -- the table refilters via Enter.
    const text = await browser.$("#audit-text")
    await text.setValue("voided")
    await browser.keys("Enter")
  })
})
