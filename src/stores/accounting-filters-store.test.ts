import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const STORAGE_KEY = "idc.accounting.filters"

function todayLocal (): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

describe("accounting filters store rehydration (H10: stale date range)", () => {
  beforeEach(() => {
    localStorage.clear()
    // The store is a module-level singleton; reset the module registry so each
    // case re-imports it and re-runs persist rehydration against its own seed.
    vi.resetModules()
  })
  afterEach(() => {
    localStorage.clear()
  })

  it("recomputes a preset range on rehydrate instead of using stale persisted dates", async () => {
    // Simulate a store persisted on a PREVIOUS day: preset 'today' but with
    // yesterday's absolute dates baked in.
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        state: {
          preset: "today",
          includeVoided: false,
          fromDate: "2020-01-01",
          toDate: "2020-01-01",
        },
        version: 0,
      }),
    )

    // Importing the store triggers persist rehydration.
    const { useAccountingFiltersStore } = await import("./accounting-filters-store")
    const s = useAccountingFiltersStore.getState()
    const today = todayLocal()
    expect(s.preset).toBe("today")
    expect(s.fromDate).toBe(today)
    expect(s.toDate).toBe(today)
  })

  it("keeps the absolute dates for a persisted custom range", async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        state: {
          preset: "custom",
          includeVoided: true,
          fromDate: "2026-03-01",
          toDate: "2026-03-15",
        },
        version: 0,
      }),
    )
    const { useAccountingFiltersStore } = await import("./accounting-filters-store")
    const s = useAccountingFiltersStore.getState()
    expect(s.preset).toBe("custom")
    expect(s.fromDate).toBe("2026-03-01")
    expect(s.toDate).toBe("2026-03-15")
    expect(s.includeVoided).toBe(true)
  })
})
