import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { formatIqd } from "@/lib/format/money"
import { invoke, isTauri } from "@/lib/ipc"
import type { SettingRecord, SettingValueWire } from "@/lib/ipc"

export const settingsKeys = {
  all: ["settings", "all"] as const,
  key: (k: string) => ["settings", "key", k] as const,
}

export function useSettings () {
  return useQuery({
    queryKey: settingsKeys.all,
    enabled: isTauri(),
    queryFn: () => invoke("settings_list"),
    staleTime: 30_000,
  })
}

export function useSetting (key: string) {
  return useQuery({
    queryKey: settingsKeys.key(key),
    enabled: isTauri(),
    queryFn: () => invoke("settings_get", { args: { key } }),
  })
}

export function useSettingUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { key: string; value: SettingValueWire }) =>
      invoke("settings_update", { args: input }),
    onSuccess: (updated: SettingRecord) => {
      void qc.invalidateQueries({ queryKey: settingsKeys.all })
      void qc.invalidateQueries({ queryKey: settingsKeys.key(updated.key) })
    },
  })
}

// DEF-007 G23: atomic multi-key save. One IPC call mutates N keys in
// a single SQLite transaction; if any (key, value) fails validation,
// the entire batch rolls back and the user observes the pre-batch
// state for every key. The IPC emits a single `settings:changed` event
// with `{ keys: [...] }` so subscribers can invalidate caches in bulk.
export function useSettingsUpdateBatch () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { entries: Array<{ key: string; value: SettingValueWire }> }) =>
      invoke("settings_update_batch", { args: input }),
    onSuccess: (updated: SettingRecord[]) => {
      void qc.invalidateQueries({ queryKey: settingsKeys.all })
      for (const s of updated) {
        void qc.invalidateQueries({ queryKey: settingsKeys.key(s.key) })
      }
    },
  })
}

export function getSettingByKey (
  settings: SettingRecord[] | undefined,
  key: string
): SettingRecord | undefined {
  return settings?.find((s) => s.key === key)
}

export function settingValueAsNumber (s: SettingRecord | undefined, fallback: number): number {
  if (!s) return fallback
  if (s.value.valueType === "int") return s.value.value
  if (s.value.valueType === "decimal") {
    const n = Number(s.value.value)
    return Number.isFinite(n) ? n : fallback
  }
  return fallback
}

export function settingValueAsBool (s: SettingRecord | undefined, fallback: boolean): boolean {
  if (!s || s.value.valueType !== "bool") return fallback
  return s.value.value
}

export function settingValueAsText (s: SettingRecord | undefined, fallback = ""): string {
  if (!s) return fallback
  if (s.value.valueType === "text") return s.value.value
  return fallback
}

export interface MoneyDisplay {
  /** Currency symbol from settings (`currency_symbol`), default `د.ع`. */
  currencySymbol: string
  /** Render numbers in Arabic-Indic digits when the setting is on. */
  arabicNumerals: boolean
  /** Group a non-negative integer IQD amount, honoring the digit setting. */
  format: (amount: number) => string
}

/**
 * Resolve the clinic's money-display preferences (currency symbol + numeral
 * shape) from settings, plus a grouping formatter. Defaults mirror the seed
 * row defaults so the UI is correct even before settings load. Used anywhere a
 * money figure is shown to the user (running total, ledgers, receipts preview).
 */
export function useMoneyDisplay (): MoneyDisplay {
  const { data } = useSettings()
  const currencySymbol = settingValueAsText(
    getSettingByKey(data, "currency_symbol"),
    "د.ع"
  )
  const arabicNumerals = settingValueAsBool(
    getSettingByKey(data, "arabic_numerals"),
    false
  )
  return {
    currencySymbol,
    arabicNumerals,
    format: (amount: number) => formatIqd(amount, { arabicNumerals }),
  }
}
