// Render assertion for the receptionist price-override marker in the accounting
// source-visits table.
//
// The override (a receptionist collecting less than billed) must be visually
// distinct in accounting: the overridden row shows the "Override" badge and the
// collected amount, while a normal row shows neither. Runs in ltr + rtl per
// .claude/rules/testing.md.

import { render, screen } from "@testing-library/react"
import { MemoryRouter } from "react-router"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import i18n from "i18next"

import "@/i18n"
import { SourceVisitsTable } from "@/components/accounting/source-visits-table"
import type { VisitReportRowRecord } from "@/lib/ipc"

function row (over: Partial<VisitReportRowRecord> = {}): VisitReportRowRecord {
  return {
    visit_id: "0190f3a0-f1c0-7000-8000-0000000f0001",
    locked_at: "2026-06-19T10:00:00Z",
    status: "locked",
    patient_name: "Sara",
    check_type_name_ar: "ت",
    check_type_name_en: "Ultrasound",
    check_subtype_name_ar: null,
    check_subtype_name_en: null,
    doctor_name: "Dr Ali",
    operator_name: "Kareem",
    dye: false,
    report: false,
    price_iqd: 50_000,
    doctor_cut_iqd: 20_000,
    operator_cut_iqd: 4_000,
    total_iqd: 50_000,
    amount_paid_override_iqd: null,
    net_iqd: 26_000,
    ...over,
  }
}

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)("SourceVisitsTable (dir=%s)", (dir) => {
  beforeEach(async () => {
    document.documentElement.dir = dir
    await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
  })
  afterEach(() => {
    document.documentElement.dir = "ltr"
  })

  function renderRows (rows: VisitReportRowRecord[]) {
    return render(
      <MemoryRouter>
        <SourceVisitsTable rows={rows} locale={dir === "rtl" ? "ar-IQ" : "en-US"} emptyLabel="none" />
      </MemoryRouter>,
    )
  }

  it("shows the override badge + collected amount on an overridden row", () => {
    renderRows([
      row({ amount_paid_override_iqd: 30_000, net_iqd: 6_000 }),
    ])
    const badge = screen.getByText(i18n.t("accounting.visits.overridden"))
    expect(badge).toBeTruthy()
    // The collected amount (30,000) renders alongside the badge in the cell.
    // Locale-aware: en-US uses Latin digits (30), ar-IQ uses Arabic-Indic (٣٠).
    const cell = badge.closest("td")
    expect(cell).toBeTruthy()
    expect((cell as HTMLElement).textContent ?? "").toMatch(/30|٣٠/)
  })

  it("renders no override badge on a normal (non-overridden) row", () => {
    renderRows([row()])
    expect(screen.queryByText(i18n.t("accounting.visits.overridden"))).toBeNull()
  })
})
