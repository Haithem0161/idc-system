// DEF-007 G23: React Query hook tests for `useSettingsUpdateBatch`.
// Runs in both ltr + rtl per `.claude/rules/testing.md` §14 anti-pattern
// ("RTL never tested"). The mocked IPC layer matches the auth tests.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

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
  settingsKeys,
  useSettings,
  useSettingsUpdateBatch,
} from "@/features/settings/queries"

const directions = [["ltr"], ["rtl"]] as const

function makeWrapper(): {
  wrapper: (props: { children: ReactNode }) => ReturnType<typeof createElement>
  client: QueryClient
} {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children)
  return { wrapper, client }
}

function settingFixture(overrides: Partial<SettingRecord> = {}): SettingRecord {
  return {
    id: "0190a000-0000-7000-8000-000000000000",
    key: "arabic_numerals",
    value: { valueType: "bool", value: true },
    entity_id: "tenant-1",
    version: 2,
    updated_at: "2026-05-19T10:00:00.000Z",
    ...overrides,
  } as SettingRecord
}

describe.each(directions)("DEF-007 G23 useSettingsUpdateBatch (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    vi.mocked(invoke).mockReset()
  })
  afterEach(() => {
    document.documentElement.dir = ""
  })

  it("dispatches `settings_update_batch` with the entries array passed through", async () => {
    const fixtures = [
      settingFixture({ key: "arabic_numerals", value: { valueType: "bool", value: true } }),
      settingFixture({ key: "currency_symbol", value: { valueType: "text", value: "IQD" } }),
      settingFixture({ key: "idle_lock_minutes", value: { valueType: "int", value: 20 } }),
    ]
    vi.mocked(invoke).mockResolvedValueOnce(fixtures as never)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useSettingsUpdateBatch(), { wrapper })
    const entries = [
      { key: "arabic_numerals", value: { valueType: "bool" as const, value: true } },
      { key: "currency_symbol", value: { valueType: "text" as const, value: "IQD" } },
      { key: "idle_lock_minutes", value: { valueType: "int" as const, value: 20 } },
    ]
    const returned = await result.current.mutateAsync({ entries })
    expect(returned).toHaveLength(3)
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("settings_update_batch", {
      args: { entries },
    })
  })

  it("invalidates settingsKeys.all AND the per-key key for every returned row", async () => {
    const fixtures = [
      settingFixture({ key: "arabic_numerals" }),
      settingFixture({ key: "currency_symbol", value: { valueType: "text", value: "IQD" } }),
    ]
    vi.mocked(invoke).mockResolvedValueOnce(fixtures as never)
    const { wrapper, client } = makeWrapper()
    // Seed the cache so we can observe the invalidation flip.
    client.setQueryData(settingsKeys.all, [])
    client.setQueryData(settingsKeys.key("arabic_numerals"), settingFixture())
    client.setQueryData(
      settingsKeys.key("currency_symbol"),
      settingFixture({ key: "currency_symbol" }),
    )
    const { result } = renderHook(() => useSettingsUpdateBatch(), { wrapper })
    await result.current.mutateAsync({
      entries: [
        { key: "arabic_numerals", value: { valueType: "bool", value: true } },
        { key: "currency_symbol", value: { valueType: "text", value: "IQD" } },
      ],
    })
    expect(client.getQueryState(settingsKeys.all)?.isInvalidated).toBe(true)
    expect(client.getQueryState(settingsKeys.key("arabic_numerals"))?.isInvalidated).toBe(true)
    expect(client.getQueryState(settingsKeys.key("currency_symbol"))?.isInvalidated).toBe(true)
  })

  it("surfaces a validation rejection without invalidating any cache key", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "VALIDATION_ERROR",
      message: "idle_lock_minutes must be a positive integer",
    })
    const { wrapper, client } = makeWrapper()
    client.setQueryData(settingsKeys.all, [])
    const { result } = renderHook(() => useSettingsUpdateBatch(), { wrapper })
    await expect(
      result.current.mutateAsync({
        entries: [
          { key: "arabic_numerals", value: { valueType: "bool", value: true } },
          { key: "idle_lock_minutes", value: { valueType: "int", value: -1 } },
        ],
      }),
    ).rejects.toMatchObject({ code: "VALIDATION_ERROR" })
    expect(client.getQueryState(settingsKeys.all)?.isInvalidated).toBe(false)
  })

  it("integrates with useSettings: a successful batch causes the list query to refetch", async () => {
    // First call: initial useSettings fetch.
    vi.mocked(invoke).mockResolvedValueOnce([
      settingFixture({ value: { valueType: "bool", value: false } }),
    ] as never)
    const { wrapper, client } = makeWrapper()
    const { result: list } = renderHook(() => useSettings(), { wrapper })
    await waitFor(() => expect(list.current.isSuccess).toBe(true))

    // Second + third calls: the batch save, then the refetch the
    // invalidate triggers.
    vi.mocked(invoke).mockResolvedValueOnce([
      settingFixture({ value: { valueType: "bool", value: true } }),
    ] as never)
    vi.mocked(invoke).mockResolvedValueOnce([
      settingFixture({ value: { valueType: "bool", value: true } }),
    ] as never)
    const { result: batch } = renderHook(() => useSettingsUpdateBatch(), { wrapper })
    await batch.current.mutateAsync({
      entries: [{ key: "arabic_numerals", value: { valueType: "bool", value: true } }],
    })
    await waitFor(() => {
      const data = client.getQueryData<SettingRecord[]>(settingsKeys.all)
      expect(data?.[0].value).toEqual({ valueType: "bool", value: true })
    })
  })
})
