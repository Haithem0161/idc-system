import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

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
