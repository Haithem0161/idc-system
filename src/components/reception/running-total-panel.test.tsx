// Component-render assertions for the reception running-total panel.
//
// Runs in both ltr + rtl per `.claude/rules/testing.md` §14. The panel reads
// currency symbol + arabic-numeral preferences through `useMoneyDisplay`
// (-> useSettings -> invoke("settings_list")), so the harness mocks that one
// IPC call and supplies the rest via pure props.
//
// What this pins:
//   (a) hasPrice=false renders the em-dash placeholder, NO line items.
//   (b) hasPrice=true renders one row per line + the bold total testid.
//   (c) The total equals what is passed (panel does not re-derive money;
//       the page composes the patient total = price + dye from the
//       authoritative pricing_effective + settings).
//   (d) The configured currency symbol is shown next to the total.
//   (e) arabic_numerals=true renders Arabic-Indic digits in the total.
//   (f) `estimating` surfaces the pending hint; absent otherwise.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

import "@/i18n"

import i18n from "i18next"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
  }
})

import { invoke } from "@/lib/ipc"
import type { SettingRecord } from "@/lib/ipc"
import {
  RunningTotalPanel,
  type RunningTotalLine,
} from "@/components/reception/running-total-panel"

const directions = [["ltr"], ["rtl"]] as const

function setting (key: string, value: SettingRecord["value"]): SettingRecord {
  return {
    id: `01000000-0000-7000-8000-${key.slice(0, 12).padEnd(12, "0")}`,
    key,
    value,
    updated_at: "2026-05-19T07:00:00.000Z",
    version: 1,
    entity_id: "01923af0-7c1a-7000-8099-000000000099",
  }
}

function mockSettings (opts: { currency?: string; arabic?: boolean } = {}): void {
  const rows: SettingRecord[] = [
    setting("currency_symbol", { valueType: "text", value: opts.currency ?? "د.ع" }),
    setting("arabic_numerals", { valueType: "bool", value: opts.arabic ?? false }),
  ]
  ;(invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation(
    (cmd: string) => {
      if (cmd === "settings_list") return Promise.resolve(rows)
      return Promise.resolve(null)
    }
  )
}

function wrapper (): (props: { children: ReactNode }) => ReturnType<typeof createElement> {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  return ({ children }) =>
    createElement(QueryClientProvider, { client }, children)
}

// Patient-facing lines only: price + dye. The reporting-doctor share is an
// internal carve-out, never part of the patient total, so it is NOT a line here.
const LINES: RunningTotalLine[] = [
  { label: "CT Scan", amountIqd: 75000, emphasis: true },
  { label: "Dye", amountIqd: 10000 },
]

describe.each(directions)("RunningTotalPanel (dir=%s)", (dir) => {
  beforeEach(async () => {
    await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
    document.documentElement.dir = dir
    mockSettings()
  })
  afterEach(() => {
    vi.clearAllMocks()
  })

  it("renders the placeholder and no line items when hasPrice is false", () => {
    render(
      <RunningTotalPanel lines={[]} totalIqd={0} hasPrice={false} />,
      { wrapper: wrapper() }
    )
    expect(screen.getByTestId("running-total").textContent).toBe("—")
    expect(screen.queryByTestId("running-total-lines")).toBeNull()
  })

  it("renders one row per line and the total when priced", async () => {
    render(
      <RunningTotalPanel lines={LINES} totalIqd={85000} hasPrice />,
      { wrapper: wrapper() }
    )
    const list = await screen.findByTestId("running-total-lines")
    expect(list.querySelectorAll("li")).toHaveLength(2)
    await waitFor(() =>
      expect(screen.getByTestId("running-total").textContent).toBe("85,000")
    )
  })

  it("shows the configured currency symbol next to the total", async () => {
    mockSettings({ currency: "IQD" })
    render(
      <RunningTotalPanel lines={LINES} totalIqd={85000} hasPrice />,
      { wrapper: wrapper() }
    )
    await screen.findByText("IQD")
  })

  it("renders Arabic-Indic digits in the total when enabled", async () => {
    mockSettings({ arabic: true })
    render(
      <RunningTotalPanel lines={LINES} totalIqd={85000} hasPrice />,
      { wrapper: wrapper() }
    )
    await waitFor(() =>
      expect(screen.getByTestId("running-total").textContent).toBe("٨٥,٠٠٠")
    )
  })

  it("surfaces the estimating hint only when estimating is true", () => {
    const { rerender } = render(
      <RunningTotalPanel lines={[]} totalIqd={0} hasPrice={false} estimating />,
      { wrapper: wrapper() }
    )
    const estimatingLabel = i18n.t("reception.new_visit.estimating")
    expect(screen.getByText(estimatingLabel)).toBeTruthy()
    rerender(
      <RunningTotalPanel lines={[]} totalIqd={0} hasPrice={false} />
    )
    expect(screen.queryByText(estimatingLabel)).toBeNull()
  })
})
