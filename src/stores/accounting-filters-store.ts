import { create } from "zustand"
import { persist } from "zustand/middleware"

export type AccountingRangePreset =
  | "today"
  | "yesterday"
  | "last_7d"
  | "month"
  | "last_month"
  | "custom"

export interface AccountingFiltersState {
  preset: AccountingRangePreset
  /** ISO date (YYYY-MM-DD) in the user's local tz. */
  fromDate: string
  /** ISO date (YYYY-MM-DD) in the user's local tz, inclusive. */
  toDate: string
  includeVoided: boolean
  setPreset: (preset: AccountingRangePreset) => void
  setCustomRange: (from: string, to: string) => void
  setIncludeVoided: (v: boolean) => void
}

function todayLocal (): string {
  const d = new Date()
  // Local-tz YYYY-MM-DD (TZ-naive on purpose; the Rust layer converts
  // local-day to UTC via the fixed Asia/Baghdad offset per phase-07 §7.8).
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

function addDays (iso: string, days: number): string {
  const d = new Date(iso + "T00:00:00")
  d.setDate(d.getDate() + days)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

function presetRange (preset: AccountingRangePreset): { from: string; to: string } {
  const today = todayLocal()
  switch (preset) {
    case "today":
      return { from: today, to: today }
    case "yesterday": {
      const y = addDays(today, -1)
      return { from: y, to: y }
    }
    case "last_7d":
      return { from: addDays(today, -6), to: today }
    case "month":
      return { from: addDays(today, -29), to: today }
    case "last_month":
      return { from: addDays(today, -59), to: addDays(today, -30) }
    case "custom":
    default:
      return { from: today, to: today }
  }
}

/**
 * Accounting page filter state (phase-07 §3 Zustand). Persisted per device,
 * not synced (per-device preference, not shared across the org).
 */
export const useAccountingFiltersStore = create<AccountingFiltersState>()(
  persist(
    (set) => {
      const initial = presetRange("last_7d")
      return {
        preset: "last_7d",
        fromDate: initial.from,
        toDate: initial.to,
        includeVoided: false,
        setPreset: (preset) => {
          const r = presetRange(preset)
          set({ preset, fromDate: r.from, toDate: r.to })
        },
        setCustomRange: (from, to) => set({ preset: "custom", fromDate: from, toDate: to }),
        setIncludeVoided: (v) => set({ includeVoided: v }),
      }
    },
    { name: "idc.accounting.filters" }
  )
)

/**
 * Convert the store state to the UTC range the IPC commands expect. The
 * conversion is fixed at Asia/Baghdad (+03:00) per phase-07 §7.8; that
 * matches the Rust `baghdad_offset_seconds` and the sync-server default.
 */
export function rangeAsUtc (
  fromDate: string,
  toDateInclusive: string
): { from_utc: string; to_utc: string } {
  const baghdadOffsetMs = 3 * 3600 * 1000
  const startLocalUtc = new Date(fromDate + "T00:00:00Z").getTime() - baghdadOffsetMs
  // Exclusive upper bound = start of (toDate + 1) day.
  const dayAfter = addDays(toDateInclusive, 1)
  const endLocalUtc = new Date(dayAfter + "T00:00:00Z").getTime() - baghdadOffsetMs
  return {
    from_utc: new Date(startLocalUtc).toISOString(),
    to_utc: new Date(endLocalUtc).toISOString(),
  }
}
